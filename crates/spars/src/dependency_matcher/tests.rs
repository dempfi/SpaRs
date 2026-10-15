use super::*;
use crate::Token;
use std::sync::OnceLock;

fn doc(heads: &[usize]) -> Doc {
    let tokens = heads
        .iter()
        .enumerate()
        .map(|(i, &head)| {
            let mut token = Token::new(
                crate::ByteOffset(i),
                crate::ByteOffset(i + 1),
                crate::CodePointOffset(i),
                "x".into(),
            );
            token.head = Some(TokenIndex(head));
            token
        })
        .collect();
    Doc {
        text: "x".repeat(heads.len()),
        tokens,
        entities: None,
        sentences: None,
        noun_chunks: None,
        tensor: Vec::new(),
        dependency_index: OnceLock::new(),
    }
}
fn pattern(relation: Relation) -> DependencyPattern {
    DependencyPattern {
        nodes: vec![
            DependencyNode {
                id: "a".into(),
                constraints: Vec::new(),
                link: None,
            },
            DependencyNode {
                id: "b".into(),
                constraints: Vec::new(),
                link: Some(DependencyLink {
                    left: "a".into(),
                    relation,
                }),
            },
        ],
    }
}
fn pairs(relation: Relation) -> Vec<Vec<usize>> {
    let mut matcher = DependencyMatcher::new();
    matcher.add("r", vec![pattern(relation)]).unwrap();
    matcher
        .find_matches(&doc(&[1, 1, 1, 2]))
        .unwrap()
        .iter()
        .map(|m| m.tokens.iter().map(|i| i.0).collect())
        .collect()
}
#[test]
fn node_constraints_require_every_condition() {
    let mut document = doc(&[0, 0, 0]);
    document.tokens[0].lemma = Some("wanted".into());
    document.tokens[1].lemma = Some("other".into());
    document.tokens[2].lemma = Some("wanted".into());
    document.tokens[2].norm = "other".into();
    document.tokens[0].norm = "wanted".into();
    document.tokens[1].norm = "wanted".into();
    let mut matcher = DependencyMatcher::new();
    matcher
        .add(
            "both",
            vec![DependencyPattern {
                nodes: vec![DependencyNode {
                    id: "a".into(),
                    link: None,
                    constraints: [TokenAttribute::Norm, TokenAttribute::Lemma]
                        .into_iter()
                        .map(|attribute| TokenConstraint {
                            attribute,
                            predicate: Predicate::Equals {
                                value: "wanted".into(),
                            },
                        })
                        .collect(),
                }],
            }],
        )
        .unwrap();
    let matches = matcher.find_matches(&document).unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].tokens, vec![TokenIndex(0)]);
}
#[test]
fn all_relations_have_independent_tree_expectations() {
    use Relation::*;
    let cases: [(Relation, &[[usize; 2]]); 20] = [
        (Parent, &[[0, 1], [2, 1], [3, 2]]),
        (Child, &[[1, 0], [1, 2], [2, 3]]),
        (Ancestor, &[[0, 1], [2, 1], [3, 1], [3, 2]]),
        (Descendant, &[[1, 0], [1, 2], [1, 3], [2, 3]]),
        (ImmediatelyPrecedes, &[[0, 1], [1, 2], [2, 3]]),
        (Precedes, &[[0, 1], [0, 2], [0, 3], [1, 2], [1, 3], [2, 3]]),
        (ImmediatelyFollows, &[[1, 0], [2, 1], [3, 2]]),
        (Follows, &[[1, 0], [2, 0], [2, 1], [3, 0], [3, 1], [3, 2]]),
        (ImmediateRightSibling, &[[1, 2]]),
        (ImmediateLeftSibling, &[[1, 0]]),
        (RightSibling, &[[0, 2], [1, 2]]),
        (LeftSibling, &[[1, 0], [2, 0]]),
        (ImmediateRightChild, &[[1, 2], [2, 3]]),
        (ImmediateLeftChild, &[[1, 0]]),
        (RightChild, &[[1, 2], [2, 3]]),
        (LeftChild, &[[1, 0]]),
        (ImmediateRightParent, &[[0, 1]]),
        (ImmediateLeftParent, &[[2, 1], [3, 2]]),
        (RightParent, &[[0, 1]]),
        (LeftParent, &[[2, 1], [3, 2]]),
    ];
    for (relation, expected) in cases {
        assert_eq!(
            pairs(relation),
            expected.iter().map(|p| p.to_vec()).collect::<Vec<_>>(),
            "{relation:?}"
        );
        let json = serde_json::to_string(&relation).unwrap();
        assert_eq!(serde_json::from_str::<Relation>(&json).unwrap(), relation);
    }
}
#[test]
fn registration_is_atomic_and_preserves_rule_and_pattern_order() {
    let mut matcher = DependencyMatcher::new();
    matcher
        .add("first", vec![pattern(Relation::Child)])
        .unwrap();
    matcher
        .add("second", vec![pattern(Relation::Parent)])
        .unwrap();
    let mut invalid = pattern(Relation::Child);
    invalid.nodes[1].id = "a".into();
    assert!(matcher
        .add("first", vec![pattern(Relation::Parent), invalid.clone()])
        .is_err());
    assert!(matcher.add("new", vec![invalid]).is_err());
    assert_eq!(matcher.get("first").unwrap().len(), 1);
    assert!(!matcher.contains("new"));
    matcher
        .add("first", vec![pattern(Relation::Parent)])
        .unwrap();
    let output = matcher.find_matches(&doc(&[0, 0])).unwrap();
    assert_eq!(
        output
            .iter()
            .map(|m| (m.rule.as_str(), m.tokens.clone()))
            .collect::<Vec<_>>(),
        vec![
            ("first", vec![TokenIndex(0), TokenIndex(1)]),
            ("first", vec![TokenIndex(1), TokenIndex(0)]),
            ("second", vec![TokenIndex(1), TokenIndex(0)]),
        ]
    );
    matcher.remove("first").unwrap();
    assert!(matcher.remove("first").is_err());
    assert_eq!(matcher.len(), 1);
    matcher.remove("second").unwrap();
    assert!(matcher.is_empty());
}
#[test]
fn malformed_patterns_are_rejected() {
    let mut matcher = DependencyMatcher::new();
    assert!(matcher
        .add("r", vec![DependencyPattern { nodes: Vec::new() }])
        .is_err());
    let mut invalid = pattern(Relation::Child);
    invalid.nodes[0].link = invalid.nodes[1].link.clone();
    assert!(matcher.add("r", vec![invalid]).is_err());
    let mut invalid = pattern(Relation::Child);
    invalid.nodes[1].link = None;
    assert!(matcher.add("r", vec![invalid]).is_err());
    let mut invalid = pattern(Relation::Child);
    invalid.nodes[1].link.as_mut().unwrap().left = "future".into();
    assert!(matcher.add("r", vec![invalid]).is_err());
    assert!(matcher.is_empty());
    assert!(serde_json::from_str::<DependencyPattern>(r#"{"nodes":[],"ignored":true}"#).is_err());
    assert!(serde_json::from_str::<Relation>(r#""unknown""#).is_err());
}
#[test]
fn matches_allow_reused_tokens_and_preserve_repeated_calls() {
    let mut p = pattern(Relation::Child);
    p.nodes.push(DependencyNode {
        id: "c".into(),
        constraints: Vec::new(),
        link: Some(DependencyLink {
            left: "b".into(),
            relation: Relation::Parent,
        }),
    });
    let mut matcher = DependencyMatcher::new();
    matcher.add("r", vec![p]).unwrap();
    let tree = doc(&[0, 0]);
    let matches = matcher.find_matches(&tree).unwrap();
    assert_eq!(
        matches[0].tokens,
        vec![TokenIndex(0), TokenIndex(1), TokenIndex(0)]
    );
    assert!(matcher.find_matches(&doc(&[0])).unwrap().is_empty());
    assert_eq!(matcher.find_matches(&tree).unwrap(), matches);
}
#[test]
fn root_buckets_limit_position_relations_without_sentence_annotations() {
    let mut matcher = DependencyMatcher::new();
    matcher.add("r", vec![pattern(Relation::Precedes)]).unwrap();
    let output = matcher.find_matches(&doc(&[0, 0, 2, 2])).unwrap();
    assert_eq!(
        output.iter().map(|m| m.tokens.clone()).collect::<Vec<_>>(),
        vec![
            vec![TokenIndex(0), TokenIndex(1)],
            vec![TokenIndex(2), TokenIndex(3)]
        ]
    );
    // Interleaved components use first-matching-token order, not root index.
    let output = matcher.find_matches(&doc(&[2, 1, 2, 1])).unwrap();
    assert_eq!(
        output.iter().map(|m| m.tokens.clone()).collect::<Vec<_>>(),
        vec![
            vec![TokenIndex(0), TokenIndex(2)],
            vec![TokenIndex(1), TokenIndex(3)]
        ]
    );
}
#[test]
fn missing_heads_and_cycles_are_errors_but_empty_matcher_needs_no_annotations() {
    let mut tree = doc(&[0]);
    tree.tokens[0].head = None;
    let mut matcher = DependencyMatcher::new();
    assert!(matcher.find_matches(&tree).unwrap().is_empty());
    matcher.add("r", vec![pattern(Relation::Child)]).unwrap();
    assert!(matcher.find_matches(&tree).is_err());
    assert!(matcher.find_matches(&doc(&[1, 0])).is_err());
    assert!(matcher.find_matches(&doc(&[3])).is_err());
    assert!(matcher.find_matches(&doc(&[])).unwrap().is_empty());
}
#[test]
fn deep_patterns_and_trees_use_iterative_search() {
    let count: usize = 4096;
    let tree = doc(&(0..count)
        .map(|i| i.saturating_sub(1))
        .collect::<Vec<usize>>());
    let mut nodes = vec![DependencyNode {
        id: "0".into(),
        constraints: Vec::new(),
        link: None,
    }];
    for i in 1..count {
        nodes.push(DependencyNode {
            id: i.to_string(),
            constraints: Vec::new(),
            link: Some(DependencyLink {
                left: (i - 1).to_string(),
                relation: Relation::Child,
            }),
        });
    }
    // Distinct norms yield one complete deep match without a Cartesian product.
    let mut deep_tree = doc(&(0..1024_usize)
        .map(|i| i.saturating_sub(1))
        .collect::<Vec<_>>());
    for (i, token) in deep_tree.tokens.iter_mut().enumerate() {
        token.norm = i.to_string();
    }
    let mut deep_nodes = nodes[..1024].to_vec();
    for (i, node) in deep_nodes.iter_mut().enumerate() {
        node.constraints.push(TokenConstraint {
            attribute: TokenAttribute::Norm,
            predicate: Predicate::Equals {
                value: i.to_string(),
            },
        });
    }
    let mut deep_matcher = DependencyMatcher::new();
    deep_matcher
        .add("deep", vec![DependencyPattern { nodes: deep_nodes }])
        .unwrap();
    let matches = deep_matcher.find_matches(&deep_tree).unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(
        matches[0].tokens,
        (0..1024).map(TokenIndex).collect::<Vec<_>>()
    );
    // An unmatched deep pattern also returns without recursive calls.
    let mut matcher = DependencyMatcher::new();
    matcher.add("r", vec![DependencyPattern { nodes }]).unwrap();
    assert!(matcher.find_matches(&doc(&[0, 0])).unwrap().is_empty());
    let mut matcher = DependencyMatcher::new();
    matcher.add("r", vec![pattern(Relation::Child)]).unwrap();
    assert_eq!(matcher.find_matches(&tree).unwrap().len(), count - 1);
}

#[test]
fn selective_membership_equals_enumeration_for_every_relation_and_candidate_subset() {
    let tree = doc(&[1, 1, 1, 2]);
    let mut graph = Graph::new(&tree).unwrap();
    for operator in [
        "<", ">", "<<", ">>", ".", ".*", ";", ";*", "$+", "$-", "$++", "$--", ">+", ">-", ">++",
        ">--", "<+", "<-", "<++", "<--",
    ] {
        let relation: Relation = serde_json::from_value(serde_json::json!(operator)).unwrap();
        for left in 0..4 {
            let related = graph.resolve(left, relation).unwrap().to_vec();
            for mask in 0..16 {
                let candidates: Vec<_> = (0..4).filter(|i| mask & (1 << i) != 0).collect();
                let expected: Vec<_> = candidates
                    .iter()
                    .copied()
                    .filter(|i| related.contains(i))
                    .collect();
                let mut actual = Vec::new();
                graph
                    .select(left, relation, &candidates, &mut actual)
                    .unwrap();
                assert_eq!(actual, expected, "{operator}: {left}, mask={mask}");
            }
        }
    }
}
#[test]
fn lexical_flags_require_a_lexicon() {
    let mut matcher = DependencyMatcher::new();
    let flagged = DependencyPattern {
        nodes: vec![DependencyNode {
            id: "a".into(),
            constraints: vec![TokenConstraint {
                attribute: TokenAttribute::IsPunct,
                predicate: Predicate::Flag { value: false },
            }],
            link: None,
        }],
    };
    assert!(matches!(
        matcher.add("flag", vec![flagged]),
        Err(Error::Pattern(_))
    ));
    assert!(matcher.is_empty());
}
