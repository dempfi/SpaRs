//! Replays spaCy 3.8.14 EntityRuler outcomes from `fixtures/entity-ruler-v1.expected.json`.
use serde::{Deserialize, Serialize};
use spars::{
    Doc, EntityMatch, EntityPattern, EntityPatternKind, EntityRuler, EntityRulerOptions, Error,
    PhraseAttribute, RulerPattern, RulerPatternKind, Span, Token, TokenIndex, TokenPattern,
};

#[derive(Deserialize)]
struct Fixture {
    format_version: u32,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    id: String,
    options: Options,
    document: Document,
    steps: Vec<Step>,
    outcomes: Vec<Outcome>,
}
#[derive(Deserialize)]
struct Options {
    overwrite_entities: bool,
    phrase_attribute: PhraseAttribute,
    id_separator: String,
}
#[derive(Deserialize, Serialize)]
struct Document {
    text: String,
    tokens: Vec<Token>,
    entities: Option<Vec<Span>>,
    sentences: Option<Vec<Span>>,
    noun_chunks: Option<Vec<Span>>,
}
#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum Action {
    Add,
    Remove,
    Clear,
    Apply,
}
#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum Input {
    Initial,
    Previous,
}
#[derive(Deserialize)]
struct Step {
    action: Action,
    patterns: Vec<PatternRecord>,
    id: Option<String>,
    input: Option<Input>,
}
#[derive(Deserialize)]
struct PatternRecord {
    label: String,
    id: Option<String>,
    tokens: Option<TokenPattern>,
    phrase: Option<Document>,
}
#[derive(Deserialize)]
struct Outcome {
    error: Option<String>,
    length: usize,
    labels: Vec<String>,
    ids: Vec<String>,
    patterns: Vec<ListedPattern>,
    matches: Option<Vec<MatchRecord>>,
    result: Option<Document>,
}
#[derive(Deserialize)]
struct ListedPattern {
    label: String,
    id: Option<String>,
    tokens: Option<TokenPattern>,
    phrase: Option<String>,
}
#[derive(Deserialize)]
struct MatchRecord {
    label: String,
    id: Option<String>,
    start: usize,
    end: usize,
}
#[derive(Serialize)]
struct Snapshot<'a> {
    format_version: u32,
    document: &'a Document,
}

fn doc(document: &Document) -> Doc {
    let ids = document.tokens.iter().any(|t| t.entity_id.is_some())
        || document.entities.iter().flatten().any(|e| e.id.is_some());
    let snapshot = Snapshot {
        format_version: if ids { 3 } else { 1 },
        document,
    };
    Doc::from_json(&serde_json::to_string(&snapshot).unwrap()).unwrap()
}

fn listed(record: &ListedPattern) -> RulerPattern {
    let pattern = match (&record.tokens, &record.phrase) {
        (Some(tokens), None) => RulerPatternKind::Tokens(tokens.clone()),
        (None, Some(text)) => RulerPatternKind::Phrase(text.clone()),
        _ => panic!("a listed pattern has tokens or a phrase"),
    };
    let mut expected = RulerPattern::new(record.label.clone(), pattern);
    expected.id = record.id.clone();
    expected
}

fn matched(record: &MatchRecord) -> EntityMatch {
    let mut expected = EntityMatch::new(
        record.label.clone(),
        TokenIndex(record.start),
        TokenIndex(record.end),
    );
    expected.id = record.id.clone();
    expected
}

#[test]
fn official_entity_ruler_outcomes() {
    let fixture: Fixture = serde_json::from_str(
        &std::fs::read_to_string("../../fixtures/entity-ruler-v1.expected.json").unwrap(),
    )
    .unwrap();
    assert_eq!(fixture.format_version, 1);
    let (mut steps, mut applications, mut matches, mut errors) = (0, 0, 0, 0);
    for case in &fixture.cases {
        let mut options = EntityRulerOptions::default();
        options.overwrite_entities = case.options.overwrite_entities;
        options.phrase_attribute = case.options.phrase_attribute;
        options.id_separator = case.options.id_separator.clone();
        let mut ruler = EntityRuler::new(options).unwrap();
        let initial = doc(&case.document);
        let mut previous = initial.clone();
        assert_eq!(case.steps.len(), case.outcomes.len(), "{}", case.id);
        for (index, (step, outcome)) in case.steps.iter().zip(&case.outcomes).enumerate() {
            let at = format!("{} step {index}", case.id);
            steps += 1;
            let error = match step.action {
                Action::Add => {
                    let phrases: Vec<Option<Doc>> = step
                        .patterns
                        .iter()
                        .map(|p| p.phrase.as_ref().map(doc))
                        .collect();
                    let patterns: Vec<EntityPattern> = step
                        .patterns
                        .iter()
                        .zip(&phrases)
                        .map(|(p, phrase)| {
                            let kind = match (&p.tokens, phrase) {
                                (Some(tokens), None) => EntityPatternKind::Tokens(tokens.clone()),
                                (None, Some(phrase)) => EntityPatternKind::Phrase(phrase),
                                _ => panic!("{at}: a pattern has tokens or a phrase"),
                            };
                            EntityPattern {
                                label: p.label.clone(),
                                id: p.id.clone(),
                                pattern: kind,
                            }
                        })
                        .collect();
                    ruler.add(&patterns).unwrap();
                    None
                }
                Action::Remove => match ruler.remove(step.id.as_deref().unwrap()) {
                    Ok(()) => None,
                    Err(Error::Pattern(_)) => Some("ValueError".to_owned()),
                    Err(other) => panic!("{at}: unexpected error {other}"),
                },
                Action::Clear => {
                    ruler.clear();
                    None
                }
                Action::Apply => {
                    applications += 1;
                    let input = match step.input.as_ref().unwrap() {
                        Input::Initial => &initial,
                        Input::Previous => &previous,
                    };
                    let expected: Vec<EntityMatch> = outcome
                        .matches
                        .as_ref()
                        .unwrap()
                        .iter()
                        .map(matched)
                        .collect();
                    matches += expected.len();
                    assert_eq!(ruler.find_matches(input).unwrap(), expected, "{at}");
                    let annotated = ruler.annotated(input).unwrap();
                    let mut edited = input.clone();
                    ruler.annotate(&mut edited).unwrap();
                    assert_eq!(annotated, edited, "{at}");
                    assert_eq!(annotated, doc(outcome.result.as_ref().unwrap()), "{at}");
                    // Snapshots keep the ruler's entities and IDs.
                    assert_eq!(
                        Doc::from_json(&annotated.to_json().unwrap()).unwrap(),
                        annotated,
                        "{at}"
                    );
                    previous = annotated;
                    None
                }
            };
            errors += usize::from(error.is_some());
            assert_eq!(error, outcome.error, "{at}");
            assert_eq!(ruler.len(), outcome.length, "{at}");
            assert_eq!(ruler.labels(), outcome.labels, "{at}");
            assert_eq!(ruler.ids(), outcome.ids, "{at}");
            let listed: Vec<RulerPattern> = outcome.patterns.iter().map(listed).collect();
            assert_eq!(ruler.patterns(), listed, "{at}");
        }
    }
    assert_eq!(
        (fixture.cases.len(), steps, applications, matches, errors),
        (61, 161, 75, 176, 5)
    );
}

#[test]
#[ignore = "requires official export; mandatory CI"]
fn lexical_flags_use_the_ruler_lexicon_and_survive_clear() {
    let model = spars::Model::load("../../assets/en_core_web_md-3.8.0").unwrap();
    let doc = model.process("Hi 42 there").unwrap();
    // IS_ALPHA is false for the phrase "1", so it matches the number; the token pattern matches
    // alphabetic tokens.
    let digits = model.process("1").unwrap();
    let alpha: TokenPattern = serde_json::from_value(serde_json::json!({"tokens": [{
        "constraints": [{"attribute": "is_alpha", "predicate": {"kind": "flag", "value": true}}],
        "repetition": {"kind": "once"},
    }]}))
    .unwrap();
    let patterns = [
        EntityPattern {
            label: "WORD".into(),
            id: None,
            pattern: EntityPatternKind::Tokens(alpha),
        },
        EntityPattern {
            label: "NUMBER".into(),
            id: Some("n".into()),
            pattern: EntityPatternKind::Phrase(&digits),
        },
    ];
    let mut options = EntityRulerOptions::default();
    options.overwrite_entities = true;
    options.phrase_attribute = PhraseAttribute::IsAlpha;
    assert!(EntityRuler::new(options.clone())
        .unwrap()
        .add(&patterns)
        .is_err());
    let mut ruler = EntityRuler::with_lexicon(options, model.lexicon()).unwrap();
    let entities = |ruler: &EntityRuler| -> Vec<(usize, String, Option<String>)> {
        let annotated = ruler.annotated(&doc).unwrap();
        annotated
            .entities()
            .unwrap()
            .iter()
            .map(|e| (e.start.0, e.label.clone(), e.id.clone()))
            .collect()
    };
    ruler.add(&patterns).unwrap();
    let expected = vec![
        (0, "WORD".to_owned(), None),
        (1, "NUMBER".to_owned(), Some("n".to_owned())),
        (2, "WORD".to_owned(), None),
    ];
    assert_eq!(entities(&ruler), expected);
    ruler.clear();
    ruler.add(&patterns).unwrap();
    assert_eq!(entities(&ruler), expected);
}
