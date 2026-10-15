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

fn tokens(label: &str, id: Option<&str>, pattern: TokenPattern) -> EntityPattern<'static> {
    EntityPattern {
        label: label.into(),
        id: id.map(str::to_owned),
        pattern: EntityPatternKind::Tokens(pattern),
    }
}

fn phrase<'a>(label: &str, id: Option<&str>, doc: &'a Doc) -> EntityPattern<'a> {
    EntityPattern {
        label: label.into(),
        id: id.map(str::to_owned),
        pattern: EntityPatternKind::Phrase(doc),
    }
}

fn ruler() -> EntityRuler {
    EntityRuler::new(EntityRulerOptions::default()).unwrap()
}

fn labels(doc: &Doc) -> Vec<(usize, usize, &str, Option<&str>)> {
    doc.entities()
        .unwrap()
        .iter()
        .map(|e| (e.start.0, e.end.0, e.label.as_str(), e.id.as_deref()))
        .collect()
}

#[test]
fn identical_spans_take_the_first_added_rule() {
    // spaCy orders these by Python set iteration; SpaRs uses addition order.
    let input = doc("a b");
    let pattern = doc("a");
    for (first, second) in [("X", "Y"), ("Y", "X")] {
        let mut ruler = ruler();
        ruler
            .add(&[
                phrase(first, None, &pattern),
                tokens(second, Some("i"), word("a")),
            ])
            .unwrap();
        assert_eq!(
            labels(&ruler.annotated(&input).unwrap()),
            [(0, 1, first, None)]
        );
    }
    // Rules first used in an earlier addition keep their place after later ones.
    let mut ruler = ruler();
    ruler.add(&[tokens("Y", None, word("b"))]).unwrap();
    ruler.add(&[tokens("X", None, word("a"))]).unwrap();
    ruler.add(&[phrase("Y", None, &pattern)]).unwrap();
    assert_eq!(
        labels(&ruler.annotated(&input).unwrap())[0],
        (0, 1, "Y", None)
    );
}

#[test]
fn remove_drops_token_and_phrase_patterns_of_a_rule() {
    // spaCy 3.8.14 stops listing both but keeps matching the token pattern.
    let pattern = doc("a");
    let mut ruler = ruler();
    ruler
        .add(&[
            phrase("ORG", Some("i"), &pattern),
            tokens("ORG", Some("i"), word("b")),
        ])
        .unwrap();
    ruler.remove("i").unwrap();
    assert!(ruler.is_empty());
    assert_eq!(ruler.find_matches(&doc("a b")).unwrap(), []);
    assert!(ruler.remove("i").is_err());
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
    let mut ruler = ruler();
    ruler.add(&[phrase("KEEP", Some("k"), &pattern)]).unwrap();
    for invalid in [
        tokens("ORG", Some("x"), empty),
        tokens("ORG", Some("x"), flag),
    ] {
        let before = (ruler.patterns(), ruler.labels(), ruler.ids());
        let valid = tokens("NEW", Some("n"), word("a"));
        assert!(matches!(
            ruler.add(&[valid, invalid]),
            Err(Error::Pattern(_))
        ));
        assert_eq!((ruler.patterns(), ruler.labels(), ruler.ids()), before);
        let found = ruler.find_matches(&doc("a")).unwrap();
        assert_eq!(
            found.iter().map(|m| m.label.as_str()).collect::<Vec<_>>(),
            ["KEEP"]
        );
    }
}

#[test]
fn phrase_attributes_validate_pattern_documents() {
    let mut ruler = EntityRuler::new(EntityRulerOptions {
        phrase_attribute: PhraseAttribute::Lemma,
        ..EntityRulerOptions::default()
    })
    .unwrap();
    // Lemma patterns need an annotated token, as in spaCy.
    let unannotated = doc("a");
    assert!(ruler.add(&[phrase("X", None, &unannotated)]).is_err());
    assert!(ruler.is_empty());
    // A rejected phrase also discards the valid patterns before it.
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
    let mut flags = EntityRuler::new(EntityRulerOptions {
        phrase_attribute: PhraseAttribute::IsAlpha,
        ..EntityRulerOptions::default()
    })
    .unwrap();
    assert!(flags.add(&[phrase("X", None, &unannotated)]).is_err());
}

#[test]
fn empty_id_separator_is_rejected() {
    let mut options = EntityRulerOptions::default();
    options.id_separator.clear();
    assert!(matches!(EntityRuler::new(options), Err(Error::Pattern(_))));
}

#[test]
fn failed_annotation_leaves_the_document_unchanged() {
    let mut ruler = ruler();
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
    assert_eq!(input, before);
    assert!(ruler.annotated(&input).is_err());
}

#[test]
fn clear_keeps_options_and_allows_new_patterns() {
    let options = EntityRulerOptions {
        overwrite_entities: true,
        ..EntityRulerOptions::default()
    };
    let mut ruler = EntityRuler::new(options.clone()).unwrap();
    ruler.add(&[tokens("X", Some("x"), word("a"))]).unwrap();
    ruler.clear();
    assert!(ruler.is_empty() && ruler.labels().is_empty() && ruler.ids().is_empty());
    assert!(ruler.remove("x").is_err());
    assert_eq!(ruler.options(), &options);
    ruler.add(&[tokens("X", Some("x"), word("a"))]).unwrap();
    assert_eq!(
        labels(&ruler.annotated(&doc("a")).unwrap()),
        [(0, 1, "X", Some("x"))]
    );
}

#[test]
fn a_rule_added_again_ranks_after_older_rules() {
    let input = doc("a");
    let mut ruler = ruler();
    ruler.add(&[tokens("X", Some("x"), word("a"))]).unwrap();
    ruler.add(&[tokens("Y", None, word("a"))]).unwrap();
    assert_eq!(
        labels(&ruler.annotated(&input).unwrap()),
        [(0, 1, "X", Some("x"))]
    );
    ruler.remove("x").unwrap();
    ruler.add(&[tokens("X", Some("x"), word("a"))]).unwrap();
    assert_eq!(
        labels(&ruler.annotated(&input).unwrap()),
        [(0, 1, "Y", None)]
    );
}

#[test]
fn removing_an_id_with_removed_and_live_rules_changes_nothing() {
    // spaCy raises too, after it has already dropped the live rule from its pattern listing.
    let mut ruler = ruler();
    ruler.add(&[tokens("ORG", Some("a"), word("a"))]).unwrap();
    ruler.remove("a").unwrap();
    ruler.add(&[tokens("GPE", Some("a"), word("b"))]).unwrap();
    let before = (ruler.patterns(), ruler.len());
    let Err(Error::Pattern(message)) = ruler.remove("a") else {
        panic!("removing a stale rule name must fail");
    };
    assert!(message.contains("already removed"), "{message}");
    assert_eq!((ruler.patterns(), ruler.len()), before);
    let found = ruler.find_matches(&doc("a b")).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(
        (found[0].label.as_str(), found[0].id.as_deref()),
        ("GPE", Some("a"))
    );
    let Err(Error::Pattern(message)) = ruler.remove("never") else {
        panic!("removing an unknown ID must fail");
    };
    assert!(message.contains("no entity pattern"), "{message}");
}

#[test]
fn empty_labels_claim_tokens_without_adding_entities() {
    let pattern = doc("b");
    let mut ruler = EntityRuler::new(EntityRulerOptions {
        overwrite_entities: true,
        ..EntityRulerOptions::default()
    })
    .unwrap();
    ruler
        .add(&[phrase("", None, &pattern), tokens("SHORT", None, word("b"))])
        .unwrap();
    let mut input = doc("a b c");
    input
        .set_entities(&EntityUpdate {
            entities: vec![Span::new(TokenIndex(1), TokenIndex(3), "X")],
            ..EntityUpdate::default()
        })
        .unwrap();
    let annotated = ruler.annotated(&input).unwrap();
    assert_eq!(annotated.entities(), Some(&[][..]));
    assert_eq!(ruler.labels(), ["", "SHORT"]);
}
