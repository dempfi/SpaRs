use super::left_edges;
use crate::{Doc, Token, TokenIndex};

fn document(heads: &[Option<usize>]) -> Doc {
    Doc {
        text: "x".repeat(heads.len()),
        tokens: heads
            .iter()
            .enumerate()
            .map(|(i, head)| {
                let mut token = Token::new(
                    crate::ByteOffset(i),
                    crate::ByteOffset(i + 1),
                    crate::CodePointOffset(i),
                    "x".into(),
                );
                token.head = head.map(TokenIndex);
                token
            })
            .collect(),
        entities: None,
        sentences: None,
        noun_chunks: None,
        tensor: Vec::new(),
        dependency_index: Default::default(),
    }
}

// Independent reference: scan all children of every reachable token, as the
// original implementation did. Deliberately avoid the optimized graph walk.
fn reference_left_edge(doc: &Doc, start: usize) -> usize {
    let mut minimum = start;
    let mut stack = vec![start];
    let mut visited = vec![false; doc.tokens.len()];
    while let Some(head) = stack.pop() {
        if visited[head] {
            continue;
        }
        visited[head] = true;
        minimum = minimum.min(head);
        for (i, token) in doc.tokens.iter().enumerate() {
            if i != head && token.head == Some(TokenIndex(head)) {
                stack.push(i);
            }
        }
    }
    minimum
}

fn assert_reference(heads: &[Option<usize>]) {
    let doc = document(heads);
    let expected: Vec<usize> = (0..heads.len())
        .map(|i| reference_left_edge(&doc, i))
        .collect();
    assert_eq!(left_edges(&doc), expected, "heads: {heads:?}");
}

#[test]
fn forests_nonprojective_arcs_and_cycles_match_original_descendants() {
    assert_reference(&[]);
    assert_reference(&[None]);
    assert_reference(&[Some(0)]);
    assert_reference(&[Some(2), None, Some(2), Some(4), Some(4)]);
    assert_reference(&[Some(3), Some(4), Some(3), Some(3), Some(3)]);
    assert_reference(&[Some(3), Some(2), Some(1), Some(2), Some(5), Some(4)]);
    assert_reference(&[Some(1), Some(2), Some(0), Some(2), Some(3)]);
}

#[test]
fn every_small_functional_graph_matches_original_descendants() {
    // Enumerate every head assignment, including missing heads, through five
    // tokens. This includes forests, self roots, cycles and branches into cycles.
    for n in 0_u32..=5 {
        let choices = n as usize + 1;
        for mut assignment in 0..choices.pow(n) {
            let mut heads = Vec::with_capacity(n as usize);
            for _ in 0..n {
                let head = assignment % choices;
                heads.push((head < n as usize).then_some(head));
                assignment /= choices;
            }
            assert_reference(&heads);
        }
    }
}

#[test]
fn long_chains_and_cycles_use_bounded_iterative_storage() {
    let n = 100_000;
    let mut heads: Vec<Option<usize>> = (1..=n).map(Some).collect();
    heads[n - 1] = Some(n - 1);
    assert_eq!(left_edges(&document(&heads)), vec![0; n]);
    heads[n - 1] = Some(0);
    assert_eq!(left_edges(&document(&heads)), vec![0; n]);
    for (i, head) in heads.iter_mut().enumerate() {
        *head = Some(i.saturating_sub(1));
    }
    assert_eq!(left_edges(&document(&heads)), (0..n).collect::<Vec<_>>());
}
