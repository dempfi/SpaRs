use super::*;
use std::sync::OnceLock;

fn doc(length: usize) -> Doc {
    let text: String = (0..length).map(|_| "a").collect();
    Doc {
        tokens: (0..length)
            .map(|i| {
                crate::Token::new(
                    crate::ByteOffset(i),
                    crate::ByteOffset(i + 1),
                    crate::CodePointOffset(i),
                    "a".into(),
                )
            })
            .collect(),
        text,
        entities: None,
        sentences: None,
        noun_chunks: None,
        tensor: Vec::new(),
        dependency_index: OnceLock::new(),
    }
}
fn entity(start: usize, end: usize, label: &str) -> Span {
    span(start, end, label, None)
}
fn range(start: usize, end: usize) -> TokenRange {
    TokenRange {
        start: TokenIndex(start),
        end: TokenIndex(end),
    }
}
fn tags(doc: &Doc) -> Vec<(Option<&str>, Option<&str>)> {
    doc.tokens()
        .iter()
        .map(|t| (t.entity_iob.as_deref(), t.entity_type.as_deref()))
        .collect()
}

#[test]
fn rejected_updates_change_nothing() {
    let mut document = doc(4);
    document
        .set_entities(&EntityUpdate {
            entities: vec![entity(0, 2, "X")],
            ..EntityUpdate::default()
        })
        .unwrap();
    let before = document.clone();
    for update in [
        EntityUpdate {
            entities: vec![entity(3, 2, "X")],
            ..EntityUpdate::default()
        },
        EntityUpdate {
            entities: vec![entity(3, 5, "X")],
            ..EntityUpdate::default()
        },
        // Unlabeled entities are ignored, but must still be inside the document.
        EntityUpdate {
            entities: vec![entity(3, 5, "")],
            ..EntityUpdate::default()
        },
        EntityUpdate {
            entities: vec![entity(0, 2, "X")],
            outside: vec![range(1, 3)],
            ..EntityUpdate::default()
        },
        EntityUpdate {
            missing: vec![range(0, 1)],
            blocked: vec![range(0, 1)],
            default: EntityDefault::Unmodified,
            ..EntityUpdate::default()
        },
    ] {
        assert!(matches!(document.set_entities(&update), Err(Error::Bounds)));
        assert_eq!(document, before);
    }
}

#[test]
fn an_all_missing_document_has_no_entity_annotation() {
    let mut document = doc(3);
    document
        .set_entities(&EntityUpdate {
            entities: vec![entity(1, 2, "X")],
            ..EntityUpdate::default()
        })
        .unwrap();
    assert_eq!(document.entities().unwrap(), [entity(1, 2, "X")]);
    document
        .set_entities(&EntityUpdate {
            default: EntityDefault::Missing,
            ..EntityUpdate::default()
        })
        .unwrap();
    assert_eq!(document.entities(), None);
    assert!(tags(&document).iter().all(|tag| *tag == (None, None)));
}

fn tagged(tags: &[(Option<&str>, Option<&str>)]) -> Doc {
    let mut document = doc(tags.len());
    for (token, (iob, kind)) in document.tokens.iter_mut().zip(tags) {
        token.entity_iob = iob.map(Into::into);
        token.entity_type = kind.map(Into::into);
    }
    document
}
fn unmodified(entities: Vec<Span>) -> EntityUpdate {
    EntityUpdate {
        entities,
        default: EntityDefault::Unmodified,
        ..EntityUpdate::default()
    }
}

#[test]
fn invalid_current_tags_reject_real_updates_without_change() {
    // An unknown tag, and an I on the first token that the update leaves in place.
    for tags in [
        [
            (Some("B"), Some("X")),
            (Some("X"), Some("")),
            (Some("O"), Some("")),
        ],
        [
            (Some("I"), Some("X")),
            (Some("O"), Some("")),
            (Some("O"), Some("")),
        ],
    ] {
        let mut document = tagged(&tags);
        let before = document.clone();
        let update = unmodified(vec![entity(1, 3, "Y")]);
        assert!(matches!(
            document.set_entities(&update),
            Err(Error::Unsupported(_))
        ));
        assert_eq!(document, before);
    }
}

#[test]
fn invalid_inside_tags_follow_spacy_repair_when_written_or_repaired() {
    // spaCy only fails on an I it cannot repair; overwriting it is a valid update.
    let mut document = tagged(&[(Some("I"), Some("X")), (Some("O"), Some(""))]);
    document.set_entities(&EntityUpdate::default()).unwrap();
    assert_eq!(document.entities(), Some(&[][..]));
    assert_eq!(tags(&document), [(Some("O"), Some("")); 2]);
    // An I after O is repaired to B, starting an entity.
    let mut document = tagged(&[(Some("O"), Some("")), (Some("I"), Some("X"))]);
    document.set_entities(&unmodified(Vec::new())).unwrap();
    assert_eq!(document.entities().unwrap(), [entity(1, 2, "X")]);
    assert_eq!(tags(&document)[1], (Some("B"), Some("X")));
}

#[test]
fn unwritten_tokens_keep_their_stored_values() {
    // Snapshot spellings that the update does not write stay as stored, like spaCy's untouched
    // token values; written tokens use the canonical form.
    let stored = [
        (Some(""), None),
        (None, Some("X")),
        (Some("O"), None),
        (Some("B"), Some("Y")),
    ];
    let mut document = tagged(&stored);
    document
        .set_entities(&unmodified(vec![entity(3, 4, "Z")]))
        .unwrap();
    assert_eq!(tags(&document)[..3], stored[..3]);
    assert_eq!(tags(&document)[3], (Some("B"), Some("Z")));
    assert_eq!(document.entities().unwrap(), [entity(3, 4, "Z")]);
    document
        .set_entities(&EntityUpdate {
            missing: vec![range(1, 2)],
            ..EntityUpdate::default()
        })
        .unwrap();
    assert_eq!(
        tags(&document),
        [
            (Some("O"), Some("")),
            (None, None),
            (Some("O"), Some("")),
            (Some("O"), Some(""))
        ]
    );
}

#[test]
fn an_empty_document_stays_annotated() {
    let mut document = doc(0);
    for default in [
        EntityDefault::Missing,
        EntityDefault::Unmodified,
        EntityDefault::Outside,
    ] {
        document
            .set_entities(&EntityUpdate {
                default,
                ..EntityUpdate::default()
            })
            .unwrap();
        assert_eq!(document.entities(), Some(&[][..]));
    }
}

#[test]
fn update_json_uses_defaults_and_rejects_unknown_fields() {
    let update: EntityUpdate = serde_json::from_str(
        r#"{"entities":[{"start":0,"end":1,"label":"X"}],"default":"missing"}"#,
    )
    .unwrap();
    assert_eq!(update.default, EntityDefault::Missing);
    assert!(update.blocked.is_empty());
    let json = serde_json::to_value(&update).unwrap();
    assert_eq!(json["default"], "missing");
    assert_eq!(
        serde_json::from_value::<EntityUpdate>(json).unwrap(),
        update
    );
    for malformed in [
        r#"{"ents":[]}"#,
        r#"{"default":"keep"}"#,
        r#"{"entities":[{"start":0,"end":1,"label":"X","kb_id":"Q1"}]}"#,
        r#"{"entities":[{"start":0,"end":1}]}"#,
        r#"{"blocked":[{"start":0,"end":1,"label":"X"}]}"#,
        r#"{"outside":[{"start":-1,"end":1}]}"#,
        r#"{"missing":[{"start":0.5,"end":1}]}"#,
    ] {
        assert!(
            serde_json::from_str::<EntityUpdate>(malformed).is_err(),
            "{malformed}"
        );
    }
}
