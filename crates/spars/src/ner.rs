use crate::config::ActionKind;
use crate::neural::TransitionTrace;
use crate::pipeline::best;
use crate::{neural::Scorer, Doc, Model, Result, Span, TokenIndex};
impl Model {
    pub(crate) fn ner(&self, d: &mut Doc) -> Result<()> {
        self.ner_traced(d, None)
    }
    pub(crate) fn ner_traced(
        &self,
        d: &mut Doc,
        mut trace: Option<&mut Vec<TransitionTrace>>,
    ) -> Result<()> {
        let p = &self.config.ner;
        let x = self.encode(d, &p.tok2vec);
        let mut scorer = Scorer::new(self, &p.transition, &x);
        let actions = &p.transition.actions;
        let mut valid = Vec::with_capacity(actions.len());
        let mut open: Option<(usize, String)> = None;
        let mut ents = vec![];
        for i in 0..d.tokens.len() {
            let ids = [
                Some(i),
                open.as_ref().map(|(i, _)| *i),
                open.as_ref().map(|_| i - 1),
            ];
            let scores = scorer.scores(&ids);
            let space = d
                .token_text(TokenIndex(i))?
                .chars()
                .all(crate::tokenizer::is_space);
            let next = i + 1 < d.tokens.len();
            let boundary = next && d.tokens[i + 1].sentence_start == Some(true);
            valid.clear();
            valid.extend((0..scores.len()).map(|j| {
                let a = &actions[j];
                let label = a.label.as_str();
                match a.kind {
                    ActionKind::Begin => {
                        open.is_none() && next && !boundary && !space && !label.is_empty()
                    }
                    ActionKind::Inside => {
                        open.as_ref().is_some_and(|(_, l)| l == label) && next && !boundary
                    }
                    ActionKind::Left => open.as_ref().is_some_and(|(_, l)| l == label),
                    ActionKind::Unit => open.is_none() && !space && !label.is_empty(),
                    ActionKind::Outside => open.is_none(),
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
            let a = &actions[a];
            let label = a.label.as_str();
            let kind = a.kind;
            d.tokens[i].entity_iob = Some(
                if kind == ActionKind::Outside {
                    "O"
                } else if kind == ActionKind::Begin || kind == ActionKind::Unit {
                    "B"
                } else {
                    "I"
                }
                .into(),
            );
            d.tokens[i].entity_type = Some(label.into());
            match kind {
                ActionKind::Begin => open = Some((i, label.into())),
                ActionKind::Left => {
                    let (s, l) = open.take().unwrap();
                    ents.push(Span {
                        start: TokenIndex(s),
                        end: TokenIndex(i + 1),
                        label: l,
                        id: None,
                    })
                }
                ActionKind::Unit => ents.push(Span {
                    start: TokenIndex(i),
                    end: TokenIndex(i + 1),
                    label: label.into(),
                    id: None,
                }),
                _ => {}
            }
        }
        d.entities = Some(ents);
        Ok(())
    }
}
