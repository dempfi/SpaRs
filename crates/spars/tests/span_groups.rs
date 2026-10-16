//! Replays spaCy 3.8.14 span group edits from `fixtures/span-groups-v1.expected.json`.
use serde::Deserialize;
use spars::{
    ByteOffset, CodePointOffset, Doc, Error, Span, SpanGroup, SpanGroupAttrs, Token, TokenIndex,
};
use std::collections::BTreeMap;

#[derive(Deserialize)]
struct Fixture {
    format_version: u32,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    id: String,
    words: Vec<String>,
    spaces: Vec<bool>,
    steps: Vec<Step>,
    states: Vec<State>,
}
#[derive(Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "snake_case")]
enum Action {
    Set,
    SetGroup,
    Delete,
    Append,
    Extend,
    ExtendGroup,
    Concat,
    SetItem,
    Copy,
}
#[derive(Deserialize)]
struct Step {
    action: Action,
    key: Option<String>,
    spans: Option<Vec<SpanRecord>>,
    name: Option<String>,
    attrs: Option<SpanGroupAttrs>,
    other: Option<String>,
    second: Option<String>,
    index: Option<i64>,
}
#[derive(Deserialize, PartialEq, Debug, Clone)]
struct SpanRecord {
    start: usize,
    end: usize,
    label: String,
    id: Option<String>,
}
#[derive(Deserialize)]
struct State {
    error: Option<Rejection>,
    groups: Vec<GroupRecord>,
}
#[derive(Deserialize, PartialEq, Debug, Clone, Copy)]
/// The Python exception spaCy raised; the fixture contains no other kind.
enum Rejection {
    #[serde(rename = "IndexError")]
    Index,
    #[serde(rename = "KeyError")]
    Key,
}
#[derive(Deserialize, PartialEq, Debug)]
struct GroupRecord {
    key: String,
    name: String,
    attrs: OrderedAttrs,
    spans: Vec<SpanRecord>,
    has_overlap: bool,
}

/// Attribute keys in order with their parsed values, so spacing in the fixture file does not
/// matter but spaCy's key order does.
#[derive(PartialEq, Debug)]
struct OrderedAttrs(Vec<(String, serde_json::Value)>);
impl From<&SpanGroupAttrs> for OrderedAttrs {
    fn from(attrs: &SpanGroupAttrs) -> Self {
        Self(
            attrs
                .iter()
                .map(|(k, v)| (k.to_owned(), serde_json::from_str(v).unwrap()))
                .collect(),
        )
    }
}
impl<'de> Deserialize<'de> for OrderedAttrs {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::from(&SpanGroupAttrs::deserialize(deserializer)?))
    }
}

fn document(words: &[String], spaces: &[bool]) -> Doc {
    let mut text = String::new();
    let mut code_points = 0;
    let mut tokens = Vec::new();
    for (word, &space) in words.iter().zip(spaces) {
        let mut token = Token::new(
            ByteOffset(text.len()),
            ByteOffset(text.len() + word.len()),
            CodePointOffset(code_points),
            word.to_lowercase(),
        );
        token.whitespace = space;
        text.push_str(word);
        code_points += word.chars().count();
        if space {
            text.push(' ');
            code_points += 1;
        }
        tokens.push(token);
    }
    let snapshot = serde_json::json!({"format_version": 1, "document": {
        "text": text, "tokens": tokens, "entities": null, "sentences": null, "noun_chunks": null,
    }});
    Doc::from_json(&snapshot.to_string()).unwrap()
}

fn span(record: &SpanRecord) -> Span {
    let mut span = Span::new(
        TokenIndex(record.start),
        TokenIndex(record.end),
        &record.label,
    );
    span.id = record.id.clone();
    span
}

fn record(span: &Span) -> SpanRecord {
    SpanRecord {
        start: span.start.0,
        end: span.end.0,
        label: span.label.clone(),
        id: span.id.clone(),
    }
}

fn groups(doc: &Doc) -> Vec<GroupRecord> {
    doc.span_groups()
        .iter()
        .map(|(key, group)| GroupRecord {
            key: key.into(),
            name: group.name.clone(),
            attrs: OrderedAttrs::from(&group.attrs),
            spans: group.spans.iter().map(record).collect(),
            has_overlap: group.has_overlap(),
        })
        .collect()
}

fn group(doc: &Doc, key: &str) -> Result<SpanGroup, Rejection> {
    doc.span_groups().get(key).cloned().ok_or(Rejection::Key)
}

/// Store `group`, reporting spaCy's error for spans outside the document.
fn store(doc: &mut Doc, key: &str, group: SpanGroup) -> Result<(), Rejection> {
    doc.set_span_group(key, group).map_err(|error| match error {
        Error::Bounds => Rejection::Index,
        other => panic!("unexpected error {other}"),
    })
}

fn run(doc: &mut Doc, step: &Step) -> Result<(), Rejection> {
    let key = step.key.as_deref().unwrap_or_default();
    let spans: Vec<Span> = step.spans.iter().flatten().map(span).collect();
    match step.action {
        Action::Set => store(doc, key, SpanGroup::new(key, spans)),
        Action::SetGroup => {
            let mut group = SpanGroup::new(step.name.clone().unwrap(), spans);
            group.attrs = step.attrs.clone().unwrap();
            store(doc, key, group)
        }
        Action::Delete => doc.remove_span_group(key).map(drop).ok_or(Rejection::Key),
        Action::Append | Action::Extend => {
            let mut group = group(doc, key)?;
            group.spans.extend(spans);
            store(doc, key, group)
        }
        Action::ExtendGroup => {
            let mut group = group(doc, key)?;
            group.extend_group(&self::group(doc, step.other.as_deref().unwrap())?);
            store(doc, key, group)
        }
        Action::Concat => {
            let first = group(doc, step.other.as_deref().unwrap())?;
            let second = group(doc, step.second.as_deref().unwrap())?;
            store(doc, key, first.concat(&second))
        }
        Action::SetItem => {
            // Python evaluates the new span, constructing it and checking its bounds, before
            // looking up the key and index.
            let length = doc.tokens().len();
            if spans
                .iter()
                .any(|s| s.start.0 > s.end.0 || s.end.0 > length)
            {
                return Err(Rejection::Index);
            }
            let mut group = group(doc, key)?;
            // Python indexing: negative indices count from the end.
            let index = step.index.unwrap();
            let length = group.spans.len() as i64;
            let position = if index < 0 { index + length } else { index };
            if !(0..length).contains(&position) {
                return Err(Rejection::Index);
            }
            group.spans[position as usize] = spans.into_iter().next().unwrap();
            store(doc, key, group)
        }
        Action::Copy => {
            *doc = doc.clone();
            Ok(())
        }
    }
}

#[test]
fn official_span_group_edits() {
    let fixture: Fixture = serde_json::from_str(
        &std::fs::read_to_string("../../fixtures/span-groups-v1.expected.json").unwrap(),
    )
    .unwrap();
    assert_eq!(fixture.format_version, 1);
    let mut steps = 0;
    let mut actions = BTreeMap::<String, usize>::new();
    let mut reached = BTreeMap::<String, usize>::new();
    for case in &fixture.cases {
        let mut doc = document(&case.words, &case.spaces);
        assert_eq!(case.steps.len(), case.states.len(), "{}", case.id);
        for (index, (step, expected)) in case.steps.iter().zip(&case.states).enumerate() {
            let at = format!("{} step {index}", case.id);
            let before = doc.clone();
            let result = run(&mut doc, step);
            assert_eq!(result.err(), expected.error, "{at}");
            if let Some(error) = expected.error {
                assert_eq!(doc, before, "{at}: a rejected edit changes nothing");
                *reached.entry(format!("{error:?}")).or_default() += 1;
            } else {
                *actions.entry(format!("{:?}", step.action)).or_default() += 1;
            }
            assert_eq!(groups(&doc), expected.groups, "{at}");
            for group in &expected.groups {
                *reached.entry("overlap".into()).or_default() += usize::from(group.has_overlap);
                let keys: Vec<_> = group.attrs.0.iter().map(|(k, _)| k).collect();
                *reached.entry("unsorted-attrs".into()).or_default() +=
                    usize::from(!keys.is_sorted());
            }
            let restored = Doc::from_json(&doc.to_json().unwrap()).unwrap();
            assert_eq!(restored, doc, "{at}: snapshot round trip");
            assert_eq!(
                groups(&restored),
                expected.groups,
                "{at}: snapshot keeps key order"
            );
            steps += 1;
        }
    }
    assert_eq!(steps, 412);
    for behavior in ["Index", "Key", "overlap", "unsorted-attrs"] {
        assert!(
            reached.get(behavior).is_some_and(|&n| n > 0),
            "{behavior}: {reached:?}"
        );
    }
    assert_eq!(
        actions.len(),
        9,
        "every action succeeds at least once: {actions:?}"
    );
}

#[test]
fn snapshots_use_version_four_only_with_span_groups() {
    let words: Vec<String> = ["a", "b"].map(String::from).to_vec();
    let mut doc = document(&words, &[true, false]);
    assert!(doc.to_json().unwrap().contains(r#""format_version":1"#));
    let mut group = SpanGroup::new("g", vec![Span::new(TokenIndex(0), TokenIndex(2), "X")]);
    group.attrs.insert("k", r#"{"z":1,"a":[2]}"#).unwrap();
    doc.set_span_group("g", group).unwrap();
    let json = doc.to_json().unwrap();
    assert!(json.contains(r#""format_version":4"#), "{json}");
    // Attribute values keep their JSON text, including key order inside objects.
    assert!(json.contains(r#""attrs":{"k":{"z":1,"a":[2]}}"#), "{json}");
    for older in [1, 2, 3] {
        let older = json.replace(
            r#""format_version":4"#,
            &format!(r#""format_version":{older}"#),
        );
        assert!(
            matches!(Doc::from_json(&older), Err(Error::Unsupported(_))),
            "{older}"
        );
    }
    doc.remove_span_group("g").unwrap();
    assert!(doc.to_json().unwrap().contains(r#""format_version":1"#));
}

#[test]
fn version_four_also_holds_entity_ids_and_contextual_rows() {
    let words: Vec<String> = ["a", "b"].map(String::from).to_vec();
    let mut snapshot: serde_json::Value =
        serde_json::from_str(&document(&words, &[true, false]).to_json().unwrap()).unwrap();
    snapshot["format_version"] = 3.into();
    snapshot["document"]["tokens"][0]["entity_id"] = "q".into();
    // Snapshot entities must agree with the token IOB tags and types.
    snapshot["document"]["tokens"][0]["entity_iob"] = "B".into();
    snapshot["document"]["tokens"][0]["entity_type"] = "X".into();
    snapshot["document"]["tokens"][1]["entity_iob"] = "O".into();
    snapshot["document"]["entities"] =
        serde_json::json!([{"start": 0, "end": 1, "label": "X", "id": "q"}]);
    snapshot["document"]["tensor"] = serde_json::json!([[1.0], [2.0]]);
    let mut doc = Doc::from_json(&snapshot.to_string()).unwrap();
    doc.set_span_group("g", SpanGroup::new("g", vec![]))
        .unwrap();
    let written: serde_json::Value = serde_json::from_str(&doc.to_json().unwrap()).unwrap();
    assert_eq!(written["format_version"], 4);
    assert_eq!(written["document"]["tokens"][0]["entity_id"], "q");
    assert_eq!(Doc::from_json(&doc.to_json().unwrap()).unwrap(), doc);
}

#[test]
fn groups_built_for_another_document_are_checked_when_stored() {
    // spaCy raises E855 for a group from another document; SpaRs groups are token ranges, so
    // only their bounds are checked.
    let long: Vec<String> = ["a", "b", "c", "d", "e"].map(String::from).to_vec();
    let short: Vec<String> = ["a", "b", "c"].map(String::from).to_vec();
    let mut other = document(&long, &[true; 5]);
    other
        .set_span_group(
            "g",
            SpanGroup::new("g", vec![Span::new(TokenIndex(3), TokenIndex(5), "")]),
        )
        .unwrap();
    let mut doc = document(&short, &[true; 3]);
    let mut group = SpanGroup::new("g", vec![Span::new(TokenIndex(0), TokenIndex(1), "")]);
    group.extend_group(other.span_groups().get("g").unwrap());
    let before = doc.clone();
    assert!(matches!(doc.set_span_group("g", group), Err(Error::Bounds)));
    assert_eq!(doc, before);
    // A group from the other document whose spans fit is stored unchanged.
    other
        .set_span_group(
            "fits",
            SpanGroup::new("fits", vec![Span::new(TokenIndex(1), TokenIndex(3), "X")]),
        )
        .unwrap();
    let fits = other.span_groups().get("fits").unwrap().clone();
    doc.set_span_group("fits", fits.clone()).unwrap();
    assert_eq!(doc.span_groups().get("fits"), Some(&fits));
}

#[derive(Debug)]
enum Expected {
    Bounds,
    Model,
    Json,
}

#[test]
fn snapshot_span_groups_are_validated() {
    let words: Vec<String> = ["a", "b"].map(String::from).to_vec();
    let base = document(&words, &[true, false]).to_json().unwrap();
    let with = |groups: &str| {
        base.replace(r#""format_version":1"#, r#""format_version":4"#)
            .replace(
                r#""noun_chunks":null"#,
                &format!(r#""noun_chunks":null,"span_groups":{groups}"#),
            )
    };
    let span = |start: usize, end: usize| format!(r#"{{"start":{start},"end":{end},"label":""}}"#);
    let accepted = with(&format!(
        r#"[{{"key":"a","name":"n","spans":[{}, {}]}},{{"key":"b","name":"b","attrs":{{}},"spans":[]}}]"#,
        span(2, 2),
        span(0, 2)
    ));
    let doc = Doc::from_json(&accepted).unwrap();
    assert_eq!(doc.span_groups().keys().collect::<Vec<_>>(), ["a", "b"]);
    let entry = |spans: &str| format!(r#"[{{"key":"a","name":"a","spans":[{spans}]}}]"#);
    for (groups, expected) in [
        (entry(&span(0, 3)), Expected::Bounds),
        (entry(&span(2, 1)), Expected::Bounds),
        (
            r#"[{"key":"a","name":"a","spans":[]},{"key":"a","name":"a","spans":[]}]"#.into(),
            Expected::Model,
        ),
        (
            entry(r#"{"start":0,"end":1,"label":"","id":""}"#),
            Expected::Model,
        ),
        (
            r#"[{"key":"a","name":"a","attrs":{"k":1,"k":2},"spans":[]}]"#.into(),
            Expected::Json,
        ),
        (
            r#"[{"key":"a","name":"a","attrs":[],"spans":[]}]"#.into(),
            Expected::Json,
        ),
        (r#"[{"key":"a","spans":[]}]"#.into(), Expected::Json),
        (
            r#"[{"key":"a","name":"a","spans":[],"extra":1}]"#.into(),
            Expected::Json,
        ),
        (r#"{"a":[]}"#.into(), Expected::Json),
    ] {
        let result = Doc::from_json(&with(&groups));
        let matched = match expected {
            Expected::Bounds => matches!(result, Err(Error::Bounds)),
            Expected::Model => matches!(result, Err(Error::Model(_))),
            Expected::Json => matches!(result, Err(Error::Json(_))),
        };
        assert!(matched, "{groups}: expected {expected:?}, got {result:?}");
    }
}
