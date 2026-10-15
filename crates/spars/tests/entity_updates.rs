//! Replays spaCy 3.8.14 `Doc.set_ents` results from `fixtures/entity-updates-v1.expected.json`.
use serde::{Deserialize, Serialize};
use spars::{Doc, EntityUpdate, Error, Span, Token};

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
}
#[derive(Deserialize, Debug)]
struct State {
    error: Option<String>,
    tokens: Vec<TokenEntity>,
    entities: Option<Vec<Span>>,
}
#[derive(Serialize)]
struct Snapshot<'a> {
    format_version: u32,
    document: Storage<'a>,
}
#[derive(Serialize)]
struct Storage<'a> {
    text: String,
    tokens: Vec<Token>,
    entities: Option<&'a [Span]>,
}

fn fixture() -> Fixture {
    serde_json::from_str(include_str!(
        "../../../fixtures/entity-updates-v1.expected.json"
    ))
    .unwrap()
}

/// A document with the words, spaces and entity annotation of the recorded initial state.
fn doc(case: &Case) -> Doc {
    assert_eq!(case.words.len(), case.spaces.len(), "{}", case.id);
    assert_eq!(case.words.len(), case.initial.tokens.len(), "{}", case.id);
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
        let mut token = Token::new(
            spars::ByteOffset(start),
            spars::ByteOffset(text.len()),
            spars::CodePointOffset(idx),
            word.clone(),
        );
        token.whitespace = *space;
        token.entity_iob = entity.iob.clone();
        token.entity_type = entity.r#type.clone();
        tokens.push(token);
        if *space {
            text.push(' ');
        }
    }
    let snapshot = Snapshot {
        format_version: 1,
        document: Storage {
            text,
            tokens,
            entities: case.initial.entities.as_deref(),
        },
    };
    Doc::from_json(&serde_json::to_string(&snapshot).unwrap()).unwrap()
}

fn assert_state(doc: &Doc, expected: &State, context: &str) {
    let tokens: Vec<TokenEntity> = doc
        .tokens()
        .iter()
        .map(|t| TokenEntity {
            iob: t.entity_iob.clone(),
            r#type: t.entity_type.clone(),
        })
        .collect();
    assert_eq!(tokens, expected.tokens, "{context}: token tags");
    assert_eq!(
        doc.entities(),
        expected.entities.as_deref(),
        "{context}: entities"
    );
}

#[test]
fn updates_match_spacy_set_ents() {
    let fixture = fixture();
    let updates: usize = fixture.cases.iter().map(|c| c.updates.len()).sum();
    let rejected = fixture
        .cases
        .iter()
        .flat_map(|c| &c.states)
        .filter(|s| s.error.is_some())
        .count();
    assert_eq!((fixture.cases.len(), updates, rejected), (34, 330, 22));
    for case in &fixture.cases {
        assert_eq!(case.updates.len(), case.states.len(), "{}", case.id);
        let mut document = doc(case);
        assert_state(&document, &case.initial, &format!("{} initial", case.id));
        for (step, (update, expected)) in case.updates.iter().zip(&case.states).enumerate() {
            let context = format!("{} update {step}", case.id);
            let before = document.clone();
            // The copying form gives the same result and never changes its source.
            let copied = before.with_entities(update);
            let result = document.set_entities(update);
            match (&copied, &result) {
                (Ok(copy), Ok(())) => assert_eq!(copy, &document, "{context}: with_entities"),
                (Err(a), Err(b)) => assert_eq!(a.to_string(), b.to_string(), "{context}"),
                _ => panic!("{context}: with_entities {copied:?}, set_entities {result:?}"),
            }
            match &expected.error {
                // spaCy raises IndexError for spans outside the document and ValueError for
                // overlaps; both are invalid spans here, and neither changes the document.
                Some(_) => {
                    assert!(
                        matches!(result, Err(Error::Bounds)),
                        "{context}: {result:?}"
                    );
                    assert_eq!(document, before, "{context}: unchanged");
                }
                None => result.unwrap_or_else(|e| panic!("{context}: {e}")),
            }
            assert_state(&document, expected, &context);
            // The result is a valid snapshot: restoring it keeps every annotation.
            assert_eq!(
                Doc::from_json(&document.to_json().unwrap()).unwrap(),
                document
            );
        }
    }
}

// The fixture starts from spaCy's predictions; check that SpaRs predicts the same tags, so
// editing a SpaRs document behaves like editing the recorded one.
#[test]
#[ignore = "requires official export; mandatory acceptance"]
fn model_predictions_equal_the_recorded_initial_state() {
    let model = spars::Model::load("../../assets/en_core_web_md-3.8.0").unwrap();
    let fixture = fixture();
    let case = fixture.cases.iter().find(|c| c.id == "after-ner").unwrap();
    let text: String = case
        .words
        .iter()
        .zip(&case.spaces)
        .map(|(w, s)| format!("{w}{}", if *s { " " } else { "" }))
        .collect();
    let mut document = model.process(&text).unwrap();
    assert_state(&document, &case.initial, "model prediction");
    // Everything except entity annotation is unchanged, including the contextual tensor.
    let without_entities = |doc: &Doc| {
        let mut json: serde_json::Value = serde_json::from_str(&doc.to_json().unwrap()).unwrap();
        let document = &mut json["document"];
        document["entities"] = serde_json::Value::Null;
        for token in document["tokens"].as_array_mut().unwrap() {
            token["entity_iob"] = serde_json::Value::Null;
            token["entity_type"] = serde_json::Value::Null;
        }
        json
    };
    let original = without_entities(&document);
    for (step, (update, expected)) in case.updates.iter().zip(&case.states).enumerate() {
        document.set_entities(update).unwrap();
        assert_state(&document, expected, &format!("model update {step}"));
        assert_eq!(without_entities(&document), original, "model update {step}");
    }
}
