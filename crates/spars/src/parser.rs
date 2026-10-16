use crate::chunks::left_edges;
use crate::config::ActionKind;
use crate::neural::TransitionTrace;
use crate::pipeline::best;
use crate::{
    neural::{Matrix, Scorer},
    Doc, Error, Model, Result, Span, TokenIndex,
};
impl Model {
    pub(crate) fn parse(&self, d: &mut Doc, x: &Matrix) -> Result<()> {
        self.parse_traced(d, x, None)
    }
    pub(crate) fn parse_traced(
        &self,
        d: &mut Doc,
        x: &Matrix,
        mut trace: Option<&mut Vec<TransitionTrace>>,
    ) -> Result<()> {
        let n = d.tokens.len();
        let mut scorer = Scorer::new(self, &self.config.parser, x);
        let actions = &self.config.parser.actions;
        let mut valid = Vec::with_capacity(actions.len());
        let mut stack = Vec::<usize>::new();
        let mut rebuffer = vec![];
        let mut at = 0;
        let mut heads: Vec<Option<usize>> = vec![None; n];
        let mut deps = vec!["ROOT".to_string(); n];
        let mut unshift = vec![false; n];
        let mut starts = vec![false; n];
        if n > 0 {
            starts[0] = true
        }
        let mut left: Vec<Vec<usize>> = vec![vec![]; n];
        let mut right = left.clone();
        let mut steps = 0;
        while at < n || !rebuffer.is_empty() || !stack.is_empty() {
            steps += 1;
            if steps > n.saturating_mul(n + 10).saturating_mul(4) {
                return Err(Error::Model("parser transition limit".into()));
            }
            let b = |i: usize| {
                if i < rebuffer.len() {
                    Some(rebuffer[rebuffer.len() - 1 - i])
                } else {
                    let k = at + i - rebuffer.len();
                    (k < n).then_some(k)
                }
            };
            let b0 = b(0);
            let b1 = b(1);
            let s0 = stack.last().copied();
            let s1 = stack.iter().rev().nth(1).copied();
            let s2 = stack.iter().rev().nth(2).copied();
            let child =
                |arcs: &Vec<Vec<usize>>, h: Option<usize>| h.and_then(|h| arcs[h].last().copied());
            let ids = [
                b0,
                b1,
                s0,
                s1,
                s2,
                child(&left, b0),
                child(&left, s0),
                child(&right, s0),
            ];
            let scores = scorer.scores(&ids);
            valid.clear();
            valid.extend((0..scores.len()).map(|i| {
                let action = &actions[i];
                match action.kind {
                    ActionKind::Shift => {
                        stack.is_empty()
                            || (b1.is_some() && !starts[b0.unwrap()] && !unshift[b0.unwrap()])
                    }
                    ActionKind::Reduce => !stack.is_empty(),
                    ActionKind::Left | ActionKind::Right => {
                        s0.is_some() && b0.is_some_and(|v| !starts[v])
                    }
                    ActionKind::Begin => {
                        b1.is_some_and(|v| Some(v) == b0.map(|u| u + 1) && !starts[v])
                    }
                    _ => false,
                }
            }));
            let a = best(scores, |i| valid[i])?;
            if let Some(t) = trace.as_mut() {
                t.push(TransitionTrace {
                    ids: ids.map(|v| v.map_or(-1, |i| i as i64)).to_vec(),
                    scores: scores.to_vec(),
                    valid: valid.clone(),
                    action: a,
                });
            }
            let action = &actions[a];
            match action.kind {
                ActionKind::Shift => {
                    let v = rebuffer.pop().unwrap_or_else(|| {
                        let v = at;
                        at += 1;
                        v
                    });
                    stack.push(v)
                }
                ActionKind::Reduce => {
                    let v = stack.pop().unwrap();
                    if heads[v].is_none() && !stack.is_empty() {
                        rebuffer.push(v);
                        unshift[v] = true
                    }
                }
                ActionKind::Left | ActionKind::Right => {
                    let (h, c) = if action.kind == ActionKind::Left {
                        (b0.unwrap(), s0.unwrap())
                    } else {
                        (s0.unwrap(), b0.unwrap())
                    };
                    if let Some(old) = heads[c] {
                        let arcs = if old > c {
                            &mut left[old]
                        } else {
                            &mut right[old]
                        };
                        arcs.retain(|v| *v != c)
                    }
                    heads[c] = Some(h);
                    deps[c] = action.label.clone();
                    if h > c {
                        left[h].push(c)
                    } else {
                        right[h].push(c)
                    }
                    if action.kind == ActionKind::Left {
                        unshift[b0.unwrap()] = false;
                        stack.pop();
                    } else {
                        let v = rebuffer.pop().unwrap_or_else(|| {
                            let v = at;
                            at += 1;
                            v
                        });
                        stack.push(v)
                    }
                }
                ActionKind::Begin => starts[b1.unwrap()] = true,
                _ => unreachable!(),
            }
        }
        for i in 0..n {
            d.tokens[i].head = Some(TokenIndex(heads[i].unwrap_or(i)));
            d.tokens[i].dep = Some(deps[i].clone());
        }
        // Pseudo-projective HEAD decoding, breadth first with ordered children.
        for i in 0..n {
            if let Some((dep, label)) = d.tokens[i].dep.clone().unwrap().split_once("||") {
                let old = d.tokens[i].head.unwrap().0;
                let mut queue = std::collections::VecDeque::from([old]);
                let mut visited = vec![false; n];
                let mut found = None;
                while let Some(h) = queue.pop_front() {
                    if visited[h] {
                        continue;
                    }
                    visited[h] = true;
                    for j in 0..n {
                        if j != h
                            && j != i
                            && d.tokens[j].head == Some(TokenIndex(h))
                            && !d
                                .token_text(TokenIndex(j))?
                                .chars()
                                .all(crate::tokenizer::is_space)
                        {
                            if d.tokens[j].dep.as_deref() == Some(label) {
                                found = Some(j);
                                break;
                            }
                            queue.push_back(j)
                        }
                    }
                    if found.is_some() {
                        break;
                    }
                }
                d.tokens[i].head = Some(TokenIndex(found.unwrap_or(old)));
                d.tokens[i].dep = Some(dep.into());
            }
        }
        let left_edges = left_edges(d);
        let mut sentence_starts = vec![];
        for (i, &left) in left_edges.iter().enumerate() {
            d.tokens[i].sentence_start = Some(false);
            if d.tokens[i].head == Some(TokenIndex(i)) {
                sentence_starts.push(left);
            }
        }
        sentence_starts.sort_unstable();
        sentence_starts.dedup();
        for &i in &sentence_starts {
            d.tokens[i].sentence_start = Some(true)
        }
        d.sentences = Some(
            sentence_starts
                .iter()
                .enumerate()
                .map(|(j, &i)| Span {
                    start: TokenIndex(i),
                    end: TokenIndex(*sentence_starts.get(j + 1).unwrap_or(&n)),
                    label: String::new(),
                    id: None,
                })
                .collect(),
        );
        Ok(())
    }
}
