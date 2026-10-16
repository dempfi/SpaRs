use super::*;
use crate::{ByteOffset, CodePointOffset, Token};
use std::sync::OnceLock;

/// A document of one-letter tokens separated by spaces, without entity annotation.
fn doc(words: &str) -> Doc {
    let text: String = words.split(' ').collect::<Vec<_>>().join(" ");
    let count = text.split(' ').count();
    let tokens = (0..count)
        .map(|i| {
            let mut token = Token::new(
                ByteOffset(2 * i),
                ByteOffset(2 * i + 1),
                CodePointOffset(2 * i),
                text[2 * i..2 * i + 1].into(),
            );
            token.whitespace = i + 1 < count;
            token
        })
        .collect();
    Doc {
        text,
        tokens,
        entities: None,
        sentences: None,
        noun_chunks: None,
        tensor: Vec::new(),
        span_groups: Default::default(),
        dependency_index: OnceLock::new(),
    }
}

fn word(text: &str) -> TokenPattern {
    serde_json::from_value(serde_json::json!({"tokens": [{
        "constraints": [{"attribute": "text", "predicate": {"kind": "equals", "value": text}}],
        "repetition": {"kind": "once"},
    }]}))
    .unwrap()
}

fn tokens(label: &str, id: Option<&str>, pattern: TokenPattern) -> SpanPattern<'static> {
    SpanPattern {
        label: label.into(),
        id: id.map(str::to_owned),
        pattern: SpanPatternKind::Tokens(pattern),
    }
}

fn phrase<'a>(label: &str, id: Option<&str>, doc: &'a Doc) -> SpanPattern<'a> {
    SpanPattern {
        label: label.into(),
        id: id.map(str::to_owned),
        pattern: SpanPatternKind::Phrase(doc),
    }
}

fn span(start: usize, end: usize, label: &str) -> Span {
    Span::new(TokenIndex(start), TokenIndex(end), label)
}

fn listed(spans: &[Span]) -> Vec<(usize, usize, &str)> {
    spans
        .iter()
        .map(|s| (s.start.0, s.end.0, s.label.as_str()))
        .collect()
}

#[test]
fn defaults_follow_the_span_ruler_factory() {
    let options = SpanRulerOptions::default();
    assert_eq!(options.spans_key.as_deref(), Some("ruler"));
    assert_eq!(options.spans_filter, None);
    assert!(!options.annotate_entities);
    assert_eq!(options.entities_filter, SpanFilter::FirstLongest);
    assert_eq!(options.phrase_attribute, PhraseAttribute::Orth);
    assert!(options.overwrite);
}

#[test]
fn invalid_additions_change_nothing() {
    let pattern = doc("a");
    let empty: TokenPattern = serde_json::from_value(serde_json::json!({"tokens": []})).unwrap();
    let flag: TokenPattern = serde_json::from_value(serde_json::json!({"tokens": [{
        "constraints": [{"attribute": "is_alpha", "predicate": {"kind": "flag", "value": true}}],
        "repetition": {"kind": "once"},
    }]}))
    .unwrap();
    let mut ruler = SpanRuler::new(SpanRulerOptions::default());
    ruler.add(&[phrase("KEEP", Some("k"), &pattern)]).unwrap();
    for invalid in [
        tokens("ORG", Some("x"), empty),
        tokens("ORG", Some("x"), flag),
    ] {
        let before = (ruler.patterns().to_vec(), ruler.labels(), ruler.ids());
        let valid = tokens("NEW", Some("n"), word("a"));
        assert!(matches!(
            ruler.add(&[valid, invalid]),
            Err(Error::Pattern(_))
        ));
        assert_eq!(
            (ruler.patterns().to_vec(), ruler.labels(), ruler.ids()),
            before
        );
        // spaCy records the labels of a failed addition; SpaRs keeps the ruler unchanged.
        assert!(!ruler.contains("NEW") && !ruler.contains("ORG"));
        assert_eq!(
            listed(&ruler.find_matches(&doc("a")).unwrap()),
            [(0, 1, "KEEP")]
        );
    }
}

#[test]
fn phrase_attributes_validate_pattern_documents() {
    let options = SpanRulerOptions {
        phrase_attribute: PhraseAttribute::Lemma,
        ..SpanRulerOptions::default()
    };
    let mut ruler = SpanRuler::new(options);
    // Lemma patterns need an annotated token, as in spaCy; valid patterns before it are discarded.
    let unannotated = doc("a");
    let rejected = [
        tokens("NEW", Some("n"), word("a")),
        phrase("X", None, &unannotated),
    ];
    assert!(matches!(
        ruler.add(&rejected),
        Err(Error::MissingAnnotation(_))
    ));
    assert!(ruler.is_empty());
    assert_eq!(ruler.find_matches(&doc("a")).unwrap(), []);
    let options = SpanRulerOptions {
        phrase_attribute: PhraseAttribute::IsAlpha,
        ..SpanRulerOptions::default()
    };
    let mut flags = SpanRuler::new(options);
    assert!(matches!(
        flags.add(&[phrase("X", None, &unannotated)]),
        Err(Error::Pattern(_))
    ));
}

#[test]
fn failed_annotation_leaves_the_document_unchanged() {
    let options = SpanRulerOptions {
        annotate_entities: true,
        ..SpanRulerOptions::default()
    };
    let mut ruler = SpanRuler::new(options);
    ruler.add(&[tokens("X", None, word("b"))]).unwrap();
    let mut input = doc("a b c");
    // An IOB tag other than B, I, O or missing, which only a direct field change can store.
    input.tokens[2].entity_iob = Some("X".into());
    input.tokens[2].entity_type = Some("Y".into());
    let before = input.clone();
    assert!(matches!(
        ruler.annotate(&mut input),
        Err(Error::Unsupported(_))
    ));
    // The span group is not written either.
    assert_eq!(input, before);
    assert!(ruler.annotated(&input).is_err());
}

#[test]
fn annotated_matches_annotate_and_keeps_the_input() {
    let options = SpanRulerOptions {
        annotate_entities: true,
        ..SpanRulerOptions::default()
    };
    let mut ruler = SpanRuler::new(options);
    let pattern = doc("b c");
    ruler.add(&[phrase("X", Some("x"), &pattern)]).unwrap();
    let input = doc("a b c");
    let copy = ruler.annotated(&input).unwrap();
    let mut edited = input.clone();
    ruler.annotate(&mut edited).unwrap();
    assert_eq!(copy, edited);
    assert!(input.span_groups().is_empty());
    let group = copy.span_groups().get("ruler").unwrap();
    assert_eq!(
        (group.name.as_str(), listed(&group.spans)),
        ("ruler", vec![(1, 3, "X")])
    );
    assert_eq!(group.spans[0].id.as_deref(), Some("x"));
}

#[test]
fn clear_keeps_options_lexicon_and_known_labels() {
    let options = SpanRulerOptions {
        spans_key: Some("sc".into()),
        ..SpanRulerOptions::default()
    };
    let mut ruler = SpanRuler::new(options.clone());
    let pattern = doc("a");
    ruler.add(&[phrase("X", None, &pattern)]).unwrap();
    ruler.clear();
    assert!(ruler.is_empty() && ruler.labels().is_empty());
    assert_eq!(ruler.options(), &options);
    // As in spaCy, the label stays known, so removing it again succeeds.
    assert!(ruler.contains("X"));
    ruler.remove("X").unwrap();
    assert!(matches!(ruler.remove("Y"), Err(Error::Pattern(_))));
    ruler.add(&[phrase("Y", None, &pattern)]).unwrap();
    let annotated = ruler.annotated(&doc("a")).unwrap();
    assert_eq!(
        listed(&annotated.span_groups().get("sc").unwrap().spans),
        [(0, 1, "Y")]
    );
}

#[test]
fn identical_spans_sort_by_string_store_hashes() {
    // ORG is a spaCy symbol with a small hash; B and ZZ hash to large values, ZZ smaller.
    let mut ruler = SpanRuler::new(SpanRulerOptions::default());
    let pattern = doc("a");
    for label in ["B", "ZZ", "ORG"] {
        ruler.add(&[phrase(label, None, &pattern)]).unwrap();
    }
    assert!(rule_key("ORG") < rule_key("ZZ") && rule_key("ZZ") < rule_key("B"));
    let found = ruler.find_matches(&doc("a")).unwrap();
    assert_eq!(listed(&found), [(0, 1, "ORG"), (0, 1, "ZZ"), (0, 1, "B")]);
}

#[test]
fn first_longest_checks_only_the_end_tokens() {
    // A shorter existing span inside a longer accepted one would be caught at its ends, so only
    // empty spans show the end-token rule: an empty span is kept unless the token at its
    // position or the one before it is taken.
    let existing = [span(0, 0, "START"), span(2, 2, "INSIDE"), span(4, 4, "END")];
    let spans = [span(1, 3, "M")];
    let kept = first_longest(&existing, &spans, 4);
    assert_eq!(listed(&kept), [(0, 0, "START"), (1, 3, "M"), (4, 4, "END")]);
}

#[test]
fn equal_candidates_keep_their_input_order() {
    let existing = [span(0, 2, "OLD")];
    let spans = [span(0, 2, "NEW")];
    assert_eq!(
        listed(&first_longest(&existing, &spans, 2)),
        [(0, 2, "OLD")]
    );
    let spans = [span(0, 1, "A"), span(1, 2, "B"), span(0, 1, "C")];
    assert_eq!(
        listed(&prioritized(&[], &spans, 2, false)),
        [(0, 1, "A"), (1, 2, "B")]
    );
}

#[test]
fn prioritize_new_removes_existing_spans_it_overlaps() {
    let existing = [
        span(0, 0, "ZERO"),
        span(0, 2, "PARTIAL"),
        span(3, 3, "INSIDE"),
        span(1, 1, "AT_START"),
        span(5, 5, "BETWEEN"),
        span(6, 7, "CLEAR"),
    ];
    // Two adjacent accepted matches: an empty span between them lies inside neither.
    let spans = [span(1, 5, "LONG"), span(5, 6, "NEXT")];
    let kept = prioritized(&existing, &spans, 7, false);
    assert_eq!(
        listed(&kept),
        [
            (0, 0, "ZERO"),
            (1, 1, "AT_START"),
            (5, 5, "BETWEEN"),
            (6, 7, "CLEAR"),
            (1, 5, "LONG"),
            (5, 6, "NEXT")
        ]
    );
    let kept = prioritized(&existing, &spans, 7, true);
    assert_eq!(listed(&kept[..6]), listed(&existing));
    assert_eq!(listed(&kept[6..]), [(5, 6, "NEXT")]);
}

#[test]
fn remove_by_empty_id_stops_patterns_without_an_id() {
    let mut ruler = SpanRuler::new(SpanRulerOptions::default());
    let (a, b, c) = (doc("a"), doc("b"), doc("c"));
    ruler
        .add(&[
            phrase("X", None, &a),
            phrase("X", Some(""), &b),
            phrase("Y", None, &c),
        ])
        .unwrap();
    assert_eq!(ruler.ids(), [""]);
    ruler.remove_by_id("").unwrap();
    // The patterns without an ID are still listed, but no longer match, whatever their label.
    assert_eq!(ruler.len(), 2);
    assert!(ruler.patterns().iter().all(|p| p.id.is_none()));
    assert_eq!(ruler.labels(), ["X", "Y"]);
    assert_eq!(ruler.find_matches(&doc("a b c")).unwrap(), []);
    assert!(matches!(ruler.remove_by_id(""), Err(Error::Pattern(_))));
    // A new pattern under the same rule matches; the listed old one still does not.
    ruler.add(&[phrase("Y", None, &b)]).unwrap();
    assert_eq!(
        listed(&ruler.find_matches(&doc("a b c")).unwrap()),
        [(1, 2, "Y")]
    );
}

#[test]
fn morphology_phrases_need_canonical_morphology() {
    let options = SpanRulerOptions {
        phrase_attribute: PhraseAttribute::Morph,
        ..SpanRulerOptions::default()
    };
    let mut ruler = SpanRuler::new(options);
    let mut pattern = doc("a");
    pattern.tokens[0].morphology = Some("Number=Sing|Case=Nom".into());
    assert!(matches!(
        ruler.add(&[tokens("T", None, word("a")), phrase("X", None, &pattern)]),
        Err(Error::Unsupported(_))
    ));
    assert!(ruler.is_empty() && !ruler.contains("X"));
}

#[test]
fn empty_or_missing_spans_key_writes_no_group() {
    for key in [None, Some(String::new())] {
        let options = SpanRulerOptions {
            spans_key: key,
            ..SpanRulerOptions::default()
        };
        let mut ruler = SpanRuler::new(options);
        let pattern = doc("a");
        ruler.add(&[phrase("X", None, &pattern)]).unwrap();
        assert!(ruler.annotated(&doc("a")).unwrap().span_groups().is_empty());
    }
}

#[test]
fn filters_keep_input_order_among_many_equal_candidates() {
    // Interleaved keys, so the sort must move candidates and an unstable sort would reorder
    // equal ones.
    let spans: Vec<Span> = (0..64)
        .map(|i| span(0, 1 + (i * 7) % 3, &format!("L{i}")))
        .collect();
    let first_longest_match = spans.iter().find(|s| s.end.0 == 3).unwrap().label.clone();
    let kept = prioritized(&[], &spans, 3, false);
    assert_eq!(listed(&kept), [(0, 3, first_longest_match.as_str())]);
    let empty: Vec<Span> = (0..64)
        .map(|i| span((i * 5) % 4, (i * 5) % 4, &format!("E{i}")))
        .collect();
    let mut expected = empty.clone();
    expected.sort_by_key(|s| s.start.0);
    assert_eq!(listed(&first_longest(&empty, &[], 4)), listed(&expected));
    // Accepted long spans come first in priority order, so ordering the result by start must
    // interleave them with the empty spans, several of which share each position.
    let long: Vec<Span> = (0..8)
        .map(|i| span(10 * i + 5, 10 * i + 7, "LONG"))
        .collect();
    let empty: Vec<Span> = (0..2000)
        .map(|i| span(10 * (i * 7 % 8), 10 * (i * 7 % 8), &format!("E{i}")))
        .collect();
    let mut expected: Vec<Span> = long.iter().chain(&empty).cloned().collect();
    expected.sort_by_key(|s| s.start.0);
    assert_eq!(listed(&first_longest(&empty, &long, 80)), listed(&expected));
}

#[test]
fn failed_remove_by_id_keeps_matching() {
    let mut ruler = SpanRuler::new(SpanRulerOptions::default());
    let pattern = doc("a");
    ruler.add(&[phrase("X", None, &pattern)]).unwrap();
    // The pattern has no ID, so no current pattern has the ID "", although its rule matches
    // under the empty ID.
    assert!(matches!(ruler.remove_by_id(""), Err(Error::Pattern(_))));
    assert_eq!(
        listed(&ruler.find_matches(&doc("a")).unwrap()),
        [(0, 1, "X")]
    );
}

#[test]
fn matching_errors_leave_the_document_unchanged() {
    let options = SpanRulerOptions {
        annotate_entities: true,
        ..SpanRulerOptions::default()
    };
    let mut ruler = SpanRuler::new(options);
    let lemma: TokenPattern = serde_json::from_value(serde_json::json!({"tokens": [{
        "constraints": [{"attribute": "lemma", "predicate": {"kind": "equals", "value": "a"}}],
        "repetition": {"kind": "once"},
    }]}))
    .unwrap();
    ruler.add(&[tokens("X", None, lemma)]).unwrap();
    let mut input = doc("a b");
    let before = input.clone();
    assert!(matches!(
        ruler.annotate(&mut input),
        Err(Error::MissingAnnotation(_))
    ));
    assert_eq!(input, before);
    assert!(matches!(
        ruler.annotated(&input),
        Err(Error::MissingAnnotation(_))
    ));
}
