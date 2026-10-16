use serde::{Deserialize, Serialize};
use spars::{Doc, Span, Token, TokenMatch, TokenMatcher, TokenPattern};
use std::collections::BTreeMap;
mod support;

#[derive(Deserialize)]
struct Fixture {
    versions: BTreeMap<String, String>,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    id: String,
    text: String,
    tokens: Vec<Token>,
    sentences: Vec<Span>,
    noun_chunks: Vec<Span>,
    rules: Vec<Rule>,
    expected: Vec<TokenMatch>,
}
#[derive(Deserialize)]
struct Rule {
    name: String,
    patterns: Vec<TokenPattern>,
}
#[derive(Serialize)]
struct Snapshot<'a> {
    format_version: u32,
    document: Storage<'a>,
}
#[derive(Serialize)]
struct Storage<'a> {
    text: &'a str,
    tokens: &'a [Token],
    sentences: &'a [Span],
    entities: Vec<Span>,
    noun_chunks: &'a [Span],
}

#[test]
fn official_token_patterns_repetition_and_order() {
    check(
        "../../fixtures/token-match-v1.expected.json",
        10,
        38,
        480,
        704,
    );
    check(
        "../../fixtures/token-match-exhaustive-v1.expected.json",
        31,
        98,
        4805,
        6261,
    );
}
#[test]
fn official_long_branching_patterns() {
    check(
        "../../fixtures/token-match-branching-v1.expected.json",
        31,
        98,
        248,
        288,
    );
}
#[test]
fn official_length_conditions() {
    check(
        "../../fixtures/token-match-length-v1.expected.json",
        3,
        28,
        105,
        386,
    );
}
#[test]
fn official_set_comparisons() {
    check(
        "../../fixtures/token-match-sets-v1.expected.json",
        3,
        21,
        558,
        653,
    );
}
#[test]
fn official_unicode_lower_conditions() {
    check(
        "../../fixtures/token-match-lower-v1.expected.json",
        4,
        45,
        172,
        280,
    );
}
#[test]
#[ignore = "requires official export; mandatory CI"]
fn official_lexical_flag_conditions() {
    // Flags depend only on token text, so every supported English size shares the
    // same expectations. Each lexicon stays valid after its model is dropped.
    for size in ["sm", "md", "lg"] {
        let lexicon = spars::Model::load(format!("../../assets/en_core_web_{size}-3.8.0"))
            .unwrap()
            .lexicon();
        check_with(
            "../../fixtures/token-match-flags-v1.expected.json",
            FLAG_CASES,
            FLAG_TOKENS,
            FLAG_RULES,
            FLAG_MATCHES,
            Some(&lexicon),
        );
    }
}
#[test]
#[ignore = "requires official export; mandatory CI"]
fn official_more_lexical_flag_conditions() {
    for size in ["sm", "md", "lg"] {
        let lexicon = spars::Model::load(format!("../../assets/en_core_web_{size}-3.8.0"))
            .unwrap()
            .lexicon();
        check_with(
            "../../fixtures/token-match-more-flags-v1.expected.json",
            3,
            98,
            99,
            1228,
            Some(&lexicon),
        );
    }
}
const FLAG_CASES: usize = 3;
const FLAG_TOKENS: usize = 65;
const FLAG_RULES: usize = 51;
const FLAG_MATCHES: usize = 442;
#[test]
#[ignore = "requires official export; mandatory CI"]
fn reused_flag_matcher_keeps_documents_independent() {
    let model = spars::Model::load("../../assets/en_core_web_md-3.8.0").unwrap();
    let pattern: TokenPattern = serde_json::from_str(
        r#"{"tokens":[{"constraints":[{"attribute":"like_num","predicate":{"kind":"flag","value":true}}],"repetition":{"kind":"once"}}]}"#,
    )
    .unwrap();
    let mut matcher = TokenMatcher::with_lexicon(model.lexicon());
    matcher.add("number", vec![pattern]).unwrap();
    let found = |text: &str| {
        let doc = model.process(text).unwrap();
        matcher
            .find_matches(&doc)
            .unwrap()
            .into_iter()
            .map(|m| (m.start.0, m.end.0))
            .collect::<Vec<_>>()
    };
    assert_eq!(found("ten apples"), [(0, 1)]);
    assert_eq!(found("I ate 5 of ten"), [(2, 3), (4, 5)]);
    assert_eq!(found("no numbers"), []);
    assert_eq!(found("ten apples"), [(0, 1)]);
}
#[test]
fn reused_length_matcher_with_mixed_rules_keeps_documents_independent() {
    let snapshot = |words: &[&str]| {
        let text = words.join(" ");
        let mut start = 0;
        let tokens: Vec<Token> = words
            .iter()
            .map(|word| {
                let idx = text[..start].chars().count();
                let token: Token = serde_json::from_value(serde_json::json!({
                    "start": start, "end": start + word.len(), "idx": idx,
                    "whitespace": start + word.len() < text.len(), "norm": word,
                }))
                .unwrap();
                start += word.len() + 1;
                token
            })
            .collect();
        Doc::from_json(
            &serde_json::json!({"format_version": 1, "document": {"text": text, "tokens": tokens}})
                .to_string(),
        )
        .unwrap()
    };
    let pattern = |raw: &str| -> TokenPattern { serde_json::from_str(raw).unwrap() };
    let mut matcher = TokenMatcher::new();
    // Only the second rule needs lengths; the per-call values must still include them.
    matcher
        .add(
            "text",
            vec![pattern(
                r#"{"tokens":[{"constraints":[{"attribute":"text","predicate":{"kind":"equals","value":"x"}}],"repetition":{"kind":"once"}}]}"#,
            )],
        )
        .unwrap();
    matcher
        .add(
            "long",
            vec![pattern(
                r#"{"tokens":[{"constraints":[{"attribute":"length","predicate":{"kind":"compare","operator":">=","value":3}}],"repetition":{"kind":"once"}}]}"#,
            )],
        )
        .unwrap();
    let found = |doc: &Doc| {
        matcher
            .find_matches(doc)
            .unwrap()
            .into_iter()
            .map(|m| (m.rule, m.start.0, m.end.0))
            .collect::<Vec<_>>()
    };
    let first = snapshot(&["ab", "abcd"]);
    let second = snapshot(&["abcd", "x", "abc"]);
    assert_eq!(found(&first), [("long".into(), 1, 2)]);
    assert_eq!(
        found(&second),
        [
            ("long".into(), 0, 1),
            ("text".into(), 1, 2),
            ("long".into(), 2, 3)
        ]
    );
    assert_eq!(found(&first), [("long".into(), 1, 2)]);
}
#[test]
fn length_on_a_later_item_uses_that_items_checks() {
    let words = ["x", "ab", "x", "abcd"];
    let text = words.join(" ");
    let mut start = 0;
    let tokens: Vec<Token> = words
        .iter()
        .map(|word| {
            let idx = text[..start].chars().count();
            let token: Token = serde_json::from_value(serde_json::json!({
                "start": start, "end": start + word.len(), "idx": idx,
                "whitespace": start + word.len() < text.len(), "norm": word,
            }))
            .unwrap();
            start += word.len() + 1;
            token
        })
        .collect();
    let doc = Doc::from_json(
        &serde_json::json!({"format_version": 1, "document": {"text": text, "tokens": tokens}})
            .to_string(),
    )
    .unwrap();
    let x = r#"{"constraints":[{"attribute":"text","predicate":{"kind":"equals","value":"x"}}],"repetition":{"kind":"once"}}"#;
    let skipped = r#"{"constraints":[{"attribute":"text","predicate":{"kind":"equals","value":"x"}}],"repetition":{"kind":"range","min":0,"max":0}}"#;
    let long = r#"{"constraints":[{"attribute":"length","predicate":{"kind":"compare","operator":">=","value":3}}],"repetition":{"kind":"once"}}"#;
    for (name, items, expected) in [
        ("plain_then_long", format!("[{x},{long}]"), vec![(2, 4)]),
        (
            "zero_count_first",
            format!("[{skipped},{x},{long}]"),
            vec![(2, 4)],
        ),
    ] {
        let pattern: TokenPattern =
            serde_json::from_str(&format!(r#"{{"tokens":{items}}}"#)).unwrap();
        let mut matcher = TokenMatcher::new();
        matcher.add(name, vec![pattern]).unwrap();
        let found: Vec<_> = matcher
            .find_matches(&doc)
            .unwrap()
            .into_iter()
            .map(|m| (m.start.0, m.end.0))
            .collect();
        assert_eq!(found, expected, "{name}");
    }
}
#[test]
fn reused_lower_matcher_keeps_documents_independent() {
    let snapshot = |words: &[&str]| {
        let text = words.join(" ");
        let mut start = 0;
        let tokens: Vec<Token> = words
            .iter()
            .map(|word| {
                let idx = text[..start].chars().count();
                let token: Token = serde_json::from_value(serde_json::json!({
                    "start": start, "end": start + word.len(), "idx": idx,
                    "whitespace": start + word.len() < text.len(), "norm": word,
                }))
                .unwrap();
                start += word.len() + 1;
                token
            })
            .collect();
        Doc::from_json(
            &serde_json::json!({"format_version": 1, "document": {"text": text, "tokens": tokens}})
                .to_string(),
        )
        .unwrap()
    };
    let pattern: TokenPattern = serde_json::from_str(
        r#"{"tokens":[{"constraints":[{"attribute":"lower","predicate":{"kind":"equals","value":"ritz"}}],"repetition":{"kind":"once"}}]}"#,
    )
    .unwrap();
    let mut matcher = TokenMatcher::new();
    matcher.add("ritz", vec![pattern]).unwrap();
    let first = snapshot(&["the", "RITZ"]);
    let second = snapshot(&["ΟΣ", "Ritz", "x", "ritz"]);
    let found = |doc: &Doc| {
        matcher
            .find_matches(doc)
            .unwrap()
            .into_iter()
            .map(|m| (m.start.0, m.end.0))
            .collect::<Vec<_>>()
    };
    assert_eq!(found(&first), [(1, 2)]);
    assert_eq!(found(&second), [(1, 2), (3, 4)]);
    assert_eq!(found(&snapshot(&["the"])), []);
    assert_eq!(found(&first), [(1, 2)]);
}
fn check(path: &str, cases: usize, tokens: usize, rules: usize, matches: usize) {
    check_with(path, cases, tokens, rules, matches, None);
}
fn check_with(
    path: &str,
    cases: usize,
    tokens: usize,
    rules: usize,
    matches: usize,
    lexicon: Option<&spars::Lexicon>,
) {
    let fixture: Fixture = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(fixture.versions["spacy"], "3.8.14");
    assert_eq!(fixture.versions["thinc"], "8.3.13");
    assert_eq!(fixture.cases.len(), cases);
    assert_eq!(
        fixture.cases.iter().map(|c| c.tokens.len()).sum::<usize>(),
        tokens
    );
    assert_eq!(
        fixture.cases.iter().map(|c| c.rules.len()).sum::<usize>(),
        rules
    );
    assert_eq!(
        fixture
            .cases
            .iter()
            .map(|c| c.expected.len())
            .sum::<usize>(),
        matches
    );
    for case in fixture.cases {
        let doc = Doc::from_json(
            &serde_json::to_string(&Snapshot {
                format_version: 1,
                document: Storage {
                    text: &case.text,
                    tokens: &case.tokens,
                    sentences: &case.sentences,
                    entities: support::entities_from_tags(&case.tokens),
                    noun_chunks: &case.noun_chunks,
                },
            })
            .unwrap(),
        )
        .unwrap();
        let mut matcher = lexicon.map_or_else(TokenMatcher::new, |lexicon| {
            TokenMatcher::with_lexicon(lexicon.clone())
        });
        for rule in case.rules {
            matcher.add(rule.name, rule.patterns).unwrap();
        }
        let actual = matcher.find_matches(&doc).unwrap();
        assert_eq!(actual, case.expected, "{}: {}", case.id, case.text);
        assert_eq!(
            matcher.find_matches(&doc).unwrap(),
            actual,
            "repeated {}",
            case.id
        );
        std::thread::scope(|scope| {
            let concurrent = scope.spawn(|| matcher.find_matches(&doc).unwrap());
            assert_eq!(concurrent.join().unwrap(), actual);
        });
    }
}
#[test]
fn noncanonical_document_morphology_is_rejected_by_find_matches() {
    let doc = |morphology: &str| {
        Doc::from_json(
            &serde_json::json!({"format_version": 1, "document": {"text": "a", "tokens": [{
                "start": 0, "end": 1, "idx": 0, "whitespace": false, "norm": "a",
                "morphology": morphology,
            }]}})
            .to_string(),
        )
        .unwrap()
    };
    let pattern: TokenPattern = serde_json::from_str(
        r#"{"tokens":[{"constraints":[{"attribute":"morphology","predicate":{"kind":"intersects","values":["Case=Nom"]}}],"repetition":{"kind":"once"}}]}"#,
    )
    .unwrap();
    let mut matcher = TokenMatcher::new();
    matcher.add("rule", vec![pattern]).unwrap();
    for noncanonical in [
        "Case=Nom|Case2=Acc",
        "Case=Acc|Case=Nom",
        "poſ=NOUN",
        "POS=noun",
    ] {
        assert!(
            matches!(
                matcher.find_matches(&doc(noncanonical)),
                Err(spars::Error::Unsupported(_))
            ),
            "{noncanonical}"
        );
    }
    assert_eq!(
        matcher
            .find_matches(&doc("Case2=Acc|Case=Nom"))
            .unwrap()
            .len(),
        1
    );
}
