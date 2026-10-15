use super::*;
use crate::Token;
use std::sync::OnceLock;

fn doc(text: &str) -> Doc {
    Doc {
        text: text.into(),
        tokens: text
            .char_indices()
            .enumerate()
            .map(|(cp, (i, c))| {
                Token::new(
                    crate::ByteOffset(i),
                    crate::ByteOffset(i + c.len_utf8()),
                    crate::CodePointOffset(cp),
                    c.to_string(),
                )
            })
            .collect(),
        entities: None,
        sentences: None,
        noun_chunks: None,
        tensor: Vec::new(),
        dependency_index: OnceLock::new(),
    }
}
fn pattern(repetitions: &[Repetition]) -> TokenPattern {
    TokenPattern {
        tokens: repetitions
            .iter()
            .map(|&repetition| TokenPatternItem {
                constraints: Vec::new(),
                repetition,
            })
            .collect(),
    }
}
fn spans(matcher: &TokenMatcher, text: &str) -> Vec<(String, usize, usize)> {
    matcher
        .find_matches(&doc(text))
        .unwrap()
        .into_iter()
        .map(|m| (m.rule, m.start.0, m.end.0))
        .collect()
}
#[test]
fn repeated_registration_keeps_global_pattern_order_and_deduplicates() {
    let mut matcher = TokenMatcher::new();
    matcher
        .add("a", vec![pattern(&[Repetition::Once, Repetition::Once])])
        .unwrap();
    matcher
        .add("b", vec![pattern(&[Repetition::Once])])
        .unwrap();
    matcher
        .add(
            "a",
            vec![pattern(&[Repetition::Once]), pattern(&[Repetition::Once])],
        )
        .unwrap();
    assert_eq!(
        spans(&matcher, "xx"),
        [
            ("b", 0, 1),
            ("a", 0, 1),
            ("a", 0, 2),
            ("b", 1, 2),
            ("a", 1, 2)
        ]
        .map(|(r, s, e)| (r.into(), s, e))
    );
    assert_eq!(matcher.get("a").unwrap().len(), 3);
    matcher.remove("a").unwrap();
    assert_eq!(spans(&matcher, "x"), vec![("b".into(), 0, 1)]);
}
#[test]
fn star_emits_delayed_matches_and_final_flush_in_reference_order() {
    let mut matcher = TokenMatcher::new();
    matcher
        .add("r", vec![pattern(&[Repetition::ZeroOrMore])])
        .unwrap();
    assert_eq!(
        spans(&matcher, "xxx"),
        [(0, 1), (0, 2), (1, 2), (0, 3), (1, 3), (2, 3)].map(|(s, e)| ("r".into(), s, e))
    );
    assert!(spans(&matcher, "").is_empty());
}
#[test]
fn optional_paths_deduplicate_and_never_emit_empty_spans() {
    let mut matcher = TokenMatcher::new();
    matcher
        .add(
            "r",
            vec![pattern(&[Repetition::Optional, Repetition::Optional])],
        )
        .unwrap();
    assert_eq!(
        spans(&matcher, "xx"),
        [(0, 1), (0, 2), (1, 2)].map(|(s, e)| ("r".into(), s, e))
    );
}
#[test]
fn registration_is_atomic_and_ranges_are_checked() {
    let mut matcher = TokenMatcher::new();
    for repetition in [
        Repetition::Range {
            min: 2,
            max: Some(1),
        },
        Repetition::Range {
            min: usize::MAX,
            max: None,
        },
    ] {
        assert!(matcher
            .add(
                "bad",
                vec![pattern(&[Repetition::Once]), pattern(&[repetition])]
            )
            .is_err());
        assert!(matcher.is_empty());
    }
    assert!(matcher.add("bad", vec![pattern(&[])]).is_err());
    matcher
        .add(
            "zero",
            vec![pattern(&[Repetition::Range {
                min: 0,
                max: Some(0),
            }])],
        )
        .unwrap();
    assert!(spans(&matcher, "xx").is_empty());
    assert!(matcher.remove("missing").is_err());
}
#[test]
fn negation_consumes_one_token_and_missing_annotations_are_errors() {
    let mut matcher = TokenMatcher::new();
    matcher
        .add(
            "r",
            vec![TokenPattern {
                tokens: vec![TokenPatternItem {
                    constraints: vec![TokenConstraint {
                        attribute: crate::TokenAttribute::Text,
                        predicate: crate::Predicate::Equals { value: "x".into() },
                    }],
                    repetition: Repetition::Negated,
                }],
            }],
        )
        .unwrap();
    assert_eq!(spans(&matcher, "xy"), vec![("r".into(), 1, 2)]);
    matcher
        .add(
            "lemma",
            vec![TokenPattern {
                tokens: vec![TokenPatternItem {
                    constraints: vec![TokenConstraint {
                        attribute: crate::TokenAttribute::Lemma,
                        predicate: crate::Predicate::Equals { value: "z".into() },
                    }],
                    repetition: Repetition::Optional,
                }],
            }],
        )
        .unwrap();
    assert!(matches!(
        matcher.find_matches(&doc("x")),
        Err(Error::MissingAnnotation(_))
    ));
}

#[test]
fn many_optional_nodes_with_failing_tail_have_bounded_states() {
    let mut matcher = TokenMatcher::new();
    let mut p = pattern(&vec![Repetition::Optional; 64]);
    p.tokens.push(TokenPatternItem {
        constraints: vec![TokenConstraint {
            attribute: crate::TokenAttribute::Text,
            predicate: crate::Predicate::Equals { value: "z".into() },
        }],
        repetition: Repetition::Once,
    });
    matcher.add("r", vec![p]).unwrap();
    assert!(spans(&matcher, &"x".repeat(96)).is_empty());
}
#[test]
fn removed_zero_repeat_items_do_not_require_annotations() {
    let mut matcher = TokenMatcher::new();
    matcher
        .add(
            "r",
            vec![TokenPattern {
                tokens: vec![
                    TokenPatternItem {
                        constraints: vec![TokenConstraint {
                            attribute: crate::TokenAttribute::Lemma,
                            predicate: crate::Predicate::Equals { value: "z".into() },
                        }],
                        repetition: Repetition::Range {
                            min: 0,
                            max: Some(0),
                        },
                    },
                    TokenPatternItem {
                        constraints: Vec::new(),
                        repetition: Repetition::Once,
                    },
                ],
            }],
        )
        .unwrap();
    assert_eq!(spans(&matcher, "x"), vec![("r".into(), 0, 1)]);
}

#[test]
fn discarded_items_still_validate_predicates() {
    let mut matcher = TokenMatcher::new();
    let invalid = TokenPattern {
        tokens: vec![TokenPatternItem {
            repetition: Repetition::Range {
                min: 0,
                max: Some(0),
            },
            constraints: vec![TokenConstraint {
                attribute: crate::TokenAttribute::Text,
                predicate: crate::Predicate::MorphSuperset { values: vec![] },
            }],
        }],
    };
    assert!(matcher.add("bad", vec![invalid]).is_err());
    assert!(matcher.is_empty());
}
#[test]
fn lexical_flags_require_a_lexicon_and_registration_stays_atomic() {
    let flag = |attribute| TokenPatternItem {
        constraints: vec![TokenConstraint {
            attribute,
            predicate: crate::Predicate::Flag { value: true },
        }],
        repetition: Repetition::Once,
    };
    let mut matcher = TokenMatcher::new();
    matcher
        .add("text", vec![pattern(&[Repetition::Once])])
        .unwrap();
    let result = matcher.add(
        "text",
        vec![
            pattern(&[Repetition::Once]),
            TokenPattern {
                tokens: vec![flag(crate::TokenAttribute::LikeNum)],
            },
        ],
    );
    assert!(matches!(result, Err(Error::Pattern(_))));
    assert!(matcher
        .add(
            "number",
            vec![TokenPattern {
                tokens: vec![flag(crate::TokenAttribute::IsDigit)],
            }],
        )
        .is_err());
    assert_eq!(matcher.get("text").unwrap().len(), 1);
    assert!(!matcher.contains("number"));
    assert_eq!(spans(&matcher, "x"), vec![("text".into(), 0, 1)]);
}
