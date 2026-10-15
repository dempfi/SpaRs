use crate::{Doc, Span, TokenIndex};
/// Minimum descendant index for every token in a functional head graph.
/// Each token has at most one parent. Remove leaves first, propagating minima
/// toward parents. Any remaining nodes form disjoint cycles; every node in a
/// cycle reaches the same descendants and therefore receives the same minimum.
/// Both phases visit each node at most twice, using O(n) time and storage.
/// Heads must be in bounds, as guaranteed by parsing and document validation.
pub(crate) fn left_edges(d: &Doc) -> Vec<usize> {
    let n = d.tokens.len();
    let mut minimum: Vec<usize> = (0..n).collect();
    let mut children = vec![0_usize; n];
    for (i, token) in d.tokens.iter().enumerate() {
        if let Some(TokenIndex(head)) = token.head {
            if head != i {
                children[head] += 1;
            }
        }
    }
    let mut leaves: Vec<usize> = children
        .iter()
        .enumerate()
        .filter_map(|(i, &count)| (count == 0).then_some(i))
        .collect();
    while let Some(i) = leaves.pop() {
        if let Some(TokenIndex(head)) = d.tokens[i].head {
            if head != i {
                minimum[head] = minimum[head].min(minimum[i]);
                children[head] -= 1;
                if children[head] == 0 {
                    leaves.push(head);
                }
            }
        }
    }
    for start in 0..n {
        if children[start] == 0 {
            continue;
        }
        let mut cycle_minimum = minimum[start];
        let mut current = start;
        loop {
            cycle_minimum = cycle_minimum.min(minimum[current]);
            current = d.tokens[current].head.expect("remaining cycle node").0;
            if current == start {
                break;
            }
        }
        loop {
            minimum[current] = cycle_minimum;
            children[current] = 0;
            current = d.tokens[current].head.expect("remaining cycle node").0;
            if current == start {
                break;
            }
        }
    }
    minimum
}
pub(crate) fn chunks(d: &mut Doc) {
    let labels = [
        "oprd",
        "nsubj",
        "dobj",
        "nsubjpass",
        "pcomp",
        "pobj",
        "dative",
        "appos",
        "attr",
        "ROOT",
    ];
    let left_edges = left_edges(d);
    let mut result = vec![];
    let mut end = 0;
    for (i, &left) in left_edges.iter().enumerate() {
        let t = &d.tokens[i];
        if !["NOUN", "PROPN", "PRON"].contains(&t.pos.as_deref().unwrap_or("")) {
            continue;
        }
        if left < end {
            continue;
        }
        let dep = t.dep.as_deref().unwrap_or("");
        let mut include = labels.contains(&dep);
        if dep == "conj" {
            let mut h = t.head.unwrap().0;
            for _ in 0..d.tokens.len() {
                let t = &d.tokens[h];
                if t.dep.as_deref() != Some("conj") || t.head.unwrap().0 >= h {
                    break;
                }
                h = t.head.unwrap().0;
            }
            include = labels.contains(&d.tokens[h].dep.as_deref().unwrap_or(""))
        }
        if include {
            end = i + 1;
            result.push(Span {
                start: TokenIndex(left),
                end: TokenIndex(end),
                label: "NP".into(),
                id: None,
            })
        }
    }
    d.noun_chunks = Some(result);
}

#[cfg(test)]
mod tests;
