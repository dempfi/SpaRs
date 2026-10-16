use serde::{Deserialize, Serialize};
use spars::{Doc, Error, PhraseAttribute, PhraseMatch, PhraseMatcher, Token};
use std::collections::BTreeMap;

#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    id: String,
    attribute: PhraseAttribute,
    tokens: Vec<AnnotatedToken>,
    operations: Vec<Operation>,
    states: Vec<State>,
}
#[derive(Deserialize)]
struct AnnotatedToken {
    word: String,
    space: bool,
    norm: String,
    lemma: Option<String>,
    pos: Option<String>,
    tag: Option<String>,
    dep: Option<String>,
    morph: Option<String>,
}
#[derive(Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Action {
    Add,
    Remove,
}
#[derive(Deserialize)]
struct Operation {
    action: Action,
    rule: String,
    patterns: Vec<Vec<AnnotatedToken>>,
}
#[derive(Deserialize)]
struct State {
    error: Option<String>,
    rules: Vec<String>,
    patterns: Vec<Vec<Vec<String>>>,
    matches: Vec<PhraseMatch>,
}
#[derive(Serialize)]
struct Snapshot {
    format_version: u32,
    document: Storage,
}
#[derive(Serialize)]
struct Storage {
    text: String,
    tokens: Vec<Token>,
}

fn doc(tokens: &[AnnotatedToken]) -> Doc {
    let mut text = String::new();
    let mut stored = Vec::new();
    for token in tokens {
        let start = text.len();
        let idx = text.chars().count();
        text.push_str(&token.word);
        let mut stored_token = Token::new(
            spars::ByteOffset(start),
            spars::ByteOffset(text.len()),
            spars::CodePointOffset(idx),
            token.norm.clone(),
        );
        stored_token.whitespace = token.space;
        stored_token.tag = token.tag.clone();
        stored_token.pos = token.pos.clone();
        stored_token.morphology = token.morph.clone();
        stored_token.lemma = token.lemma.clone();
        stored_token.dep = token.dep.clone();
        stored.push(stored_token);
        if token.space {
            text.push(' ');
        }
    }
    let snapshot = Snapshot {
        format_version: 1,
        document: Storage {
            text,
            tokens: stored,
        },
    };
    Doc::from_json(&serde_json::to_string(&snapshot).unwrap()).unwrap()
}

// spaCy's name for each attribute's missing annotation, or None when it needs none.
fn required(attribute: PhraseAttribute) -> Option<&'static str> {
    match attribute {
        PhraseAttribute::Lemma => Some("lemma"),
        PhraseAttribute::Pos => Some("POS"),
        PhraseAttribute::Tag => Some("tag"),
        PhraseAttribute::Dep => Some("dependency label"),
        PhraseAttribute::Morph => Some("morphology"),
        _ => None,
    }
}
fn annotated(attribute: PhraseAttribute, token: &AnnotatedToken) -> bool {
    let value = match attribute {
        PhraseAttribute::Lemma => &token.lemma,
        PhraseAttribute::Pos => &token.pos,
        PhraseAttribute::Tag => &token.tag,
        PhraseAttribute::Dep => &token.dep,
        PhraseAttribute::Morph => &token.morph,
        _ => return true,
    };
    value.is_some()
}

#[test]
fn official_annotation_attributes_and_rule_lifecycle() {
    let fixture: Fixture = serde_json::from_str(include_str!(
        "../../../fixtures/phrase-match-annotations-v1.expected.json"
    ))
    .unwrap();
    let states: usize = fixture.cases.iter().map(|c| c.states.len()).sum();
    let outputs: usize = fixture
        .cases
        .iter()
        .flat_map(|c| &c.states)
        .map(|s| s.matches.len())
        .sum();
    let errors = fixture
        .cases
        .iter()
        .flat_map(|c| &c.states)
        .filter(|s| s.error.is_some())
        .count();
    assert_eq!(
        (fixture.cases.len(), states, outputs, errors),
        (18, 204, 705, 45)
    );
    let mut compared = 0;
    for case in &fixture.cases {
        let input = doc(&case.tokens);
        let mut matcher = PhraseMatcher::with_attribute(case.attribute);
        assert_eq!(matcher.attribute(), case.attribute);
        // spaCy registers the rule and keeps patterns that precede a rejected one;
        // SpaRs validates first and changes nothing. Until a later successful
        // addition makes them agree again, such a rule is compared only for its
        // registration in SpaRs (the value).
        let mut diverged: BTreeMap<String, bool> = BTreeMap::new();
        let mut labels: Vec<&str> = Vec::new();
        for (index, (op, expected)) in case.operations.iter().zip(&case.states).enumerate() {
            let context = format!("{} operation {index}", case.id);
            if !labels.contains(&op.rule.as_str()) {
                labels.push(&op.rule);
            }
            let was_registered = matcher.contains(&op.rule);
            let result = match op.action {
                Action::Add => {
                    let patterns: Vec<Doc> = op.patterns.iter().map(|p| doc(p)).collect();
                    matcher.add(op.rule.as_str(), &patterns.iter().collect::<Vec<_>>())
                }
                Action::Remove => {
                    assert!(!diverged.contains_key(&op.rule), "{context}");
                    matcher.remove(&op.rule)
                }
            };
            match (&expected.error, result) {
                (None, Ok(())) => {
                    diverged.remove(&op.rule);
                }
                (Some(error), Err(Error::MissingAnnotation(name))) => {
                    assert_eq!(error, "ValueError", "{context}");
                    assert_eq!(Some(name), required(case.attribute), "{context}");
                    // spaCy stores nonempty patterns until the first unannotated one.
                    let stored_before_error = op
                        .patterns
                        .iter()
                        .filter(|p| !p.is_empty())
                        .take_while(|p| p.iter().any(|t| annotated(case.attribute, t)))
                        .count();
                    if !was_registered || stored_before_error > 0 {
                        diverged.insert(op.rule.clone(), was_registered);
                    }
                }
                (expected, actual) => panic!("{context}: expected {expected:?}, got {actual:?}"),
            }
            let rules: Vec<&str> = labels
                .iter()
                .copied()
                .filter(|label| matcher.contains(label))
                .collect();
            let spacy_rules: Vec<&str> = expected
                .rules
                .iter()
                .map(String::as_str)
                .filter(|rule| diverged.get(*rule).is_none_or(|registered| *registered))
                .collect();
            assert_eq!(rules, spacy_rules, "{context}");
            assert_eq!(matcher.len(), rules.len(), "{context}");
            for (rule, stored) in expected.rules.iter().zip(&expected.patterns) {
                if diverged.contains_key(rule) {
                    continue;
                }
                let mut actual: Vec<Vec<String>> = matcher
                    .get(rule)
                    .unwrap()
                    .iter()
                    .map(|p| p.tokens().to_vec())
                    .collect();
                actual.sort();
                assert_eq!(&actual, stored, "{context} rule {rule:?}");
            }
            let filter = |found: Vec<PhraseMatch>| -> Vec<PhraseMatch> {
                found
                    .into_iter()
                    .filter(|m| !diverged.contains_key(&m.rule.0))
                    .collect()
            };
            let expected_matches = filter(expected.matches.clone());
            assert_eq!(
                filter(matcher.find_matches(&input).unwrap()),
                expected_matches,
                "{context}"
            );
            if diverged.is_empty() {
                compared += 1;
            }
            // The same matcher serves concurrent read-only calls.
            std::thread::scope(|scope| {
                let other = scope.spawn(|| matcher.find_matches(&input).unwrap());
                assert_eq!(filter(other.join().unwrap()), expected_matches, "{context}");
            });
        }
        assert!(diverged.is_empty(), "{}", case.id);
    }
    eprintln!(
        "PhraseMatcher annotations: {} cases / {states} lifecycle states / {outputs} exact ordered matches / {errors} missing-annotation errors; {compared} states compared in full",
        fixture.cases.len()
    );
}

// The fixture records spaCy's annotations. Check that SpaRs annotates the same
// input and pattern phrases identically, so its own documents match the same way.
#[test]
#[ignore = "requires official export; mandatory acceptance"]
fn model_annotations_equal_the_recorded_pipeline_tokens() {
    let model = spars::Model::load("../../assets/en_core_web_md-3.8.0").unwrap();
    let fixture: Fixture = serde_json::from_str(include_str!(
        "../../../fixtures/phrase-match-annotations-v1.expected.json"
    ))
    .unwrap();
    let case = fixture
        .cases
        .iter()
        .find(|case| case.id == "pipeline-lemma")
        .unwrap();
    let mut documents = vec![&case.tokens];
    for op in &case.operations {
        // Unannotated patterns were only tokenized; the rest were processed.
        documents.extend(
            op.patterns
                .iter()
                .filter(|p| p.iter().any(|t| t.pos.is_some())),
        );
    }
    for tokens in documents {
        let text: String = tokens
            .iter()
            .map(|t| format!("{}{}", t.word, if t.space { " " } else { "" }))
            .collect();
        let processed = model.process(&text).unwrap();
        let present = |value: &Option<String>| value.clone().filter(|v| !v.is_empty());
        let actual: Vec<_> = processed
            .tokens()
            .iter()
            .enumerate()
            .map(|(i, t)| {
                (
                    processed
                        .token_text(spars::TokenIndex(i))
                        .unwrap()
                        .to_owned(),
                    t.norm.clone(),
                    present(&t.lemma),
                    present(&t.pos),
                    present(&t.tag),
                    present(&t.dep),
                    t.morphology.clone(),
                )
            })
            .collect();
        let expected: Vec<_> = tokens
            .iter()
            .map(|t| {
                (
                    t.word.clone(),
                    t.norm.clone(),
                    t.lemma.clone(),
                    t.pos.clone(),
                    t.tag.clone(),
                    t.dep.clone(),
                    t.morph.clone(),
                )
            })
            .collect();
        assert_eq!(actual, expected, "{text:?}");
    }
}
