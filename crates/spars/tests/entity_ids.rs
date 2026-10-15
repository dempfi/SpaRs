//! Replays spaCy 3.8.14 entity IDs through `Doc.set_ents` from `fixtures/entity-ids-v1.expected.json`.
use serde::Deserialize;
use serde_json::json;
use spars::{Doc, EntityUpdate, Error, Span};

#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    id: String,
    words: Vec<String>,
    spaces: Vec<bool>,
    initial: State,
    updates: Vec<EntityUpdate>,
    states: Vec<State>,
}
#[derive(Deserialize, Debug, PartialEq)]
struct TokenEntity {
    iob: Option<String>,
    r#type: Option<String>,
    id: Option<String>,
}
#[derive(Deserialize, Debug)]
struct State {
    error: Option<String>,
    tokens: Vec<TokenEntity>,
    entities: Option<Vec<Span>>,
}

fn fixture() -> Fixture {
    serde_json::from_str(include_str!(
        "../../../fixtures/entity-ids-v1.expected.json"
    ))
    .unwrap()
}

/// A document with the words, spaces and entity annotation of the recorded initial state.
fn doc(case: &Case) -> Doc {
    let mut text = String::new();
    let mut tokens = Vec::new();
    for ((word, space), entity) in case
        .words
        .iter()
        .zip(&case.spaces)
        .zip(&case.initial.tokens)
    {
        let start = text.len();
        let idx = text.chars().count();
        text.push_str(word);
        tokens.push(
            json!({"start": start, "end": text.len(), "idx": idx, "whitespace": space,
                           "norm": word, "entity_iob": entity.iob, "entity_type": entity.r#type,
                           "entity_id": entity.id}),
        );
        if *space {
            text.push(' ');
        }
    }
    let snapshot = json!({"format_version": 3, "document": {"text": text, "tokens": tokens,
                          "entities": case.initial.entities, "sentences": null, "noun_chunks": null}});
    Doc::from_json(&snapshot.to_string()).unwrap()
}

fn assert_state(doc: &Doc, expected: &State, context: &str) {
    let tokens: Vec<TokenEntity> = doc
        .tokens()
        .iter()
        .map(|t| TokenEntity {
            iob: t.entity_iob.clone(),
            r#type: t.entity_type.clone(),
            id: t.entity_id.clone(),
        })
        .collect();
    assert_eq!(tokens, expected.tokens, "{context}: tokens");
    assert_eq!(
        doc.entities(),
        expected.entities.as_deref(),
        "{context}: entities"
    );
}

#[test]
fn entity_ids_match_spacy() {
    let fixture = fixture();
    let updates: usize = fixture.cases.iter().map(|c| c.updates.len()).sum();
    let rejected = fixture
        .cases
        .iter()
        .flat_map(|c| &c.states)
        .filter(|s| s.error.is_some())
        .count();
    assert_eq!((fixture.cases.len(), updates, rejected), (27, 193, 2));
    for case in &fixture.cases {
        assert_eq!(case.updates.len(), case.states.len(), "{}", case.id);
        let mut document = doc(case);
        assert_state(&document, &case.initial, &format!("{} initial", case.id));
        for (step, (update, expected)) in case.updates.iter().zip(&case.states).enumerate() {
            let context = format!("{} update {step}", case.id);
            let before = document.clone();
            let copied = before.with_entities(update);
            let result = document.set_entities(update);
            match &expected.error {
                Some(_) => {
                    assert!(
                        matches!(result, Err(Error::Bounds)),
                        "{context}: {result:?}"
                    );
                    assert!(matches!(copied, Err(Error::Bounds)), "{context}");
                    assert_eq!(document, before, "{context}: unchanged");
                }
                None => {
                    result.unwrap_or_else(|e| panic!("{context}: {e}"));
                    assert_eq!(copied.unwrap(), document, "{context}: with_entities");
                }
            }
            assert_state(&document, expected, &context);
            assert_eq!(
                Doc::from_json(&document.to_json().unwrap()).unwrap(),
                document,
                "{context}: snapshot"
            );
        }
    }
}
