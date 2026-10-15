use super::*;
#[test]
fn typed_options_reject_unsupported_attributes() {
    for (name, attribute) in [
        ("ORTH", PhraseAttribute::Orth),
        ("TEXT", PhraseAttribute::Text),
        ("LOWER", PhraseAttribute::Lower),
        ("NORM", PhraseAttribute::Norm),
        ("LEMMA", PhraseAttribute::Lemma),
        ("POS", PhraseAttribute::Pos),
        ("TAG", PhraseAttribute::Tag),
        ("DEP", PhraseAttribute::Dep),
        ("MORPH", PhraseAttribute::Morph),
        ("IS_ALPHA", PhraseAttribute::IsAlpha),
        ("IS_LEFT_PUNCT", PhraseAttribute::IsLeftPunct),
        ("LIKE_EMAIL", PhraseAttribute::LikeEmail),
        ("LENGTH", PhraseAttribute::Length),
    ] {
        let json = format!("\"{name}\"");
        assert_eq!(
            serde_json::from_str::<PhraseAttribute>(&json).unwrap(),
            attribute
        );
        assert_eq!(serde_json::to_string(&attribute).unwrap(), json);
    }
    for name in [
        "lemma",
        "SHAPE",
        "ENT_TYPE",
        "MORPHOLOGY",
        "ISALPHA",
        "IS_SENT_START",
        "SPACY",
    ] {
        assert!(serde_json::from_str::<PhraseAttribute>(&format!("\"{name}\"")).is_err());
    }
    assert_eq!(
        PhraseMatcher::with_attribute(PhraseAttribute::Text).attribute(),
        PhraseAttribute::Text
    );
}
#[test]
fn symbol_ids_follow_spacy() {
    assert_eq!(rule_key(""), 0);
    assert_eq!(rule_key("IS_ALPHA"), 1);
    assert_eq!(rule_key("ORTH"), 65);
}
fn doc(text: &str) -> Doc {
    Doc {
        text: text.into(),
        tokens: text
            .char_indices()
            .enumerate()
            .map(|(cp, (i, c))| {
                crate::Token::new(
                    crate::ByteOffset(i),
                    crate::ByteOffset(i + c.len_utf8()),
                    crate::CodePointOffset(cp),
                    c.to_string(),
                )
            })
            .collect(),
        entities: None,
        sentences: None,
        noun_chunks: None,
        tensor: Vec::new(),
        dependency_index: OnceLock::new(),
    }
}
#[test]
fn lookup_is_unique_nonempty_ordered_and_resets() {
    let mut matcher = PhraseMatcher::new();
    matcher
        .add("r", &[&doc("b"), &doc("a"), &doc("b"), &doc("")])
        .unwrap();
    matcher.add("r", &[&doc("c"), &doc("a")]).unwrap();
    assert_eq!(
        matcher
            .get("r")
            .unwrap()
            .iter()
            .map(|p| p.tokens().to_vec())
            .collect::<Vec<_>>(),
        vec![vec!["b"], vec!["a"], vec!["c"]]
    );
    matcher.remove("r").unwrap();
    assert!(matcher.get("r").is_none());
    matcher.add("r", &[&doc("a")]).unwrap();
    assert_eq!(matcher.get("r").unwrap()[0].tokens(), &["a"]);
    assert_eq!(matcher.get("r").unwrap().len(), 1);
}
#[test]
fn text_alias_is_exact_and_annotation_independent() {
    let mut orth = PhraseMatcher::new();
    let mut text = PhraseMatcher::with_attribute(PhraseAttribute::Text);
    for matcher in [&mut orth, &mut text] {
        matcher.add("r", &[&doc("é\n🙂")]).unwrap();
    }
    let mut input = doc("É\n🙂é\n🙂");
    input.tokens[4].sentence_start = Some(true);
    let expected = vec![PhraseMatch {
        rule: "r".into(),
        start: TokenIndex(3),
        end: TokenIndex(6),
    }];
    assert_eq!(orth.find_matches(&input).unwrap(), expected);
    assert_eq!(text.find_matches(&input).unwrap(), expected);
    let restored = Doc::from_json(&input.to_json().unwrap()).unwrap();
    assert_eq!(text.find_matches(&restored).unwrap(), expected);
}
#[test]
fn collision_rejection_is_atomic_and_recoverable() {
    let mut matcher = PhraseMatcher::new();
    matcher.add("r", &[&doc("a")]).unwrap();
    let before = matcher.find_matches(&doc("ab")).unwrap();
    let patterns = matcher.get("r").unwrap().to_vec();
    matcher
        .identities
        .insert(rule_key("candidate"), "different".into());
    assert!(matches!(
        matcher.add("candidate", &[&doc("b")]),
        Err(Error::Pattern(_))
    ));
    assert_eq!(matcher.len(), 1);
    assert_eq!(matcher.get("r").unwrap(), patterns);
    assert!(!matcher.contains("candidate"));
    assert_eq!(matcher.find_matches(&doc("ab")).unwrap(), before);
    matcher.add("usable", &[&doc("b")]).unwrap();
    assert_eq!(matcher.find_matches(&doc("ab")).unwrap().len(), 2);
}
#[test]
fn malformed_document_rejected_before_registration() {
    let matcher = PhraseMatcher::new();
    let invalid = doc("é")
        .to_json()
        .unwrap()
        .replace("\"end\":2", "\"end\":1");
    assert!(Doc::from_json(&invalid).is_err());
    assert!(matcher.is_empty());
}
#[test]
fn long_patterns_drop_and_remove_without_recursive_stack_growth() {
    let input = doc(&"a".repeat(10000));
    let mut matcher = PhraseMatcher::new();
    matcher.add("long", &[&input]).unwrap();
    matcher.remove("long").unwrap();
    assert!(matcher.nodes[0].children.is_empty());
    assert!(matcher
        .nodes
        .iter()
        .all(|n| n.children.is_empty() && n.terminal.is_none()));
}

#[test]
fn populated_deep_trie_drops_on_small_stack() {
    std::thread::Builder::new()
        .stack_size(128 * 1024)
        .spawn(|| {
            let input = doc(&"a".repeat(10000));
            let mut matcher = PhraseMatcher::new();
            matcher.add("deep", &[&input]).unwrap();
            drop(matcher);
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn lower_lookup_uses_unique_normalized_keys_and_preserves_exact_mode() {
    let lower_attribute = serde_json::from_str::<PhraseAttribute>("\"LOWER\"").unwrap();
    let mut lower = PhraseMatcher::with_attribute(lower_attribute);
    lower.add("r", &[&doc("Ab"), &doc("ab")]).unwrap();
    assert_eq!(lower.get("r").unwrap().len(), 1);
    assert_eq!(lower.get("r").unwrap()[0].tokens(), &["a", "b"]);
    assert_eq!(lower.find_matches(&doc("AB")).unwrap().len(), 1);
    let mut exact = PhraseMatcher::new();
    exact.add("r", &[&doc("ab")]).unwrap();
    assert!(exact.find_matches(&doc("AB")).unwrap().is_empty());
}

// Each token is (lemma, morphology); other annotations stay missing.
fn annotated(values: &[(Option<&str>, Option<&str>)]) -> Doc {
    let mut doc = doc(&"x".repeat(values.len()));
    for (token, (lemma, morphology)) in doc.tokens.iter_mut().zip(values) {
        token.lemma = lemma.map(Into::into);
        token.morphology = morphology.map(Into::into);
    }
    doc
}
fn spans(matcher: &PhraseMatcher, input: &Doc) -> Vec<(String, usize, usize)> {
    matcher
        .find_matches(input)
        .unwrap()
        .into_iter()
        .map(|m| (m.rule.0, m.start.0, m.end.0))
        .collect()
}
#[test]
fn an_empty_annotation_is_missing_like_spacy_key_zero() {
    // spaCy stores "" as key 0, the same as no value, so an empty lemma is missing.
    let mut matcher = PhraseMatcher::with_attribute(PhraseAttribute::Lemma);
    assert!(matches!(
        matcher.add("r", &[&annotated(&[(Some(""), None), (None, None)])]),
        Err(Error::MissingAnnotation("lemma"))
    ));
    assert!(!matcher.contains("r"));
    matcher
        .add("r", &[&annotated(&[(Some("a"), None), (Some(""), None)])])
        .unwrap();
    assert_eq!(matcher.get("r").unwrap()[0].tokens(), &["a", ""]);
    let input = annotated(&[
        (Some("a"), None),
        (None, None),
        (Some("a"), None),
        (Some("b"), None),
    ]);
    assert_eq!(spans(&matcher, &input), vec![("r".into(), 0, 2)]);
}
#[test]
fn the_empty_morphological_analysis_is_an_annotation() {
    let mut matcher = PhraseMatcher::with_attribute(PhraseAttribute::Morph);
    matcher
        .add("r", &[&annotated(&[(None, Some(""))])])
        .unwrap();
    assert_eq!(matcher.get("r").unwrap()[0].tokens(), &["_"]);
    assert!(matches!(
        matcher.add("missing", &[&annotated(&[(None, None)])]),
        Err(Error::MissingAnnotation("morphology"))
    ));
    let input = annotated(&[(None, Some("")), (None, None), (None, Some("Number=Plur"))]);
    assert_eq!(spans(&matcher, &input), vec![("r".into(), 0, 1)]);
}
#[test]
fn failed_registration_changes_nothing() {
    let mut matcher = PhraseMatcher::with_attribute(PhraseAttribute::Lemma);
    let good = annotated(&[(Some("a"), None)]);
    let bad = annotated(&[(None, None)]);
    // spaCy keeps the rule and earlier patterns; SpaRs validates every pattern first.
    assert!(matches!(
        matcher.add("r", &[&good, &bad]),
        Err(Error::MissingAnnotation("lemma"))
    ));
    assert!(matcher.is_empty());
    assert!(matcher.find_matches(&good).unwrap().is_empty());
    // Empty patterns are skipped before the annotation check.
    matcher.add("empty", &[&doc("")]).unwrap();
    assert_eq!(matcher.get("empty").unwrap().len(), 0);
}
#[test]
fn noncanonical_morphology_is_rejected_in_patterns_and_input() {
    let mut matcher = PhraseMatcher::with_attribute(PhraseAttribute::Morph);
    let noncanonical = annotated(&[(None, Some("Tense=Past|Number=Sing"))]);
    assert!(matches!(
        matcher.add("r", &[&noncanonical]),
        Err(Error::Unsupported(_))
    ));
    assert!(matcher.is_empty());
    matcher
        .add(
            "r",
            &[&annotated(&[(None, Some("Number=Sing|Tense=Past"))])],
        )
        .unwrap();
    assert!(matches!(
        matcher.find_matches(&noncanonical),
        Err(Error::Unsupported(_))
    ));
}
#[test]
fn morphology_validation_does_not_depend_on_registered_rules() {
    let noncanonical = annotated(&[(None, Some("Tense=Past|Number=Sing"))]);
    let mut matcher = PhraseMatcher::with_attribute(PhraseAttribute::Morph);
    assert!(matches!(
        matcher.find_matches(&noncanonical),
        Err(Error::Unsupported(_))
    ));
    matcher.add("empty", &[&doc("")]).unwrap();
    assert!(matches!(
        matcher.find_matches(&noncanonical),
        Err(Error::Unsupported(_))
    ));
    // A literal `_` is spaCy's string for the empty analysis, but snapshots store
    // that analysis as "", so `_` in a snapshot is rejected like other malformed values.
    assert!(matches!(
        matcher.find_matches(&annotated(&[(None, Some("_"))])),
        Err(Error::Unsupported(_))
    ));
}
#[test]
fn other_attributes_ignore_noncanonical_morphology() {
    for attribute in [
        PhraseAttribute::Norm,
        PhraseAttribute::Lemma,
        PhraseAttribute::Pos,
        PhraseAttribute::Tag,
        PhraseAttribute::Dep,
    ] {
        let mut value = annotated(&[(Some("x"), Some("Tense=Past|Number=Sing"))]);
        let token = &mut value.tokens[0];
        (token.pos, token.tag, token.dep) =
            (Some("X".into()), Some("XX".into()), Some("dep".into()));
        let mut matcher = PhraseMatcher::with_attribute(attribute);
        matcher.add("r", &[&value]).unwrap();
        assert_eq!(
            spans(&matcher, &value),
            vec![("r".into(), 0, 1)],
            "{attribute:?}"
        );
    }
}
#[test]
fn text_attributes_ignore_annotations() {
    let pattern = annotated(&[(Some("a"), Some("Number=Plur"))]);
    let input = annotated(&[(None, None)]);
    for attribute in [
        PhraseAttribute::Orth,
        PhraseAttribute::Lower,
        PhraseAttribute::Norm,
    ] {
        let mut matcher = PhraseMatcher::with_attribute(attribute);
        matcher.add("r", &[&pattern]).unwrap();
        assert_eq!(
            spans(&matcher, &input),
            vec![("r".into(), 0, 1)],
            "{attribute:?}"
        );
    }
}
