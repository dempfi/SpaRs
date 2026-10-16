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

// A small deterministic generator, so failures reproduce from the printed seed.
struct Xorshift(u64);
impl Xorshift {
    fn below(&mut self, bound: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        usize::try_from(self.0 % u64::try_from(bound).unwrap()).unwrap()
    }
    fn pick<'a>(&mut self, values: &[&'a str]) -> &'a str {
        values[self.below(values.len())]
    }
}

// Unicode words whose Python lowercase differs from a per-character mapping, and words that
// differ only in case, so `Lower` keys and token values meet in both directions.
const WORDS: [&str; 14] = [
    "a", "A", "ab", "AB", "b", "İ", "i̇", "Σ", "σ", "ς", "ẞ", "ß", "ǅ", "x",
];

fn words_doc(words: &[&str], norms: &[&str], lemmas: bool) -> Doc {
    let text = words.join(" ");
    let (mut start, mut cp) = (0, 0);
    let tokens = words
        .iter()
        .zip(norms)
        .enumerate()
        .map(|(i, (word, norm))| {
            let mut token = Token::new(
                crate::ByteOffset(start),
                crate::ByteOffset(start + word.len()),
                crate::CodePointOffset(cp),
                (*norm).into(),
            );
            token.whitespace = i + 1 < words.len();
            token.lemma = lemmas.then(|| norm.to_lowercase());
            start += word.len() + 1;
            cp += word.chars().count() + 1;
            token
        })
        .collect();
    Doc {
        text,
        tokens,
        entities: None,
        sentences: None,
        noun_chunks: None,
        tensor: Vec::new(),
        dependency_index: OnceLock::new(),
    }
}

fn random_constraint(rng: &mut Xorshift) -> TokenConstraint {
    use crate::{Comparison, FiniteNumber, Predicate, TokenAttribute};
    if rng.below(12) == 0 {
        return TokenConstraint {
            attribute: TokenAttribute::Length,
            predicate: Predicate::Compare {
                operator: [Comparison::LessOrEqual, Comparison::Greater][rng.below(2)],
                value: FiniteNumber::new(1.0).unwrap(),
            },
        };
    }
    let attribute = [
        TokenAttribute::Text,
        TokenAttribute::Lower,
        TokenAttribute::Norm,
        TokenAttribute::Lemma,
    ][rng.below(4)];
    let values = |rng: &mut Xorshift| -> Vec<String> {
        (0..rng.below(4)).map(|_| rng.pick(&WORDS).into()).collect()
    };
    let predicate = match rng.below(7) {
        0..=2 => Predicate::Equals {
            value: rng.pick(&WORDS).into(),
        },
        3 => Predicate::In {
            values: values(rng),
        },
        4 => Predicate::NotIn {
            values: values(rng),
        },
        5 => Predicate::IsSubset {
            values: values(rng),
        },
        _ => Predicate::IsSuperset {
            values: values(rng),
        },
    };
    TokenConstraint {
        attribute,
        predicate,
    }
}

fn random_pattern(rng: &mut Xorshift) -> TokenPattern {
    let repetitions = [
        Repetition::Once,
        Repetition::Once,
        Repetition::Once,
        Repetition::OneOrMore,
        Repetition::Optional,
        Repetition::ZeroOrMore,
        Repetition::Negated,
        Repetition::Range {
            min: 0,
            max: Some(0),
        },
        Repetition::Range {
            min: 2,
            max: Some(3),
        },
        Repetition::Range { min: 1, max: None },
        // Nine optional nodes select the engine's suffix-cache path.
        Repetition::Range {
            min: 1,
            max: Some(10),
        },
    ];
    let mut tokens: Vec<TokenPatternItem> = (0..1 + rng.below(3))
        .map(|_| TokenPatternItem {
            constraints: (0..rng.below(3)).map(|_| random_constraint(rng)).collect(),
            repetition: repetitions[rng.below(repetitions.len())],
        })
        .collect();
    // Occasionally make the pattern invalid, so a rejected `add` must leave the index unchanged.
    if rng.below(20) == 0 {
        tokens.push(TokenPatternItem {
            constraints: Vec::new(),
            repetition: Repetition::Range {
                min: 3,
                max: Some(1),
            },
        });
    }
    TokenPattern { tokens }
}

type Outcome = std::result::Result<Vec<(String, usize, usize)>, String>;
fn outcome(result: Result<Vec<TokenMatch>>) -> Outcome {
    result
        .map(|matches| {
            matches
                .into_iter()
                .map(|m| (m.rule, m.start.0, m.end.0))
                .collect()
        })
        .map_err(|error| format!("{error:?}"))
}

#[test]
fn indexed_starts_match_starting_every_pattern_at_every_token() {
    let mut compared = 0;
    let mut nonempty = 0;
    let mut errors = 0;
    let mut rejected = 0;
    // Matches from rules whose every pattern is reached only through the value index.
    let mut indexed_matches = 0;
    for seed in 1..=300u64 {
        let mut rng = Xorshift(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
        let docs: Vec<Doc> = (0..6)
            .map(|_| {
                // Include empty documents.
                let length = rng.below(14);
                let words: Vec<&str> = (0..length).map(|_| rng.pick(&WORDS)).collect();
                let norms: Vec<&str> = (0..length).map(|_| rng.pick(&WORDS)).collect();
                words_doc(&words, &norms, rng.below(3) != 0)
            })
            .collect();
        let mut matcher = TokenMatcher::new();
        let names = ["r0", "r1", "r2", "r3", "r4"];
        for _ in 0..12 {
            // Repeated names append patterns; removal renumbers later patterns.
            let name = rng.pick(&names);
            if rng.below(4) == 0 && matcher.contains(name) {
                matcher.remove(name).unwrap();
            } else {
                let patterns = (0..1 + rng.below(3))
                    .map(|_| random_pattern(&mut rng))
                    .collect();
                if matcher.add(name, patterns).is_err() {
                    rejected += 1;
                }
            }
            let indexed_positions = matcher.starts.indexed_positions();
            let indexed_rule = |rule: &str| {
                matcher
                    .compiled
                    .iter()
                    .enumerate()
                    .filter(|(_, pattern)| pattern.name == rule)
                    .all(|(position, _)| indexed_positions.contains(&position))
            };
            for doc in &docs {
                let indexed = outcome(matcher.find_matches(doc));
                let reference = outcome(matcher.find_matches_unindexed(doc));
                assert_eq!(indexed, reference, "seed {seed}, document {:?}", doc.text);
                compared += 1;
                match indexed {
                    Ok(found) if !found.is_empty() => {
                        nonempty += 1;
                        indexed_matches += found
                            .iter()
                            .filter(|(rule, _, _)| indexed_rule(rule))
                            .count();
                    }
                    Err(_) => errors += 1,
                    Ok(_) => {}
                }
            }
        }
    }
    // The generator must exercise matches, matches that need indexed starts, rejected
    // registrations, and missing-annotation errors.
    assert!(nonempty > compared / 4, "{nonempty} of {compared}");
    assert!(indexed_matches > 1000, "{indexed_matches} indexed matches");
    assert!(rejected > 10, "{rejected} rejected registrations");
    assert!(
        errors > 0 && errors < compared / 2,
        "{errors} of {compared}"
    );
}

#[test]
fn indexed_patterns_still_report_missing_annotations_without_matching_tokens() {
    let text = |value: &str| TokenConstraint {
        attribute: crate::TokenAttribute::Text,
        predicate: crate::Predicate::Equals {
            value: value.into(),
        },
    };
    let lemma = TokenConstraint {
        attribute: crate::TokenAttribute::Lemma,
        predicate: crate::Predicate::Equals { value: "z".into() },
    };
    for tokens in [
        vec![TokenPatternItem {
            constraints: vec![text("absent"), lemma.clone()],
            repetition: Repetition::Once,
        }],
        vec![
            TokenPatternItem {
                constraints: vec![text("absent")],
                repetition: Repetition::Once,
            },
            TokenPatternItem {
                constraints: vec![lemma.clone()],
                repetition: Repetition::Once,
            },
        ],
    ] {
        let mut matcher = TokenMatcher::new();
        matcher.add("r", vec![TokenPattern { tokens }]).unwrap();
        assert!(matches!(
            matcher.find_matches(&doc("xy")),
            Err(Error::MissingAnnotation("lemma"))
        ));
    }
}

#[test]
fn removal_keeps_indexed_starts_in_registration_order() {
    let exact = |attribute, repetition| TokenPattern {
        tokens: vec![TokenPatternItem {
            constraints: vec![TokenConstraint {
                attribute,
                predicate: crate::Predicate::Equals { value: "x".into() },
            }],
            repetition,
        }],
    };
    // Every pattern emits at the token where it starts, so the output follows pattern order.
    let checked = |matcher: &TokenMatcher, text: &str| {
        let found = spans(matcher, text);
        assert_eq!(
            outcome(matcher.find_matches_unindexed(&doc(text))),
            Ok(found.clone())
        );
        found
    };
    let expected = |rules: &[&str]| -> Vec<(String, usize, usize)> {
        rules.iter().map(|rule| ((*rule).into(), 0, 1)).collect()
    };
    use crate::TokenAttribute::{Lower, Norm, Text};
    let mut matcher = TokenMatcher::new();
    matcher
        .add("a", vec![exact(Text, Repetition::Once)])
        .unwrap();
    // Optional first items are not indexed.
    matcher
        .add("b", vec![exact(Lower, Repetition::Optional)])
        .unwrap();
    matcher
        .add("c", vec![exact(Norm, Repetition::Once)])
        .unwrap();
    matcher
        .add("a", vec![exact(Lower, Repetition::Once)])
        .unwrap();
    assert_eq!(checked(&matcher, "x"), expected(&["a", "b", "c"]));
    matcher.remove("a").unwrap();
    assert_eq!(checked(&matcher, "x"), expected(&["b", "c"]));
    matcher
        .add("a", vec![exact(Text, Repetition::Once)])
        .unwrap();
    assert_eq!(checked(&matcher, "x"), expected(&["b", "c", "a"]));
    matcher.remove("b").unwrap();
    assert_eq!(checked(&matcher, "xy"), expected(&["c", "a"]));
    matcher.remove("c").unwrap();
    matcher.remove("a").unwrap();
    assert!(checked(&matcher, "x").is_empty());
}

#[test]
fn candidates_are_ascending_distinct_and_exclude_impossible_values() {
    use crate::dependency_matcher::predicates::{Needs, TokenValues};
    use crate::TokenAttribute::{Lower, Norm, Text};
    let exact = |attribute, value: &str, repetition| TokenPattern {
        tokens: vec![TokenPatternItem {
            constraints: vec![TokenConstraint {
                attribute,
                predicate: crate::Predicate::Equals {
                    value: value.into(),
                },
            }],
            repetition,
        }],
    };
    let mut matcher = TokenMatcher::new();
    for (rule, pattern) in [
        ("text", exact(Text, "A", Repetition::Once)),
        ("lower", exact(Lower, "a", Repetition::Once)),
        ("norm", exact(Norm, "b", Repetition::OneOrMore)),
        // Lowercase text never contains "A", so this pattern never starts.
        ("never", exact(Lower, "A", Repetition::Once)),
        ("optional", exact(Text, "z", Repetition::Optional)),
        ("other", exact(Text, "q", Repetition::Once)),
    ] {
        matcher.add(rule, vec![pattern]).unwrap();
    }
    let doc = words_doc(&["A"], &["b"], false);
    let needs = Needs {
        lower: true,
        ..Needs::default()
    };
    let values = TokenValues::new(&doc, needs, None).unwrap();
    let mut out = vec![99];
    matcher
        .starts
        .candidates(doc.token(TokenIndex(0)).unwrap(), &values, &mut out);
    assert_eq!(out, [0, 1, 2, 4]);
}

#[test]
fn rejected_registration_leaves_indexed_starts_unchanged() {
    let text = TokenPatternItem {
        constraints: vec![TokenConstraint {
            attribute: crate::TokenAttribute::Text,
            predicate: crate::Predicate::Equals { value: "x".into() },
        }],
        repetition: Repetition::Once,
    };
    let invalid = pattern(&[Repetition::Range {
        min: 3,
        max: Some(1),
    }]);
    let mut matcher = TokenMatcher::new();
    let valid = TokenPattern { tokens: vec![text] };
    assert!(matcher.add("bad", vec![valid.clone(), invalid]).is_err());
    matcher.add("good", vec![valid]).unwrap();
    let found = spans(&matcher, "xyx");
    assert_eq!(found, [("good".into(), 0, 1), ("good".into(), 2, 3)]);
    assert_eq!(
        outcome(matcher.find_matches_unindexed(&doc("xyx"))),
        Ok(found)
    );
}
