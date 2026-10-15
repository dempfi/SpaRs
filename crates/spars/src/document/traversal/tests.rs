use super::*;
use crate::{Span, Token};

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

fn indices<'a>(tokens: impl Iterator<Item = TokenView<'a>>) -> Vec<usize> {
    tokens.map(|token| token.index().0).collect()
}

#[test]
fn roots_children_and_ancestors_exclude_self() {
    let doc = document(&[Some(1), Some(1), Some(1), Some(2)]);
    let root = doc.token(TokenIndex(1)).unwrap();
    assert_eq!(indices(root.children().unwrap()), [0, 2]);
    assert!(root.ancestors().unwrap().next().is_none());
    assert_eq!(
        indices(doc.token(TokenIndex(3)).unwrap().ancestors().unwrap()),
        [2, 1]
    );
    assert_eq!(indices(root.subtree().unwrap()), [0, 1, 2, 3]);
    let leaf = doc.token(TokenIndex(0)).unwrap();
    assert_eq!(leaf.children().unwrap().len(), 0);
    assert_eq!(indices(leaf.subtree().unwrap()), [0]);
}

#[test]
fn nonprojective_subtrees_follow_recursive_order_not_sorted_indices() {
    // Root 2 has left child 0, whose right child 3 crosses root 2.
    let doc = document(&[Some(2), Some(2), Some(2), Some(0)]);
    assert_eq!(
        indices(doc.token(TokenIndex(2)).unwrap().subtree().unwrap()),
        [0, 3, 1, 2]
    );
}

#[test]
fn missing_heads_bad_heads_and_cycles_fail_before_any_traversal() {
    let cases = [
        (
            vec![Some(0), None],
            DependencyError::MissingHead {
                token: TokenIndex(1),
            },
        ),
        (
            vec![Some(0), Some(2)],
            DependencyError::HeadOutOfBounds {
                token: TokenIndex(1),
                head: TokenIndex(2),
            },
        ),
        (
            vec![Some(0), Some(2), Some(1)],
            DependencyError::Cycle {
                token: TokenIndex(1),
            },
        ),
    ];
    for (heads, expected) in cases {
        let doc = document(&heads);
        let root = doc.token(TokenIndex(0)).unwrap();
        for error in [
            root.children().err(),
            root.ancestors().err(),
            root.subtree().err(),
        ] {
            assert!(matches!(error, Some(Error::Dependency(actual)) if actual == expected));
        }
        assert!(matches!(doc.dependency_index.get(), Some(Err(actual)) if *actual == expected));
    }
}

#[test]
fn sentence_access_distinguishes_missing_empty_and_gapped_annotations() {
    let mut doc = document(&[Some(0), Some(1), Some(2)]);
    assert!(doc.sentence_views().is_none());
    assert!(matches!(
        doc.token(TokenIndex(0)).unwrap().sentence(),
        Err(Error::MissingAnnotation("sentences"))
    ));
    doc.sentences = Some(vec![
        Span {
            start: TokenIndex(0),
            end: TokenIndex(1),
            label: String::new(),
            id: None,
        },
        Span {
            start: TokenIndex(2),
            end: TokenIndex(3),
            label: String::new(),
            id: None,
        },
    ]);
    assert_eq!(doc.sentence_views().unwrap().len(), 2);
    assert_eq!(
        doc.token(TokenIndex(2))
            .unwrap()
            .sentence()
            .unwrap()
            .start(),
        TokenIndex(2)
    );
    assert!(matches!(
        doc.token(TokenIndex(1)).unwrap().sentence(),
        Err(Error::InvalidSentence(TokenIndex(1)))
    ));
    let mut empty = document(&[]);
    empty.sentences = Some(vec![]);
    assert_eq!(empty.sentence_views().unwrap().len(), 0);
}

#[test]
fn cache_does_not_change_snapshots_equality_or_clone_results() {
    let doc = document(&[Some(1), Some(1), Some(1)]);
    let before = doc.clone();
    let json = doc.to_json().unwrap();
    assert!(doc.dependency_index.get().is_none());
    assert_eq!(
        indices(doc.token(TokenIndex(1)).unwrap().children().unwrap()),
        [0, 2]
    );
    let cache = doc.dependency_index().unwrap() as *const DependencyIndex;
    assert_eq!(
        cache,
        doc.dependency_index().unwrap() as *const DependencyIndex
    );
    assert_eq!(doc, before);
    assert_eq!(doc.to_json().unwrap(), json);
    let restored = Doc::from_json(&json).unwrap();
    assert!(restored.dependency_index.get().is_none());
    assert_eq!(restored, doc);
    assert_eq!(
        indices(doc.clone().token(TokenIndex(1)).unwrap().subtree().unwrap()),
        [0, 1, 2]
    );
}

#[test]
fn deep_trees_use_iteration_without_recursive_stack_growth() {
    let n: usize = 100_000;
    let heads: Vec<_> = (0..n).map(|i| Some(i.saturating_sub(1))).collect();
    let doc = document(&heads);
    assert_eq!(
        doc.token(TokenIndex(n - 1))
            .unwrap()
            .ancestors()
            .unwrap()
            .count(),
        n - 1
    );
    let mut count = 0;
    for (i, token) in doc
        .token(TokenIndex(0))
        .unwrap()
        .subtree()
        .unwrap()
        .enumerate()
    {
        assert_eq!(token.index().0, i);
        count += 1;
    }
    assert_eq!(count, n);
}

#[test]
fn concurrent_calls_share_an_immutable_cache() {
    let doc = document(&[Some(1), Some(1), Some(1)]);
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..8)
            .map(|_| {
                scope.spawn(|| {
                    let root = doc.token(TokenIndex(1)).unwrap();
                    assert_eq!(indices(root.children().unwrap()), [0, 2]);
                    assert_eq!(indices(root.subtree().unwrap()), [0, 1, 2]);
                    doc.dependency_index().unwrap()
                })
            })
            .collect();
        for handle in handles {
            assert!(std::ptr::eq(
                handle.join().unwrap(),
                doc.dependency_index().unwrap()
            ));
        }
    });
}

// Independent recursive definition for small forests; production traversal uses
// a cached child index and an explicit stack instead of these full token scans.
fn reference_subtree(doc: &Doc, node: usize, output: &mut Vec<usize>) {
    for i in 0..node {
        if doc.tokens[i].head == Some(TokenIndex(node)) {
            reference_subtree(doc, i, output);
        }
    }
    output.push(node);
    for i in node + 1..doc.tokens.len() {
        if doc.tokens[i].head == Some(TokenIndex(node)) {
            reference_subtree(doc, i, output);
        }
    }
}

#[test]
fn all_small_forests_match_an_independent_recursive_walk() {
    let mut valid = 0;
    for encoding in 0..4_usize.pow(4) {
        let mut number = encoding;
        let heads: Vec<_> = (0..4)
            .map(|_| {
                let head = number % 4;
                number /= 4;
                Some(head)
            })
            .collect();
        let doc = document(&heads);
        if doc.dependency_index().is_err() {
            continue;
        }
        valid += 1;
        for node in 0..4 {
            let mut expected = Vec::new();
            reference_subtree(&doc, node, &mut expected);
            assert_eq!(
                indices(doc.token(TokenIndex(node)).unwrap().subtree().unwrap()),
                expected
            );
        }
    }
    assert_eq!(valid, 125);
}
