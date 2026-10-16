use serde::{Deserialize, Serialize};
use spars::{DependencyMatch, DependencyMatcher, DependencyPattern, Doc, Span, Token};
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
    expected: Vec<DependencyMatch>,
}
#[derive(Deserialize)]
struct Rule {
    name: String,
    patterns: Vec<DependencyPattern>,
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
fn official_dependency_pattern_order_and_annotations() {
    check(
        "../../fixtures/dependency-match-v1.expected.json",
        6,
        Some(52),
    );
    check(
        "../../fixtures/dependency-match-regressions-v1.expected.json",
        2,
        None,
    );
}
#[test]
#[ignore = "requires official export; mandatory CI"]
fn official_lexical_flag_conditions() {
    let path = "../../fixtures/dependency-match-flags-v1.expected.json";
    let fixture: Fixture = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let total = |count: fn(&Case) -> usize| fixture.cases.iter().map(count).sum::<usize>();
    assert_eq!(total(|c| c.tokens.len()), 65);
    assert_eq!(total(|c| c.rules.len()), 42);
    assert_eq!(total(|c| c.expected.len()), 376);
    for size in ["sm", "md", "lg"] {
        let lexicon = spars::Model::load(format!("../../assets/en_core_web_{size}-3.8.0"))
            .unwrap()
            .lexicon();
        check_with(path, 3, None, Some(&lexicon));
    }
}
#[test]
#[ignore = "requires official export; mandatory CI"]
fn official_more_lexical_flag_conditions() {
    let path = "../../fixtures/dependency-match-more-flags-v1.expected.json";
    let fixture: Fixture = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let total = |count: fn(&Case) -> usize| fixture.cases.iter().map(count).sum::<usize>();
    assert_eq!(total(|c| c.tokens.len()), 98);
    assert_eq!(total(|c| c.rules.len()), 90);
    assert_eq!(total(|c| c.expected.len()), 1209);
    for size in ["sm", "md", "lg"] {
        let lexicon = spars::Model::load(format!("../../assets/en_core_web_{size}-3.8.0"))
            .unwrap()
            .lexicon();
        check_with(path, 3, None, Some(&lexicon));
    }
}
#[test]
#[ignore = "requires official export; mandatory CI"]
fn reused_flag_matcher_keeps_documents_independent() {
    let model = spars::Model::load("../../assets/en_core_web_md-3.8.0").unwrap();
    let pattern: DependencyPattern = serde_json::from_str(
        r#"{"nodes":[{"id":"n","constraints":[{"attribute":"like_num","predicate":{"kind":"flag","value":true}}],"link":null}]}"#,
    )
    .unwrap();
    let mut matcher = DependencyMatcher::with_lexicon(model.lexicon());
    matcher.add("number", vec![pattern]).unwrap();
    let found = |text: &str| {
        let doc = model.process(text).unwrap();
        matcher
            .find_matches(&doc)
            .unwrap()
            .into_iter()
            .map(|m| m.tokens.iter().map(|t| t.0).collect::<Vec<_>>())
            .collect::<Vec<_>>()
    };
    assert_eq!(found("ten apples"), [vec![0]]);
    assert_eq!(found("I ate 5 of ten"), [vec![2], vec![4]]);
    assert_eq!(found("ten apples"), [vec![0]]);
}
#[test]
fn official_length_conditions() {
    let path = "../../fixtures/dependency-match-length-v1.expected.json";
    let fixture: Fixture = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let total = |count: fn(&Case) -> usize| fixture.cases.iter().map(count).sum::<usize>();
    assert_eq!(total(|c| c.tokens.len()), 28);
    assert_eq!(total(|c| c.rules.len()), 96);
    assert_eq!(total(|c| c.expected.len()), 321);
    check(path, 3, None);
}
#[test]
fn official_set_comparisons() {
    let path = "../../fixtures/dependency-match-sets-v1.expected.json";
    let fixture: Fixture = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let total = |count: fn(&Case) -> usize| fixture.cases.iter().map(count).sum::<usize>();
    assert_eq!(total(|c| c.tokens.len()), 21);
    assert_eq!(total(|c| c.rules.len()), 546);
    assert_eq!(total(|c| c.expected.len()), 587);
    check(path, 3, None);
}
#[test]
fn official_morphology_normalization() {
    let path = "../../fixtures/dependency-match-morph-normalization-v1.expected.json";
    let fixture: Fixture = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let total = |count: fn(&Case) -> usize| fixture.cases.iter().map(count).sum::<usize>();
    assert_eq!(total(|c| c.tokens.len()), 5);
    assert_eq!(total(|c| c.rules.len()), 66);
    assert_eq!(total(|c| c.expected.len()), 80);
    check(path, 1, None);
}
#[test]
fn noncanonical_document_morphology_is_rejected_by_find_matches() {
    let doc = |morphology: &str| {
        Doc::from_json(
            &serde_json::json!({"format_version": 1, "document": {"text": "a", "tokens": [{
                "start": 0, "end": 1, "idx": 0, "whitespace": false, "norm": "a", "head": 0,
                "dep": "ROOT", "morphology": morphology,
            }]}})
            .to_string(),
        )
        .unwrap()
    };
    for predicate in [
        r#"{"kind":"is_superset","values":["Case=Nom"]}"#,
        r#"{"kind":"equals","value":"Case=Nom"}"#,
    ] {
        let pattern: DependencyPattern = serde_json::from_str(&format!(
            r#"{{"nodes":[{{"id":"a","constraints":[{{"attribute":"morphology","predicate":{predicate}}}],"link":null}}]}}"#
        ))
        .unwrap();
        let mut matcher = DependencyMatcher::new();
        matcher.add("rule", vec![pattern]).unwrap();
        for noncanonical in [
            "Case=Nom|Case2=Acc",
            "Case=Acc|Case=Nom",
            "poſ=NOUN",
            "POS=noun",
            "Case=Nom,Acc",
        ] {
            assert!(
                matches!(
                    matcher.find_matches(&doc(noncanonical)),
                    Err(spars::Error::Unsupported(_))
                ),
                "{noncanonical}"
            );
        }
        // spaCy's own order for the same fields is accepted.
        let canonical = matcher.find_matches(&doc("Case2=Acc|Case=Nom")).unwrap();
        assert_eq!(canonical.len(), usize::from(predicate.contains("superset")));
    }
}
#[test]
fn reused_length_matcher_checks_child_nodes_per_document() {
    // Every token attaches to token 0, the root.
    let snapshot = |words: &[&str]| {
        let text = words.join(" ");
        let mut start = 0;
        let tokens: Vec<serde_json::Value> = words
            .iter()
            .enumerate()
            .map(|(index, word)| {
                let token = serde_json::json!({
                    "start": start, "end": start + word.len(), "idx": text[..start].chars().count(),
                    "whitespace": start + word.len() < text.len(), "norm": word, "head": 0,
                    "dep": if index == 0 { "ROOT" } else { "dep" },
                });
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
    // LENGTH appears only on the child node; the unconstrained root comes first.
    let pattern: DependencyPattern = serde_json::from_str(
        r#"{"nodes":[{"id":"head","constraints":[],"link":null},{"id":"child","constraints":[{"attribute":"length","predicate":{"kind":"compare","operator":">=","value":3}}],"link":{"left":"head","relation":">"}}]}"#,
    )
    .unwrap();
    let mut matcher = DependencyMatcher::new();
    matcher.add("long_child", vec![pattern]).unwrap();
    let found = |doc: &Doc| {
        matcher
            .find_matches(doc)
            .unwrap()
            .into_iter()
            .map(|m| m.tokens.iter().map(|t| t.0).collect::<Vec<_>>())
            .collect::<Vec<_>>()
    };
    let first = snapshot(&["root", "ab", "abcd"]);
    let second = snapshot(&["r", "abc", "x"]);
    assert_eq!(found(&first), [vec![0, 2]]);
    assert_eq!(found(&second), [vec![0, 1]]);
    assert_eq!(found(&first), [vec![0, 2]]);
}
#[test]
fn official_unicode_lower_conditions() {
    let path = "../../fixtures/dependency-match-lower-v1.expected.json";
    let fixture: Fixture = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let total = |count: fn(&Case) -> usize| fixture.cases.iter().map(count).sum::<usize>();
    assert_eq!(total(|c| c.tokens.len()), 43);
    assert_eq!(total(|c| c.rules.len()), 160);
    assert_eq!(total(|c| c.expected.len()), 223);
    check(path, 4, None);
}
fn check(path: &str, cases: usize, additions: Option<usize>) {
    check_with(path, cases, additions, None);
}
fn check_with(
    path: &str,
    cases: usize,
    additions: Option<usize>,
    lexicon: Option<&spars::Lexicon>,
) {
    let fixture: Fixture = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(fixture.versions["spacy"], "3.8.14");
    assert_eq!(fixture.versions["thinc"], "8.3.13");
    assert_eq!(fixture.cases.len(), cases);
    let mut comparisons = 0;
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
        let mut matcher = lexicon.map_or_else(DependencyMatcher::new, |lexicon| {
            DependencyMatcher::with_lexicon(lexicon.clone())
        });
        if let Some(additions) = additions {
            assert_eq!(case.rules.len(), additions);
        }
        comparisons += case.rules.len();
        for rule in case.rules {
            matcher.add(rule.name, rule.patterns).unwrap();
        }
        if additions.is_some() {
            assert_eq!(matcher.len(), 51);
        }
        let actual = matcher.find_matches(&doc).unwrap();
        assert_eq!(actual, case.expected, "{}", case.id);
        assert_eq!(
            matcher.find_matches(&doc).unwrap(),
            actual,
            "repeated {}",
            case.id
        );
        std::thread::scope(|scope| {
            let first = scope.spawn(|| matcher.find_matches(&doc).unwrap());
            assert_eq!(first.join().unwrap(), actual);
        });
    }
    assert!(comparisons > 0);
    if additions.is_some() {
        assert_eq!(comparisons, 312);
    }
}
