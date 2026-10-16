use super::*;
use crate::{ByteOffset, CodePointOffset, Token, TokenIndex};

fn doc(length: usize) -> Doc {
    Doc {
        text: "x".repeat(length),
        tokens: (0..length)
            .map(|i| {
                Token::new(
                    ByteOffset(i),
                    ByteOffset(i + 1),
                    CodePointOffset(i),
                    "x".into(),
                )
            })
            .collect(),
        entities: None,
        sentences: None,
        noun_chunks: None,
        tensor: Vec::new(),
        span_groups: SpanGroups::default(),
        dependency_index: Default::default(),
    }
}

fn span(start: usize, end: usize) -> Span {
    Span::new(TokenIndex(start), TokenIndex(end), "")
}

fn group(bounds: &[(usize, usize)]) -> SpanGroup {
    SpanGroup::new("g", bounds.iter().map(|&(s, e)| span(s, e)).collect())
}

#[test]
fn has_overlap_compares_sorted_neighbours() {
    // Independent check: any pair of spans sharing a token, plus spaCy's handling of empty spans,
    // which overlap a span that starts before and ends after them.
    let brute = |bounds: &[(usize, usize)]| {
        bounds.iter().enumerate().any(|(i, a)| {
            bounds[i + 1..].iter().any(|b| {
                let (first, second) = if a <= b { (a, b) } else { (b, a) };
                second.0 < first.1
            })
        })
    };
    let cases: &[&[(usize, usize)]] = &[
        &[],
        &[(0, 1)],
        &[(0, 1), (1, 2)],
        &[(1, 2), (1, 2)],
        &[(4, 6), (0, 5)],
        &[(0, 6), (2, 3)],
        &[(2, 2), (2, 2)],
        &[(2, 2), (1, 3)],
        &[(1, 2), (2, 2)],
        &[(0, 4), (1, 2), (5, 6)],
    ];
    let expected = [
        false, false, false, true, true, true, false, true, false, true,
    ];
    for (bounds, expected) in cases.iter().zip(expected) {
        assert_eq!(group(bounds).has_overlap(), expected, "{bounds:?}");
        assert_eq!(brute(bounds), expected, "{bounds:?}");
    }
}

#[test]
fn attrs_keep_insertion_order_and_replace_in_place() {
    let mut attrs = SpanGroupAttrs::new();
    assert_eq!(attrs.insert("b", "1").unwrap(), None);
    assert_eq!(attrs.insert("a", "2").unwrap(), None);
    assert_eq!(attrs.insert("b", "3").unwrap().as_deref(), Some("1"));
    assert_eq!(attrs.iter().collect::<Vec<_>>(), [("b", "3"), ("a", "2")]);
    // Text that is not one JSON value is rejected without changing the attrs.
    for invalid in ["", "{", "1 2", "'a'"] {
        assert!(
            matches!(attrs.insert("b", invalid), Err(Error::Json(_))),
            "{invalid}"
        );
    }
    assert_eq!(attrs.get("b"), Some("3"));
    assert_eq!(attrs.remove("b").as_deref(), Some("3"));
    assert!(attrs.remove("b").is_none());
    assert_eq!(attrs.len(), 1);
    assert!(attrs.contains_key("a") && !attrs.contains_key("b"));
}

#[test]
fn attrs_compare_order_and_text() {
    let mut first = SpanGroupAttrs::new();
    first.insert("a", "1").unwrap();
    first.insert("b", "2").unwrap();
    let mut reordered = SpanGroupAttrs::new();
    reordered.insert("b", "2").unwrap();
    reordered.insert("a", "1").unwrap();
    assert_ne!(first, reordered);
    let mut same = SpanGroupAttrs::new();
    same.insert("a", "1").unwrap();
    same.insert("b", "2").unwrap();
    assert_eq!(first, same);
}

#[test]
fn attrs_reject_repeated_keys_and_non_objects() {
    let parsed: SpanGroupAttrs =
        serde_json::from_str(r#"{"z": {"y": 1, "x": 2}, "a": null}"#).unwrap();
    assert_eq!(
        serde_json::to_string(&parsed).unwrap(),
        r#"{"z":{"y": 1, "x": 2},"a":null}"#
    );
    assert!(serde_json::from_str::<SpanGroupAttrs>(r#"{"a": 1, "a": 2}"#).is_err());
    assert!(serde_json::from_str::<SpanGroupAttrs>("[]").is_err());
}

#[test]
fn extend_group_adds_spans_and_missing_attrs() {
    let mut target = group(&[(0, 1)]);
    target.attrs.insert("shared", r#""target""#).unwrap();
    let mut other = group(&[(1, 2), (0, 1)]);
    other.name = "other".into();
    other.attrs.insert("shared", r#""other""#).unwrap();
    other.attrs.insert("extra", "true").unwrap();
    let combined = target.concat(&other);
    assert_eq!(combined.name, "g");
    assert_eq!(combined.spans, [span(0, 1), span(1, 2), span(0, 1)]);
    let attrs: Vec<_> = combined.attrs.iter().collect();
    assert_eq!(attrs, [("shared", r#""target""#), ("extra", "true")]);
    assert_eq!(
        target.spans.len(),
        1,
        "concat leaves its operands unchanged"
    );
    // A group can be extended with a copy of itself.
    let copy = target.clone();
    target.extend_group(&copy);
    assert_eq!(target.spans, [span(0, 1), span(0, 1)]);
}

#[test]
fn set_span_group_checks_bounds_and_keeps_key_positions() {
    let mut document = doc(3);
    document.set_span_group("b", group(&[(0, 1)])).unwrap();
    document.set_span_group("a", group(&[(3, 3)])).unwrap();
    let before = document.clone();
    for bounds in [(0, 4), (2, 1), (4, 4)] {
        assert!(matches!(
            document.set_span_group("a", group(&[(0, 1), bounds])),
            Err(Error::Bounds)
        ));
        assert!(matches!(
            document.set_span_group("c", group(&[bounds])),
            Err(Error::Bounds)
        ));
        assert_eq!(document, before);
    }
    document.set_span_group("b", group(&[(1, 3)])).unwrap();
    assert_eq!(
        document.span_groups().keys().collect::<Vec<_>>(),
        ["b", "a"]
    );
    assert_eq!(document.span_groups().get("b").unwrap().spans, [span(1, 3)]);
    assert_eq!(document.remove_span_group("b").unwrap().spans, [span(1, 3)]);
    assert!(document.remove_span_group("b").is_none());
    assert_eq!(document.span_groups().keys().collect::<Vec<_>>(), ["a"]);
}

#[test]
fn empty_span_ids_are_stored_as_none() {
    let mut document = doc(2);
    let mut value = group(&[(0, 1), (1, 2)]);
    value.spans[0].id = Some(String::new());
    value.spans[1].id = Some("i".into());
    document.set_span_group("g", value).unwrap();
    let ids: Vec<_> = document
        .span_groups()
        .get("g")
        .unwrap()
        .spans
        .iter()
        .map(|s| s.id.as_deref())
        .collect();
    assert_eq!(ids, [None, Some("i")]);
}

#[test]
fn span_groups_count_towards_document_equality_and_memory() {
    let mut document = doc(2);
    let empty = document.clone();
    let bytes = document.estimated_heap_bytes();
    let mut value = group(&[(0, 1)]);
    value.attrs.insert("key", r#""value""#).unwrap();
    document.set_span_group("g", value).unwrap();
    assert_ne!(document, empty);
    assert!(document.estimated_heap_bytes() > bytes + "g".len() + "key".len());
    document.remove_span_group("g");
    assert_eq!(document, empty);
}

#[test]
fn attrs_compare_as_json_text() {
    let attrs = |json: &str| {
        let mut attrs = SpanGroupAttrs::new();
        attrs.insert("k", json).unwrap();
        attrs
    };
    assert_ne!(attrs("1"), attrs("1.0"));
    assert_ne!(attrs(r#"{"a": 1}"#), attrs(r#"{"a":1}"#));
    assert_eq!(
        attrs(" 1\n"),
        attrs("1"),
        "surrounding whitespace is dropped"
    );
}

#[test]
fn heap_estimate_counts_labels_ids_and_attr_values() {
    let long = "x".repeat(1000);
    let estimate = |label: &str, id: Option<&str>, value: &str| {
        let mut document = doc(2);
        let mut value_group = group(&[(0, 1)]);
        value_group.spans[0].label = label.into();
        value_group.spans[0].id = id.map(String::from);
        value_group.attrs.insert("k", value).unwrap();
        document.set_span_group("g", value_group).unwrap();
        document.estimated_heap_bytes()
    };
    let base = estimate("", None, r#""""#);
    assert!(estimate(&long, None, r#""""#) >= base + 1000);
    assert!(estimate("", Some(&long), r#""""#) >= base + 1000);
    assert!(estimate("", None, &format!(r#""{long}""#)) >= base + 1000);
}

#[test]
fn copies_and_snapshots_keep_empty_spans() {
    let mut document = doc(3);
    document
        .set_span_group("g", group(&[(0, 0), (1, 1), (3, 3)]))
        .unwrap();
    let copy = document.clone();
    assert_eq!(
        copy.span_groups().get("g").unwrap().spans,
        [span(0, 0), span(1, 1), span(3, 3)]
    );
    assert_eq!(
        Doc::from_json(&document.to_json().unwrap()).unwrap(),
        document
    );
}

#[test]
fn stored_groups_are_independent_values() {
    let mut document = doc(3);
    let shared = group(&[(0, 1)]);
    document.set_span_group("a", shared.clone()).unwrap();
    document.set_span_group("b", shared).unwrap();
    // An edited copy changes nothing until stored, and storing it changes only its own key.
    let mut edited = document.span_groups().get("a").unwrap().clone();
    edited.spans.push(span(1, 2));
    assert_eq!(document.span_groups().get("a").unwrap().spans, [span(0, 1)]);
    document.set_span_group("a", edited).unwrap();
    assert_eq!(
        document.span_groups().get("a").unwrap().spans,
        [span(0, 1), span(1, 2)]
    );
    assert_eq!(document.span_groups().get("b").unwrap().spans, [span(0, 1)]);
}
