use super::*;

#[test]
fn malformed_regex_is_rejected_during_rule_decoding() {
    let rule = r#"{"index":0,"patterns":[[{"LOWER":{"REGEX":"["}}]],"attrs":{"POS":"NOUN"}}"#;
    let error = serde_json::from_str::<crate::config::AttributeRule>(rule)
        .err()
        .expect("invalid regex must fail during rule decoding");
    let regex_error = fancy_regex::Regex::new("[").unwrap_err().to_string();
    assert!(error.to_string().contains(&regex_error), "{error}");
}

#[test]
fn constraints_combine_inclusion_exclusion_and_regex() {
    let constraint: Option<Constraint> =
        serde_json::from_str(r#"{"IN":["cat","car","dog"],"NOT_IN":["car"],"REGEX":"^ca"}"#)
            .unwrap();
    assert!(matches(&constraint, "cat").unwrap());
    for value in ["car", "dog", "cattle", ""] {
        assert!(!matches(&constraint, value).unwrap());
    }
    assert!(matches(&None, "anything").unwrap());
}

fn tagged_document(word: &str) -> Doc {
    let mut token = crate::Token::new(
        crate::ByteOffset(0),
        crate::ByteOffset(word.len()),
        crate::CodePointOffset(0),
        word.into(),
    );
    token.tag = Some("NN".into());
    Doc {
        text: word.into(),
        tokens: vec![token],
        entities: None,
        sentences: None,
        noun_chunks: None,
        tensor: Vec::new(),
        dependency_index: Default::default(),
    }
}

#[test]
fn matching_uses_original_annotations_and_last_matching_rule_wins() {
    let rules: Vec<crate::config::AttributeRule> = serde_json::from_str(
        r#"[
          {"index":0,"patterns":[[{"TAG":"NN"}]],"attrs":{"TAG":"VB","POS":"NOUN"}},
          {"index":0,"patterns":[[{"TAG":"VB"}]],"attrs":{"POS":"VERB"}},
          {"index":-1,"patterns":[[{"TAG":"NN"}]],"attrs":{"POS":"PROPN","MORPH":"_"}}
        ]"#,
    )
    .unwrap();
    let mut doc = tagged_document("cat");
    apply_rules(&rules, &mut doc, str::to_lowercase).unwrap();
    assert_eq!(doc.tokens[0].tag.as_deref(), Some("VB"));
    assert_eq!(doc.tokens[0].pos.as_deref(), Some("PROPN"));
    assert_eq!(doc.tokens[0].morphology.as_deref(), Some(""));
}

#[test]
fn lowercase_is_lazy_shared_across_rules_and_local_to_each_call() {
    let rules: Vec<crate::config::AttributeRule> = serde_json::from_str(
        r#"[
          {"index":0,"patterns":[[{"TAG":"VB","LOWER":"cat"}]],"attrs":{"POS":"VERB"}},
          {"index":0,"patterns":[[{"TAG":"NN"}]],"attrs":{"POS":"NOUN"}}
        ]"#,
    )
    .unwrap();
    let calls = std::cell::Cell::new(0);
    let lower = |s: &str| {
        calls.set(calls.get() + 1);
        s.to_lowercase()
    };
    apply_rules(&rules, &mut tagged_document("CAT"), lower).unwrap();
    assert_eq!(calls.get(), 0);
    let rules: Vec<crate::config::AttributeRule> = serde_json::from_str(
        r#"[
          {"index":0,"patterns":[[{"LOWER":"cat"}]],"attrs":{"POS":"NOUN"}},
          {"index":0,"patterns":[[{"LOWER":{"REGEX":"^cat$"}}]],"attrs":{"POS":"PROPN"}}
        ]"#,
    )
    .unwrap();
    let mut cat = tagged_document("CAT");
    apply_rules(&rules, &mut cat, lower).unwrap();
    assert_eq!(calls.get(), 1);
    assert_eq!(cat.tokens[0].pos.as_deref(), Some("PROPN"));
    let mut dog = tagged_document("DOG");
    apply_rules(&rules, &mut dog, lower).unwrap();
    assert_eq!(calls.get(), 2);
    assert_eq!(dog.tokens[0].pos, None);
    assert_eq!(dog.tokens[0].morphology.as_deref(), Some(""));
}

#[test]
fn failed_token_stops_multi_token_pattern_and_empty_documents_stay_empty() {
    let rules: Vec<crate::config::AttributeRule> = serde_json::from_str(
        r#"[{"index":-1,"patterns":[[{"TAG":"VB"},{"LOWER":"cat"}]],"attrs":{"POS":"NOUN"}}]"#,
    )
    .unwrap();
    let mut doc = tagged_document("cat");
    doc.text = "cat cat".into();
    let mut second = crate::Token::new(
        crate::ByteOffset(4),
        crate::ByteOffset(7),
        crate::CodePointOffset(4),
        "cat".into(),
    );
    second.tag = Some("NN".into());
    doc.tokens.push(second);
    let calls = std::cell::Cell::new(0);
    let lower = |s: &str| {
        calls.set(calls.get() + 1);
        s.to_lowercase()
    };
    apply_rules(&rules, &mut doc, lower).unwrap();
    assert_eq!(calls.get(), 0);
    assert!(doc.tokens.iter().all(|token| token.pos.is_none()));
    doc.text.clear();
    doc.tokens.clear();
    apply_rules(&rules, &mut doc, lower).unwrap();
    assert!(doc.tokens.is_empty());
    assert_eq!(calls.get(), 0);
}
