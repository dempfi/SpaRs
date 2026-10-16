//! Replays spaCy 3.8.14 SpanRuler outcomes from `fixtures/span-ruler-v1.expected.json`.
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use spars::{
    Doc, Error, PhraseAttribute, RulerPatternKind, Span, SpanFilter, SpanPattern, SpanPatternKind,
    SpanRuler, SpanRulerOptions, SpanRulerPattern, Token, TokenPattern,
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
    probes: Vec<String>,
    steps: Vec<Step>,
    outcomes: Vec<Outcome>,
}
#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum Filter {
    FirstLongest,
    PrioritizeNew,
    PrioritizeExisting,
}
impl From<Filter> for SpanFilter {
    fn from(filter: Filter) -> Self {
        match filter {
            Filter::FirstLongest => SpanFilter::FirstLongest,
            Filter::PrioritizeNew => SpanFilter::PrioritizeNew,
            Filter::PrioritizeExisting => SpanFilter::PrioritizeExisting,
        }
    }
}
#[derive(Deserialize)]
struct Options {
    spans_key: Option<String>,
    spans_filter: Option<Filter>,
    annotate_entities: bool,
    entities_filter: Filter,
    phrase_attribute: PhraseAttribute,
    overwrite: bool,
}
#[derive(Deserialize, Serialize)]
struct Group {
    key: String,
    name: String,
    // Raw JSON keeps the attr key order spaCy recorded.
    #[serde(deserialize_with = "compact")]
    attrs: Box<RawValue>,
    spans: Vec<Span>,
}
/// Attr JSON without the fixture's indentation, which depends on how deeply a group is nested.
fn compact<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Box<RawValue>, D::Error> {
    let raw = Box::<RawValue>::deserialize(deserializer)?;
    let (mut text, mut quoted, mut escaped) = (String::new(), false, false);
    for c in raw.get().chars() {
        if quoted || !c.is_whitespace() {
            text.push(c);
        }
        if quoted {
            (quoted, escaped) = (escaped || c != '"', !escaped && c == '\\');
        } else {
            quoted = c == '"';
        }
    }
    RawValue::from_string(text).map_err(serde::de::Error::custom)
}

#[derive(Deserialize, Serialize)]
struct Document {
    text: String,
    tokens: Vec<Token>,
    entities: Option<Vec<Span>>,
    sentences: Option<Vec<Span>>,
    noun_chunks: Option<Vec<Span>>,
    span_groups: Vec<Group>,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Action {
    Add,
    Remove,
    RemoveById,
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
    argument: Option<String>,
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
    contained: Vec<String>,
    patterns: Vec<ListedPattern>,
    matches: Option<Vec<Span>>,
    result: Option<Document>,
}
#[derive(Deserialize)]
struct ListedPattern {
    label: String,
    id: Option<String>,
    tokens: Option<TokenPattern>,
    phrase: Option<String>,
}
#[derive(Serialize)]
struct Snapshot<'a> {
    format_version: u32,
    document: &'a Document,
}

fn doc(document: &Document) -> Doc {
    // Version 4 carries entity IDs and span groups.
    let snapshot = Snapshot {
        format_version: 4,
        document,
    };
    Doc::from_json(&serde_json::to_string(&snapshot).unwrap()).unwrap()
}

fn listed(record: &ListedPattern) -> SpanRulerPattern {
    let pattern = match (&record.tokens, &record.phrase) {
        (Some(tokens), None) => RulerPatternKind::Tokens(tokens.clone()),
        (None, Some(text)) => RulerPatternKind::Phrase(text.clone()),
        _ => panic!("a listed pattern has tokens or a phrase"),
    };
    let mut expected = SpanRulerPattern::new(record.label.clone(), pattern);
    expected.id = record.id.clone();
    expected
}

fn options(options: &Options) -> SpanRulerOptions {
    let mut result = SpanRulerOptions::default();
    result.spans_key = options.spans_key.clone();
    result.spans_filter = options.spans_filter.map(SpanFilter::from);
    result.annotate_entities = options.annotate_entities;
    result.entities_filter = options.entities_filter.into();
    result.phrase_attribute = options.phrase_attribute;
    result.overwrite = options.overwrite;
    result
}

#[test]
fn official_span_ruler_outcomes() {
    let fixture: Fixture = serde_json::from_str(
        &std::fs::read_to_string("../../fixtures/span-ruler-v1.expected.json").unwrap(),
    )
    .unwrap();
    assert_eq!(fixture.format_version, 1);
    let (mut steps, mut applications, mut matches, mut errors) = (0, 0, 0, 0);
    for case in &fixture.cases {
        let mut ruler = SpanRuler::new(options(&case.options));
        let initial = doc(&case.document);
        let mut previous = initial.clone();
        assert_eq!(case.steps.len(), case.outcomes.len(), "{}", case.id);
        for (index, (step, outcome)) in case.steps.iter().zip(&case.outcomes).enumerate() {
            let at = format!("{} step {index}", case.id);
            steps += 1;
            let removed = |result: spars::Result<()>| match result {
                Ok(()) => None,
                Err(Error::Pattern(_)) => Some("ValueError".to_owned()),
                Err(other) => panic!("{at}: unexpected error {other}"),
            };
            let error = match step.action {
                Action::Add => {
                    let phrases: Vec<Option<Doc>> = step
                        .patterns
                        .iter()
                        .map(|p| p.phrase.as_ref().map(doc))
                        .collect();
                    let patterns: Vec<SpanPattern> = step
                        .patterns
                        .iter()
                        .zip(&phrases)
                        .map(|(p, phrase)| {
                            let kind = match (&p.tokens, phrase) {
                                (Some(tokens), None) => SpanPatternKind::Tokens(tokens.clone()),
                                (None, Some(phrase)) => SpanPatternKind::Phrase(phrase),
                                _ => panic!("{at}: a pattern has tokens or a phrase"),
                            };
                            SpanPattern {
                                label: p.label.clone(),
                                id: p.id.clone(),
                                pattern: kind,
                            }
                        })
                        .collect();
                    ruler.add(&patterns).unwrap_or_else(|e| panic!("{at}: {e}"));
                    None
                }
                Action::Remove => removed(ruler.remove(step.argument.as_deref().unwrap())),
                Action::RemoveById => {
                    removed(ruler.remove_by_id(step.argument.as_deref().unwrap()))
                }
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
                    let expected = outcome.matches.as_ref().unwrap();
                    matches += expected.len();
                    assert_eq!(
                        &ruler
                            .find_matches(input)
                            .unwrap_or_else(|e| panic!("{at}: {e}")),
                        expected,
                        "{at}"
                    );
                    let annotated = ruler
                        .annotated(input)
                        .unwrap_or_else(|e| panic!("{at}: {e}"));
                    let mut edited = input.clone();
                    ruler.annotate(&mut edited).unwrap();
                    assert_eq!(annotated, edited, "{at}");
                    assert_eq!(annotated, doc(outcome.result.as_ref().unwrap()), "{at}");
                    // Snapshots keep the ruler's span groups, entities and IDs.
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
            let contained: Vec<&String> =
                case.probes.iter().filter(|p| ruler.contains(p)).collect();
            assert_eq!(
                contained,
                outcome.contained.iter().collect::<Vec<_>>(),
                "{at}"
            );
            let listed: Vec<SpanRulerPattern> = outcome.patterns.iter().map(listed).collect();
            assert_eq!(ruler.patterns(), listed, "{at}");
        }
    }
    assert_eq!(
        (fixture.cases.len(), steps, applications, matches, errors),
        (100, 378, 188, 799, 15)
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
        SpanPattern {
            label: "WORD".into(),
            id: None,
            pattern: SpanPatternKind::Tokens(alpha),
        },
        SpanPattern {
            label: "NUMBER".into(),
            id: Some("n".into()),
            pattern: SpanPatternKind::Phrase(&digits),
        },
    ];
    let mut options = SpanRulerOptions::default();
    options.phrase_attribute = PhraseAttribute::IsAlpha;
    assert!(SpanRuler::new(options.clone()).add(&patterns).is_err());
    let mut ruler = SpanRuler::with_lexicon(options, model.lexicon());
    let spans = |ruler: &SpanRuler| -> Vec<(usize, String, Option<String>)> {
        let annotated = ruler.annotated(&doc).unwrap();
        annotated
            .span_groups()
            .get("ruler")
            .unwrap()
            .spans
            .iter()
            .map(|s| (s.start.0, s.label.clone(), s.id.clone()))
            .collect()
    };
    ruler.add(&patterns).unwrap();
    let expected = vec![
        (0, "WORD".to_owned(), None),
        (1, "NUMBER".to_owned(), Some("n".to_owned())),
        (2, "WORD".to_owned(), None),
    ];
    assert_eq!(spans(&ruler), expected);
    ruler.clear();
    ruler.add(&patterns).unwrap();
    assert_eq!(spans(&ruler), expected);
}
