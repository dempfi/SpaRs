use super::*;

// Compiled conditions with the pattern-level LENGTH table they index into.
struct Compiled {
    conditions: CompiledConditions,
    lengths: Vec<LengthChecks>,
}
impl Compiled {
    fn matches(&self, values: &TokenValues<'_>, index: usize) -> Result<bool> {
        self.conditions
            .lexical
            .matches(values, index, &self.lengths, 0)
    }
}
fn compile(constraints: &[TokenConstraint]) -> Result<Compiled> {
    let mut lengths = Vec::new();
    compile_conditions(constraints, &mut lengths).map(|conditions| Compiled {
        conditions,
        lengths,
    })
}
use crate::{Token, TokenIndex};

fn doc() -> Doc {
    let mut token = Token::new(
        crate::ByteOffset(0),
        crate::ByteOffset(1),
        crate::CodePointOffset(0),
        "x".into(),
    );
    token.morphology = Some("Case=Acc,Nom|Number=Sing".into());
    Doc {
        text: "x".into(),
        tokens: vec![token],
        entities: None,
        sentences: None,
        noun_chunks: None,
        tensor: Vec::new(),
        dependency_index: Default::default(),
    }
}
fn check(predicate: Predicate) -> bool {
    let doc = doc();
    let compiled = TokenConstraint {
        attribute: TokenAttribute::Morphology,
        predicate,
    }
    .compile()
    .unwrap();
    compiled.validate_document(&doc).unwrap();
    let values = TokenValues::new(
        &doc,
        Needs {
            lower: compiled.attribute() == TokenAttribute::Lower,
            ..Needs::default()
        },
        None,
    )
    .unwrap();
    compiled
        .matches(doc.token(TokenIndex(0)).unwrap(), &values)
        .unwrap()
}
#[test]
fn morphology_equality_sets_and_atomic_features_have_distinct_semantics() {
    assert!(check(Predicate::Equals {
        value: "Case=Acc,Nom|Number=Sing".into()
    }));
    assert!(!check(Predicate::Equals {
        value: "Number=Sing|Case=Nom,Acc".into()
    }));
    assert!(check(Predicate::In {
        values: vec!["Number=Sing|Case=Nom,Acc".into()]
    }));
    assert!(!check(Predicate::NotIn {
        values: vec!["Number=Sing|Case=Nom,Acc".into()]
    }));
    assert!(check(Predicate::MorphSuperset {
        values: vec!["Case=Nom".into(), "Number=Sing".into()]
    }));
    assert!(!check(Predicate::MorphSuperset {
        values: vec!["Case=Acc,Nom".into()]
    }));
    assert!(check(Predicate::MorphIntersects {
        values: vec!["Case=Dat".into(), "Case=Nom".into()]
    }));
    assert!(check(Predicate::MorphSuperset { values: vec![] }));
    assert!(!check(Predicate::MorphIntersects { values: vec![] }));
}
#[test]
fn malformed_constraints_and_missing_annotations_are_explicit() {
    for value in [
        "Case",
        "=Nom",
        "Case=",
        "Case=Nom,",
        "Case=Nom=Acc",
        "Case=Nom Acc",
    ] {
        assert!(TokenConstraint {
            attribute: TokenAttribute::Morphology,
            predicate: Predicate::In {
                values: vec![value.into()]
            }
        }
        .compile()
        .is_err());
    }
    assert!(TokenConstraint {
        attribute: TokenAttribute::Text,
        predicate: Predicate::MorphSuperset { values: vec![] }
    }
    .compile()
    .is_err());
    let constraint = TokenConstraint {
        attribute: TokenAttribute::Lemma,
        predicate: Predicate::Equals { value: "x".into() },
    }
    .compile()
    .unwrap();
    assert!(matches!(
        constraint.validate_document(&doc()),
        Err(Error::MissingAnnotation("lemma"))
    ));
    assert!(serde_json::from_str::<TokenConstraint>(
        r#"{"attribute":"unknown","predicate":{"kind":"equals","value":"x"}}"#
    )
    .is_err());
    assert!(serde_json::from_str::<Predicate>(r#"{"kind":"regex","value":"x"}"#).is_err());
}

#[test]
fn upstream_morphology_duplicate_and_empty_key_rules() {
    assert_eq!(normalize_morph("Case=Acc|Case=Nom").unwrap(), "Case=Nom");
    assert_eq!(normalize_morph("Case=Nom,Nom").unwrap(), "Case=Nom,Nom");
    assert_eq!(normalize_morph("pos=noun").unwrap(), "POS=NOUN");
    assert_eq!(
        normalize_morph("pos=noun|POS=VERB|pos=adj").unwrap(),
        "POS=VERB"
    );
    let mut doc = doc();
    doc.tokens[0].morphology = Some(String::new());
    for (value, expected) in [("", false), ("_", true)] {
        let condition = TokenConstraint {
            attribute: TokenAttribute::Morphology,
            predicate: Predicate::Equals {
                value: value.into(),
            },
        }
        .compile()
        .unwrap();
        let values = TokenValues::new(&doc, Needs::default(), None).unwrap();
        assert_eq!(
            condition
                .matches(doc.token(TokenIndex(0)).unwrap(), &values)
                .unwrap(),
            expected
        );
    }
}
#[test]
fn lower_needs_no_annotations_and_compares_pattern_values_as_written() {
    let text = "ΟΣ The";
    let doc = Doc {
        text: text.into(),
        tokens: vec![
            Token::new(
                crate::ByteOffset(0),
                crate::ByteOffset(4),
                crate::CodePointOffset(0),
                "ΟΣ".into(),
            ),
            Token::new(
                crate::ByteOffset(5),
                crate::ByteOffset(8),
                crate::CodePointOffset(3),
                "The".into(),
            ),
        ],
        entities: None,
        sentences: None,
        noun_chunks: None,
        tensor: Vec::new(),
        dependency_index: Default::default(),
    };
    let check = |predicate: Predicate, index: usize| {
        let compiled = TokenConstraint {
            attribute: TokenAttribute::Lower,
            predicate,
        }
        .compile()
        .unwrap();
        compiled.validate_document(&doc).unwrap();
        let values = TokenValues::new(
            &doc,
            Needs {
                lower: compiled.attribute() == TokenAttribute::Lower,
                ..Needs::default()
            },
            None,
        )
        .unwrap();
        let token = doc.token(TokenIndex(index)).unwrap();
        let cached = compiled.matches(token, &values).unwrap();
        let empty = TokenValues::new(&doc, Needs::default(), None).unwrap();
        assert_eq!(compiled.matches(token, &empty).unwrap(), cached);
        cached
    };
    let equals = |value: &str| Predicate::Equals {
        value: value.into(),
    };
    assert!(check(equals("ος"), 0));
    assert!(!check(equals("οσ"), 0));
    assert!(!check(equals("ΟΣ"), 0));
    assert!(check(equals("the"), 1));
    assert!(!check(equals("The"), 1));
    assert!(check(
        Predicate::In {
            values: vec!["ος".into(), "x".into()]
        },
        0
    ));
    assert!(!check(
        Predicate::NotIn {
            values: vec!["the".into()]
        },
        1
    ));
    assert!(check(Predicate::NotIn { values: vec![] }, 1));
}
#[test]
fn malformed_lower_conditions_are_rejected() {
    for predicate in [
        Predicate::MorphSuperset {
            values: vec!["the".into()],
        },
        Predicate::MorphIntersects { values: vec![] },
    ] {
        assert!(TokenConstraint {
            attribute: TokenAttribute::Lower,
            predicate,
        }
        .compile()
        .is_err());
    }
    for raw in [
        r#"{"attribute":"LOWER","predicate":{"kind":"equals","value":"a"}}"#,
        r#"{"attribute":"lowercase","predicate":{"kind":"equals","value":"a"}}"#,
        r#"{"attribute":"lower","predicate":{"kind":"equals","value":1}}"#,
        r#"{"attribute":"lower","predicate":{"kind":"equals","value":true}}"#,
        r#"{"attribute":"lower","predicate":{"kind":"in","values":"a"}}"#,
        r#"{"attribute":"lower","predicate":{"kind":"equals","value":"a","extra":1}}"#,
    ] {
        assert!(
            serde_json::from_str::<TokenConstraint>(raw).is_err(),
            "{raw}"
        );
    }
    let parsed: TokenConstraint =
        serde_json::from_str(r#"{"attribute":"lower","predicate":{"kind":"equals","value":"a"}}"#)
            .unwrap();
    assert_eq!(parsed.attribute, TokenAttribute::Lower);
}
#[test]
fn borrowed_lowercase_fast_path_matches_the_pinned_resource() {
    let ascii: String = (0u8..128).map(char::from).collect();
    let samples = [
        "",
        "the",
        "The",
        "THE",
        "123",
        ascii.as_str(),
        "ΟΣ",
        "AΣ\u{301}",
        "İ",
        "straße",
        "\u{212a}",
    ];
    for sample in samples {
        let fast = lower(sample);
        assert_eq!(fast, crate::unicode_lower::lower(sample), "{sample:?}");
        let borrowed = matches!(fast, Cow::Borrowed(_));
        let needs_change = !sample.is_ascii() || sample.bytes().any(|b| b.is_ascii_uppercase());
        assert_eq!(borrowed, !needs_change, "{sample:?}");
    }
}
#[test]
fn lower_set_values_are_not_lowercased_and_cache_is_filled_only_when_needed() {
    let doc = Doc {
        text: "ΟΣ The".into(),
        tokens: vec![
            Token::new(
                crate::ByteOffset(0),
                crate::ByteOffset(4),
                crate::CodePointOffset(0),
                "ΟΣ".into(),
            ),
            Token::new(
                crate::ByteOffset(5),
                crate::ByteOffset(8),
                crate::CodePointOffset(3),
                "The".into(),
            ),
        ],
        entities: None,
        sentences: None,
        noun_chunks: None,
        tensor: Vec::new(),
        dependency_index: Default::default(),
    };
    let values = TokenValues::new(
        &doc,
        Needs {
            lower: true,
            ..Needs::default()
        },
        None,
    )
    .unwrap();
    assert_eq!(values.lower, ["ος", "the"]);
    assert!(TokenValues::new(&doc, Needs::default(), None)
        .unwrap()
        .lower
        .is_empty());
    let check = |predicate: Predicate, index: usize| {
        TokenConstraint {
            attribute: TokenAttribute::Lower,
            predicate,
        }
        .compile()
        .unwrap()
        .matches(doc.token(TokenIndex(index)).unwrap(), &values)
        .unwrap()
    };
    let uppercase = || vec!["THE".into(), "ΟΣ".into()];
    assert!(!check(
        Predicate::In {
            values: uppercase()
        },
        0
    ));
    assert!(!check(
        Predicate::In {
            values: uppercase()
        },
        1
    ));
    assert!(check(
        Predicate::NotIn {
            values: uppercase()
        },
        0
    ));
    assert!(check(
        Predicate::NotIn {
            values: uppercase()
        },
        1
    ));
}
#[test]
fn malformed_lexical_flag_conditions_are_rejected() {
    for raw in [
        r#"{"attribute":"is_alpha","predicate":{"kind":"flag","value":1}}"#,
        r#"{"attribute":"is_alpha","predicate":{"kind":"flag","value":"true"}}"#,
        r#"{"attribute":"is_alpha","predicate":{"kind":"flag","value":null}}"#,
        r#"{"attribute":"is_alpha","predicate":{"kind":"flag"}}"#,
        r#"{"attribute":"IS_ALPHA","predicate":{"kind":"flag","value":true}}"#,
        r#"{"attribute":"is_oov","predicate":{"kind":"flag","value":true}}"#,
    ] {
        assert!(
            serde_json::from_str::<TokenConstraint>(raw).is_err(),
            "{raw}"
        );
    }
    for attribute in FLAG_ATTRIBUTES {
        let parsed: TokenConstraint = serde_json::from_value(serde_json::json!({
            "attribute": attribute, "predicate": {"kind": "flag", "value": false}
        }))
        .unwrap();
        assert_eq!(parsed.attribute, attribute);
        let compiled = compile(std::slice::from_ref(&parsed)).unwrap();
        assert!(compiled.conditions.constraints.is_empty());
        assert_eq!(
            compiled.conditions.lexical.mask(),
            attribute.flag_bit().unwrap()
        );
        for predicate in [
            Predicate::Equals {
                value: "true".into(),
            },
            Predicate::In {
                values: vec!["true".into()],
            },
            Predicate::NotIn { values: vec![] },
        ] {
            assert!(compile(&[TokenConstraint {
                attribute,
                predicate
            }])
            .is_err());
        }
    }
    for attribute in [
        TokenAttribute::Text,
        TokenAttribute::Lower,
        TokenAttribute::Morphology,
    ] {
        assert!(compile(&[TokenConstraint {
            attribute,
            predicate: Predicate::Flag { value: true }
        }])
        .is_err());
    }
}
#[test]
fn flag_tests_compare_every_required_bit_and_need_a_lexicon() {
    let doc = doc();
    let mask = TokenAttribute::LikeNum.flag_bit().unwrap();
    assert!(TokenValues::new(
        &doc,
        Needs {
            flags: mask,
            ..Needs::default()
        },
        None
    )
    .is_err());
    let flag = |attribute: TokenAttribute, value: bool| TokenConstraint {
        attribute,
        predicate: Predicate::Flag { value },
    };
    let test = |constraints: &[TokenConstraint]| compile(constraints).unwrap();
    let alpha = TokenAttribute::IsAlpha.flag_bit().unwrap();
    // Token 0 is alphabetic only; token 1 is a number only; token 2 has neither flag.
    let values = TokenValues {
        lower: Vec::new(),
        flags: vec![alpha, mask, 0],
        lengths: Vec::new(),
    };
    let check = |flags: Compiled| {
        (0..3)
            .map(|index| flags.matches(&values, index).unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(check(test(&[])), [true, true, true]);
    assert_eq!(
        check(test(&[flag(TokenAttribute::IsAlpha, true)])),
        [true, false, false]
    );
    assert_eq!(
        check(test(&[flag(TokenAttribute::IsAlpha, false)])),
        [false, true, true]
    );
    assert_eq!(
        check(test(&[
            flag(TokenAttribute::IsAlpha, false),
            flag(TokenAttribute::LikeNum, true)
        ])),
        [false, true, false]
    );
    assert_eq!(
        check(test(&[
            flag(TokenAttribute::LikeNum, true),
            flag(TokenAttribute::LikeNum, false)
        ])),
        [false, false, false]
    );
    let empty = TokenValues::new(&doc, Needs::default(), None).unwrap();
    assert!(test(&[flag(TokenAttribute::IsAlpha, true)])
        .matches(&empty, 0)
        .is_err());
}
#[test]
fn length_counts_code_points_and_compares_like_spacy() {
    // Code points: "a" 1, decomposed "é" 2, the ZWJ emoji sequence 4, "𐐀" 1.
    let words = [
        "a",
        "e\u{301}",
        "\u{1f469}\u{1f3fd}\u{200d}\u{1f4bb}",
        "\u{10400}",
    ];
    let text = words.join(" ");
    let mut tokens = Vec::new();
    let (mut start, mut idx) = (0, 0);
    for word in words {
        tokens.push(Token::new(
            crate::ByteOffset(start),
            crate::ByteOffset(start + word.len()),
            crate::CodePointOffset(idx),
            word.into(),
        ));
        start += word.len() + 1;
        idx += word.chars().count() + 1;
    }
    let doc = Doc {
        text,
        tokens,
        entities: None,
        sentences: None,
        noun_chunks: None,
        tensor: Vec::new(),
        dependency_index: Default::default(),
    };
    let values = TokenValues::new(
        &doc,
        Needs {
            length: true,
            ..Needs::default()
        },
        None,
    )
    .unwrap();
    assert_eq!(values.lengths, [1, 2, 4, 1]);
    let length = |predicate: Predicate| TokenConstraint {
        attribute: TokenAttribute::Length,
        predicate,
    };
    let compare = |operator, value: f64| {
        length(Predicate::Compare {
            operator,
            value: FiniteNumber::new(value).unwrap(),
        })
    };
    let check = |constraints: &[TokenConstraint]| {
        let test = compile(constraints).unwrap();
        (0..4)
            .map(|index| test.matches(&values, index).unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        check(&[compare(Comparison::Equal, 2.0)]),
        [false, true, false, false]
    );
    assert_eq!(
        check(&[compare(Comparison::NotEqual, 1.0)]),
        [false, true, true, false]
    );
    assert_eq!(
        check(&[compare(Comparison::Greater, 2.5)]),
        [false, false, true, false]
    );
    assert_eq!(
        check(&[compare(Comparison::GreaterOrEqual, 2.0)]),
        [false, true, true, false]
    );
    assert_eq!(
        check(&[compare(Comparison::Less, 2.0)]),
        [true, false, false, true]
    );
    assert_eq!(
        check(&[compare(Comparison::LessOrEqual, 1.0)]),
        [true, false, false, true]
    );
    assert_eq!(
        check(&[
            compare(Comparison::GreaterOrEqual, 2.0),
            compare(Comparison::Less, 4.0)
        ]),
        [false, true, false, false]
    );
    assert_eq!(
        check(&[length(Predicate::InIntegers {
            values: vec![-1, 4]
        })]),
        [false, false, true, false]
    );
    assert_eq!(
        check(&[length(Predicate::NotInIntegers { values: vec![1] })]),
        [false, true, true, false]
    );
    assert_eq!(
        check(&[length(Predicate::InIntegers { values: vec![] })]),
        [false; 4]
    );
    // Fractional values must not be truncated or rounded.
    assert_eq!(check(&[compare(Comparison::Equal, 2.5)]), [false; 4]);
    assert_eq!(
        check(&[compare(Comparison::GreaterOrEqual, 2.5)]),
        [false, false, true, false]
    );
    assert_eq!(
        check(&[compare(Comparison::Less, 2.5)]),
        [true, true, false, true]
    );
    assert_eq!(check(&[compare(Comparison::LessOrEqual, -0.5)]), [false; 4]);
    assert_eq!(
        check(&[compare(Comparison::NotEqual, 2.0)]),
        [true, false, true, true]
    );
    assert_eq!(
        check(&[length(Predicate::InIntegers { values: vec![2, 2] })]),
        [false, true, false, false]
    );
    assert_eq!(
        check(&[length(Predicate::NotInIntegers { values: vec![-1] })]),
        [true; 4]
    );
    // Flags and lengths on one item must both hold.
    let alpha = TokenAttribute::IsAlpha.flag_bit().unwrap();
    let mixed = TokenValues {
        lower: Vec::new(),
        flags: vec![alpha, alpha, 0],
        lengths: vec![1, 4, 4],
    };
    let both = compile(&[
        TokenConstraint {
            attribute: TokenAttribute::IsAlpha,
            predicate: Predicate::Flag { value: true },
        },
        compare(Comparison::GreaterOrEqual, 4.0),
    ])
    .unwrap();
    assert_eq!(
        (0..3)
            .map(|index| both.matches(&mixed, index).unwrap())
            .collect::<Vec<_>>(),
        [false, true, false]
    );
    let without = TokenValues::new(&doc, Needs::default(), None).unwrap();
    assert!(compile(&[compare(Comparison::Equal, 1.0)])
        .unwrap()
        .matches(&without, 0)
        .is_err());
}
#[test]
fn malformed_length_conditions_are_rejected() {
    assert!(FiniteNumber::new(f64::NAN).is_none());
    assert!(FiniteNumber::new(f64::INFINITY).is_none());
    for raw in [
        r#"{"attribute":"length","predicate":{"kind":"compare","operator":"=","value":1}}"#,
        r#"{"attribute":"length","predicate":{"kind":"compare","operator":"==","value":"1"}}"#,
        r#"{"attribute":"length","predicate":{"kind":"compare","operator":"==","value":true}}"#,
        r#"{"attribute":"length","predicate":{"kind":"compare","operator":"==","value":null}}"#,
        r#"{"attribute":"length","predicate":{"kind":"compare","value":1}}"#,
        r#"{"attribute":"length","predicate":{"kind":"in_integers","values":[1.5]}}"#,
        r#"{"attribute":"length","predicate":{"kind":"in_integers","values":["1"]}}"#,
        r#"{"attribute":"length","predicate":{"kind":"in_integers","values":[true]}}"#,
        r#"{"attribute":"length","predicate":{"kind":"in_integers","values":[1e30]}}"#,
        r#"{"attribute":"LENGTH","predicate":{"kind":"compare","operator":"==","value":1}}"#,
        r#"{"attribute":"length","predicate":{"kind":"compare","operator":"==","value":1,"values":[1]}}"#,
    ] {
        assert!(
            serde_json::from_str::<TokenConstraint>(raw).is_err(),
            "{raw}"
        );
    }
    let parsed: TokenConstraint = serde_json::from_str(
        r#"{"attribute":"length","predicate":{"kind":"compare","operator":">","value":2.5}}"#,
    )
    .unwrap();
    assert_eq!(
        parsed.predicate,
        Predicate::Compare {
            operator: Comparison::Greater,
            value: FiniteNumber::new(2.5).unwrap()
        }
    );
    for predicate in [
        Predicate::Equals { value: "3".into() },
        Predicate::In {
            values: vec!["3".into()],
        },
        Predicate::Flag { value: true },
        Predicate::MorphSuperset { values: vec![] },
    ] {
        assert!(matches!(
            compile(&[TokenConstraint {
                attribute: TokenAttribute::Length,
                predicate
            }]),
            Err(Error::Pattern(_))
        ));
    }
    for attribute in [
        TokenAttribute::Text,
        TokenAttribute::Lower,
        TokenAttribute::IsAlpha,
    ] {
        assert!(matches!(
            compile(&[TokenConstraint {
                attribute,
                predicate: Predicate::InIntegers { values: vec![1] }
            }]),
            Err(Error::Pattern(_))
        ));
    }
}
#[test]
fn flag_masks_use_distinct_bits_and_mark_contradictions() {
    let email = TokenAttribute::LikeEmail.flag_bit().unwrap();
    let alpha = TokenAttribute::IsAlpha.flag_bit().unwrap();
    assert_eq!(email, 1 << 16);
    assert!(email & (LENGTH_BIT | CONTRADICTION_BIT) == 0);
    // Token 0 is an email; token 1 is alphabetic; token 2 is both.
    let values = TokenValues {
        lower: Vec::new(),
        flags: vec![email, alpha, email | alpha],
        lengths: Vec::new(),
    };
    let flag = |attribute: TokenAttribute, value: bool| TokenConstraint {
        attribute,
        predicate: Predicate::Flag { value },
    };
    let check = |constraints: &[TokenConstraint]| {
        let compiled = compile(constraints).unwrap();
        (0..3)
            .map(|index| compiled.matches(&values, index).unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        check(&[flag(TokenAttribute::LikeEmail, true)]),
        [true, false, true]
    );
    assert_eq!(
        check(&[
            flag(TokenAttribute::LikeEmail, true),
            flag(TokenAttribute::IsAlpha, false)
        ]),
        [true, false, false]
    );
    assert_eq!(
        check(&[
            flag(TokenAttribute::LikeEmail, true),
            flag(TokenAttribute::LikeEmail, false)
        ]),
        [false; 3]
    );
    for attribute in FLAG_ATTRIBUTES {
        let bit = attribute.flag_bit().unwrap();
        assert_eq!(bit.count_ones(), 1);
        assert!(bit & (LENGTH_BIT | CONTRADICTION_BIT) == 0);
    }
}
#[test]
fn every_named_flag_attribute_has_a_distinct_bit() {
    let names = [
        "is_alpha",
        "is_digit",
        "is_space",
        "is_punct",
        "like_num",
        "is_lower",
        "is_upper",
        "is_title",
        "is_ascii",
        "is_currency",
        "is_stop",
        "is_bracket",
        "is_quote",
        "is_left_punct",
        "is_right_punct",
        "like_url",
        "like_email",
    ];
    let mut all = 0u32;
    for name in names {
        let attribute: TokenAttribute = serde_json::from_value(serde_json::json!(name)).unwrap();
        let bit = attribute
            .flag_bit()
            .unwrap_or_else(|| panic!("{name} has no flag bit"));
        assert_eq!(all & bit, 0, "{name} shares a bit");
        all |= bit;
    }
    assert_eq!(all.count_ones(), 17);
}
#[test]
fn contradictory_flags_fail_even_with_length_checks() {
    let email = TokenAttribute::LikeEmail.flag_bit().unwrap();
    let values = TokenValues {
        lower: Vec::new(),
        flags: vec![email, 0],
        lengths: vec![3, 3],
    };
    let flag = |value| TokenConstraint {
        attribute: TokenAttribute::LikeEmail,
        predicate: Predicate::Flag { value },
    };
    let compiled = compile(&[
        flag(true),
        flag(false),
        TokenConstraint {
            attribute: TokenAttribute::Length,
            predicate: Predicate::Compare {
                operator: Comparison::GreaterOrEqual,
                value: FiniteNumber::new(0.0).unwrap(),
            },
        },
    ])
    .unwrap();
    assert!(!compiled.matches(&values, 0).unwrap());
    assert!(!compiled.matches(&values, 1).unwrap());
}
#[test]
fn morphology_set_predicates_compare_individual_features() {
    let set = |values: &[&str]| values.iter().map(|value| value.to_string()).collect();
    // The token is `Case=Acc,Nom|Number=Sing`: features Case=Acc, Case=Nom, Number=Sing.
    assert!(check(Predicate::IsSubset {
        values: set(&["Case=Acc", "Case=Nom", "Number=Sing", "Person=3"])
    }));
    assert!(!check(Predicate::IsSubset {
        values: set(&["Case=Nom", "Number=Sing"])
    }));
    // A multi-valued entry never equals one feature, even after normalization.
    assert!(!check(Predicate::IsSubset {
        values: set(&["Case=Nom,Acc", "Number=Sing"])
    }));
    assert!(!check(Predicate::IsSubset { values: vec![] }));
    assert!(check(Predicate::IsSuperset {
        values: set(&["Case=Nom", "Number=Sing"])
    }));
    assert!(check(Predicate::IsSuperset { values: vec![] }));
    assert!(check(Predicate::Intersects {
        values: set(&["Case=Dat", "Case=Acc"])
    }));
    assert!(!check(Predicate::Intersects { values: vec![] }));
    // A repeated field keeps its last value, as in spaCy, so the entry is one feature.
    assert!(check(Predicate::IsSuperset {
        values: set(&["Number=Plur|Number=Sing"])
    }));
    assert!(!check(Predicate::IsSuperset {
        values: set(&["Number=Sing|Number=Plur"])
    }));
    // An empty analysis has no features, so it is a subset of every list.
    assert!(morph_subset("", &HashSet::new()));
    assert!(!morph_subset("Case=Nom", &HashSet::new()));
}
#[test]
fn string_set_predicates_treat_the_value_as_one_element() {
    let doc = doc();
    let matches = |predicate: Predicate| {
        let compiled = TokenConstraint {
            attribute: TokenAttribute::Text,
            predicate,
        }
        .compile()
        .unwrap();
        let values = TokenValues::new(&doc, Needs::default(), None).unwrap();
        compiled
            .matches(doc.token(TokenIndex(0)).unwrap(), &values)
            .unwrap()
    };
    let set = |values: &[&str]| values.iter().map(|value| value.to_string()).collect();
    assert!(matches(Predicate::IsSubset {
        values: set(&["x", "y"])
    }));
    assert!(!matches(Predicate::IsSubset {
        values: set(&["X"])
    }));
    assert!(!matches(Predicate::IsSubset { values: vec![] }));
    assert!(matches(Predicate::IsSuperset { values: vec![] }));
    assert!(matches(Predicate::IsSuperset {
        values: set(&["x", "x"])
    }));
    assert!(!matches(Predicate::IsSuperset {
        values: set(&["x", "y"])
    }));
    assert!(matches(Predicate::Intersects {
        values: set(&["y", "x"])
    }));
    assert!(!matches(Predicate::Intersects { values: vec![] }));
}
#[test]
fn length_set_predicates_match_spacy_one_element_sets() {
    let doc = doc();
    let values = TokenValues::new(
        &doc,
        Needs {
            length: true,
            ..Needs::default()
        },
        None,
    )
    .unwrap();
    // The token "x" has length 1.
    let matches = |predicate: Predicate| {
        compile(&[TokenConstraint {
            attribute: TokenAttribute::Length,
            predicate,
        }])
        .unwrap()
        .matches(&values, 0)
        .unwrap()
    };
    assert!(matches(Predicate::IsSubsetIntegers { values: vec![1, 2] }));
    assert!(!matches(Predicate::IsSubsetIntegers { values: vec![] }));
    assert!(matches(Predicate::IsSupersetIntegers { values: vec![] }));
    assert!(matches(Predicate::IsSupersetIntegers {
        values: vec![1, 1]
    }));
    assert!(!matches(Predicate::IsSupersetIntegers {
        values: vec![1, 2]
    }));
    assert!(!matches(Predicate::IsSupersetIntegers { values: vec![2] }));
    assert!(matches(Predicate::IntersectsIntegers {
        values: vec![-1, 1]
    }));
    assert!(!matches(Predicate::IntersectsIntegers { values: vec![] }));
}
#[test]
fn malformed_set_conditions_are_rejected() {
    // String sets need a string attribute; integer sets need LENGTH.
    for (attribute, predicate) in [
        (
            TokenAttribute::Length,
            Predicate::IsSubset {
                values: vec!["1".into()],
            },
        ),
        (
            TokenAttribute::Length,
            Predicate::IsSuperset { values: vec![] },
        ),
        (
            TokenAttribute::Length,
            Predicate::Intersects { values: vec![] },
        ),
        (
            TokenAttribute::IsAlpha,
            Predicate::IsSubset { values: vec![] },
        ),
        (
            TokenAttribute::Text,
            Predicate::IsSubsetIntegers { values: vec![1] },
        ),
        (
            TokenAttribute::Morphology,
            Predicate::IsSupersetIntegers { values: vec![] },
        ),
        (
            TokenAttribute::IsStop,
            Predicate::IntersectsIntegers { values: vec![] },
        ),
        (
            TokenAttribute::IsStop,
            Predicate::IsSuperset { values: vec![] },
        ),
        (
            TokenAttribute::LikeEmail,
            Predicate::Intersects { values: vec![] },
        ),
        (
            TokenAttribute::Lower,
            Predicate::MorphSuperset { values: vec![] },
        ),
    ] {
        assert!(
            matches!(
                compile(&[TokenConstraint {
                    attribute,
                    predicate: predicate.clone()
                }]),
                Err(Error::Pattern(_))
            ),
            "{attribute:?} {predicate:?}"
        );
    }
    // Morphology entries are validated as for the other set predicates.
    for predicate in [
        Predicate::IsSubset {
            values: vec!["Case".into()],
        },
        Predicate::IsSuperset {
            values: vec!["Case=".into()],
        },
        Predicate::Intersects {
            values: vec!["Case=Nom Acc".into()],
        },
    ] {
        assert!(matches!(
            TokenConstraint {
                attribute: TokenAttribute::Morphology,
                predicate
            }
            .compile(),
            Err(Error::Pattern(_))
        ));
    }
    for raw in [
        r#"{"attribute":"length","predicate":{"kind":"is_subset_integers","values":[1.5]}}"#,
        r#"{"attribute":"length","predicate":{"kind":"is_superset_integers","values":["1"]}}"#,
        r#"{"attribute":"length","predicate":{"kind":"intersects_integers","values":[true]}}"#,
        r#"{"attribute":"lower","predicate":{"kind":"is_subset","values":[1]}}"#,
        r#"{"attribute":"lower","predicate":{"kind":"is_superset","values":null}}"#,
        r#"{"attribute":"lower","predicate":{"kind":"intersects","value":"a"}}"#,
        r#"{"attribute":"lower","predicate":{"kind":"IS_SUBSET","values":["a"]}}"#,
    ] {
        assert!(
            serde_json::from_str::<TokenConstraint>(raw).is_err(),
            "{raw}"
        );
    }
}
#[test]
fn set_predicates_still_require_their_annotation() {
    // Even an always-true `IsSuperset []` validates that the annotation exists.
    for (attribute, name) in [
        (TokenAttribute::Pos, "POS"),
        (TokenAttribute::Lemma, "lemma"),
    ] {
        let constraint = TokenConstraint {
            attribute,
            predicate: Predicate::IsSuperset { values: vec![] },
        }
        .compile()
        .unwrap();
        assert!(matches!(
            constraint.validate_document(&doc()),
            Err(Error::MissingAnnotation(missing)) if missing == name
        ));
    }
}
#[test]
fn morphology_normalization_sorts_whole_fields_and_folds_pos_like_spacy() {
    // Outputs of spaCy 3.8.14 `Morphology.normalize_features`, checked in the pinned
    // reference environment; the frozen fixture covers a subset through the matcher.
    for (input, expected) in [
        ("Case=Nom|Case2=Acc", "Case2=Acc|Case=Nom"),
        (
            "Number[psor]=Sing|Number=Plur",
            "Number=Plur|Number[psor]=Sing",
        ),
        ("A_B=x|A=y", "A=y|A_B=x"),
        ("a=1|B=2", "B=2|a=1"),
        ("poſ=noun", "POS=NOUN"),
        ("POS=ſym", "POS=SYM"),
        ("pos=foo", "POS=foo"),
        ("pos=ıntj", "POS=INTJ"),
        ("pos=space", "POS=SPACE"),
        ("POS=noun,verb", "POS=noun,verb"),
        ("pos=x|POS=y", "POS=y"),
    ] {
        assert_eq!(normalize_morph(input).unwrap(), expected, "{input}");
    }
    assert!(is_pos_key("poſ") && is_pos_key("Pos") && is_pos_key("POS"));
    assert!(!is_pos_key("POSS") && !is_pos_key("PO") && !is_pos_key("Case"));
    // Document morphology must already be in that order, with no repeated field.
    for canonical in [
        "Case2=Acc|Case=Nom",
        "Number=Plur|Number[psor]=Sing",
        "POS=SYM",
        "POS=foo",
        "POS=Foo",
        "POS=noun,verb",
        "Case=Acc,Nom",
        "Case=Nom,Nom",
        "",
    ] {
        assert!(validate_canonical_morph(canonical).is_ok(), "{canonical}");
    }
    for noncanonical in [
        "Case=Nom|Case2=Acc",
        "Case=Acc|Case=Nom",
        "poſ=NOUN",
        "POS=ſym",
        "POS=noun",
        "Case=Nom,Acc",
        "Case=Nom,",
        "Case=Nom Acc",
        "Case=Nom=Acc",
        "=Nom",
        "Case",
        "_",
        "Case2=Acc|Case=Nom|Case=Nom",
    ] {
        assert!(
            matches!(
                validate_canonical_morph(noncanonical),
                Err(Error::Unsupported(_))
            ),
            "{noncanonical}"
        );
    }
}
#[test]
fn known_pos_names_are_spacy_part_of_speech_ids() {
    // spaCy 3.8.14 `spacy.parts_of_speech.IDS`, without the empty name.
    let names = [
        "ADJ", "ADP", "ADV", "AUX", "CCONJ", "CONJ", "DET", "EOL", "INTJ", "NOUN", "NUM", "PART",
        "PRON", "PROPN", "PUNCT", "SCONJ", "SPACE", "SYM", "VERB", "X",
    ];
    for name in names {
        assert!(known_pos(name), "{name}");
    }
    for other in ["", "noun", "NOUNS", "PROPER", "SPACES"] {
        assert!(!known_pos(other), "{other}");
    }
}
