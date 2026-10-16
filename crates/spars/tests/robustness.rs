use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use spars::{ByteOffset, Doc, Error, Model, TokenIndex};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

const ASSETS: &str = "../../assets/en_core_web_md-3.8.0";

struct TempModel(PathBuf);
impl TempModel {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "spars-robustness-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        // create_dir refuses to overwrite another run's files.
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn write(&self, manifest: &Value, weights: &[u8]) {
        std::fs::write(
            self.0.join("manifest.json"),
            serde_json::to_vec(manifest).unwrap(),
        )
        .unwrap();
        // Never hard-link mutated weights to the official export.
        std::fs::write(self.0.join("weights.safetensors"), weights).unwrap();
    }
}
impl Drop for TempModel {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn assets() -> (Value, Vec<u8>) {
    let path = Path::new(ASSETS);
    (
        serde_json::from_slice(&std::fs::read(path.join("manifest.json")).unwrap()).unwrap(),
        std::fs::read(path.join("weights.safetensors")).unwrap(),
    )
}
fn load_error(path: &Path, case: &str) -> Error {
    match Model::load(path) {
        Ok(_) => panic!("accepted malformed model: {case}"),
        Err(error) => {
            assert!(!error.to_string().is_empty(), "missing diagnostic: {case}");
            error
        }
    }
}

#[test]
#[ignore = "requires official export; mandatory acceptance"]
fn malformed_resource_types_and_missing_configuration_are_rejected() {
    let (manifest, weights) = assets();
    let tmp = TempModel::new();
    let cases = [
        ("/symbols", json!([])),
        ("/norms", json!(["invalid"])),
        ("/tokenizer/rules", Value::Null),
        ("/lexical/lower", json!(1)),
        ("/lexical/stops", json!([42])),
        ("/tok2vec", Value::Null),
        ("/vector_keys", json!({"123": "row"})),
        ("/tensors", Value::Null),
    ];
    for (pointer, replacement) in cases {
        let mut invalid = manifest.clone();
        *invalid.pointer_mut(pointer).unwrap() = replacement;
        tmp.write(&invalid, &weights);
        let error = load_error(&tmp.0, pointer);
        assert!(
            matches!(error, Error::Json(_) | Error::Model(_)),
            "{pointer}: {error}"
        );
    }
    // Stop-word and TLD lookups binary-search these lists, so they must stay sorted.
    for pointer in ["/lexical/stops", "/lexical/tlds"] {
        let mut unsorted = manifest.clone();
        let list = unsorted
            .pointer_mut(pointer)
            .unwrap()
            .as_array_mut()
            .unwrap();
        assert!(list.len() > 1, "{pointer}");
        list.reverse();
        tmp.write(&unsorted, &weights);
        let error = load_error(&tmp.0, pointer);
        assert!(matches!(error, Error::Model(_)), "{pointer}: {error}");
        assert!(
            error.to_string().contains("not sorted"),
            "{pointer}: {error}"
        );
    }
    // An attribute rule may only assign a universal POS tag, as in spaCy.
    let mut lowercase = manifest.clone();
    let rule = lowercase["attribute_rules"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|rule| rule["attrs"].get("POS").is_some())
        .unwrap();
    rule["attrs"]["POS"] = json!("noun");
    tmp.write(&lowercase, &weights);
    let error = load_error(&tmp.0, "lowercase POS");
    assert!(error.to_string().contains("universal POS"), "{error}");
    let mut missing = manifest.clone();
    missing.as_object_mut().unwrap().remove("tokenizer");
    tmp.write(&missing, &weights);
    let error = load_error(&tmp.0, "missing tokenizer");
    assert!(matches!(error, Error::Json(_)), "{error}");
    assert!(
        error.to_string().contains("missing field `tokenizer`"),
        "{error}"
    );
}

#[test]
#[ignore = "requires official export; mandatory acceptance"]
fn damaged_tensor_files_reach_validation_after_checksum_is_updated() {
    let (manifest, weights) = assets();
    let tmp = TempModel::new();
    let header_length = u64::from_le_bytes(weights[..8].try_into().unwrap()) as usize;
    let header: Value = serde_json::from_slice(&weights[8..8 + header_length]).unwrap();
    let tensor = header
        .as_object()
        .unwrap()
        .iter()
        .find(|(key, _)| key.as_str() != "__metadata__")
        .unwrap();
    let data_start = 8 + header_length + tensor.1["data_offsets"][0].as_u64().unwrap() as usize;
    for (name, bytes) in [
        ("truncated header", weights[..7].to_vec()),
        ("truncated data", weights[..weights.len() - 1].to_vec()),
        ("NaN tensor", {
            let mut bytes = weights.clone();
            bytes[data_start..data_start + 4].copy_from_slice(&f32::NAN.to_le_bytes());
            bytes
        }),
        ("infinite tensor", {
            let mut bytes = weights.clone();
            bytes[data_start..data_start + 4].copy_from_slice(&f32::INFINITY.to_le_bytes());
            bytes
        }),
    ] {
        let mut invalid = manifest.clone();
        invalid["weights_sha256"] = json!(Sha256::digest(&bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>());
        tmp.write(&invalid, &bytes);
        let error = load_error(&tmp.0, name);
        assert!(matches!(error, Error::Model(_)), "{name}: {error}");
        assert!(!error.to_string().contains("checksum"), "{name}: {error}");
        if name.ends_with("tensor") {
            assert!(error.to_string().contains("nonfinite"), "{name}: {error}");
        }
    }
}

fn snapshot() -> Value {
    let token = |start, end, idx, whitespace| {
        json!({
            "start": start, "end": end, "idx": idx, "whitespace": whitespace,
            "norm": "", "tag": null, "pos": null, "morphology": null,
            "lemma": null, "head": null, "dep": null, "sentence_start": null,
            "entity_iob": null, "entity_type": null
        })
    };
    json!({"format_version": 1, "document": {
        "text": "😀 A", "tokens": [token(0, 4, 0, true), token(5, 6, 2, false)],
        "entities": null, "sentences": null, "noun_chunks": null
    }})
}

#[test]
fn malformed_snapshots_reject_unicode_offsets_indices_and_spans() {
    let valid = snapshot();
    let doc = Doc::from_json(&valid.to_string()).unwrap();
    assert_eq!(doc.token_text(TokenIndex(0)).unwrap(), "😀");
    for offset in 1..4 {
        assert!(matches!(
            doc.codepoint_offset(ByteOffset(offset)),
            Err(Error::Bounds)
        ));
    }
    let cases = [
        ("/document/tokens/0/end", json!(1)),
        ("/document/tokens/0/end", json!(0)),
        ("/document/tokens/0/end", json!(99)),
        ("/document/tokens/0/start", json!(1)),
        ("/document/tokens/0/idx", json!(1)),
        ("/document/tokens/0/head", json!(2)),
        ("/document/tokens/0/whitespace", json!(false)),
        ("/document/tokens/1/start", json!(4)),
        ("/document/tokens/1/idx", json!(5)),
        ("/document/tokens/1/whitespace", json!(true)),
        ("/document/text", json!("😀 A ")),
        ("/document/tokens", json!([])),
    ];
    for (pointer, replacement) in cases {
        let mut invalid = valid.clone();
        *invalid.pointer_mut(pointer).unwrap() = replacement;
        assert!(
            matches!(Doc::from_json(&invalid.to_string()), Err(Error::Bounds)),
            "accepted {pointer}: {invalid}"
        );
    }
    for field in ["entities", "sentences", "noun_chunks"] {
        for spans in [
            json!([{"start": 0, "end": 3, "label": "X"}]),
            json!([{"start": 1, "end": 0, "label": "X"}]),
            json!([{"start": 1, "end": 1, "label": "X"}]),
            json!([{"start": 0, "end": 2, "label": "X"}, {"start": 1, "end": 2, "label": "Y"}]),
        ] {
            let mut invalid = valid.clone();
            invalid["document"][field] = spans;
            assert!(
                matches!(Doc::from_json(&invalid.to_string()), Err(Error::Bounds)),
                "accepted {field}: {invalid}"
            );
        }
    }
    // Closed value sets: spaCy's IOB tags and universal POS tags, and the fixed span labels.
    let values = [
        ("/document/tokens/0/entity_iob", json!("X")),
        ("/document/tokens/0/entity_iob", json!("b")),
        ("/document/tokens/0/pos", json!("noun")),
        ("/document/tokens/0/pos", json!("FOO")),
    ];
    for (pointer, replacement) in values {
        let mut invalid = valid.clone();
        *invalid.pointer_mut(pointer).unwrap() = replacement;
        assert!(
            matches!(Doc::from_json(&invalid.to_string()), Err(Error::Model(_))),
            "accepted {pointer}: {invalid}"
        );
    }
    for (field, label) in [("entities", ""), ("sentences", "S"), ("noun_chunks", "")] {
        let mut invalid = valid.clone();
        invalid["document"][field] = json!([{"start": 0, "end": 1, "label": label}]);
        assert!(
            matches!(Doc::from_json(&invalid.to_string()), Err(Error::Model(_))),
            "accepted {field} label {label:?}"
        );
    }
    for (field, label) in [("entities", "X"), ("sentences", ""), ("noun_chunks", "NP")] {
        let mut snapshot = valid.clone();
        snapshot["document"][field] = json!([{"start": 0, "end": 1, "label": label}]);
        if field == "entities" {
            snapshot = tagged_snapshot(
                [(json!("B"), json!("X")), (json!("O"), json!(""))],
                snapshot["document"]["entities"].take(),
            );
        }
        assert!(
            Doc::from_json(&snapshot.to_string()).is_ok(),
            "rejected {field} label {label:?}"
        );
    }
    for (pointer, accepted) in [
        ("/document/tokens/0/entity_iob", json!("")),
        ("/document/tokens/0/entity_iob", json!("O")),
        ("/document/tokens/0/pos", json!("")),
        ("/document/tokens/0/pos", json!("SPACE")),
    ] {
        let mut snapshot = valid.clone();
        *snapshot.pointer_mut(pointer).unwrap() = accepted;
        // An entity tag is annotation, so the document stores an entity list.
        snapshot["document"]["entities"] = json!([]);
        assert!(
            Doc::from_json(&snapshot.to_string()).is_ok(),
            "rejected {pointer}"
        );
    }
    let mut unsupported = valid.clone();
    unsupported["format_version"] = json!(5);
    assert!(matches!(
        Doc::from_json(&unsupported.to_string()),
        Err(Error::Unsupported(_))
    ));
    // Entity IDs need format v3, so an older reader rejects them instead of dropping them.
    let mut ids = tagged_snapshot(
        [(json!("B"), json!("X")), (json!("O"), json!(""))],
        json!([{"start": 0, "end": 1, "label": "X", "id": "q1"}]),
    );
    ids["document"]["tokens"][0]["entity_id"] = json!("q1");
    // Either kind of ID alone needs v3.
    let mut token_only = ids.clone();
    token_only["document"]["entities"] = json!([]);
    token_only["document"]["tokens"][0]["entity_iob"] = json!("O");
    let mut entity_only = ids.clone();
    entity_only["document"]["tokens"][0]
        .as_object_mut()
        .unwrap()
        .remove("entity_id");
    for snapshot in [&mut ids, &mut token_only, &mut entity_only] {
        for version in [1, 2] {
            snapshot["format_version"] = json!(version);
            assert!(
                matches!(
                    Doc::from_json(&snapshot.to_string()),
                    Err(Error::Unsupported(_))
                ),
                "accepted IDs in v{version}: {snapshot}"
            );
        }
    }
    ids["format_version"] = json!(3);
    let restored = Doc::from_json(&ids.to_string()).unwrap();
    assert_eq!(restored.tokens()[0].entity_id.as_deref(), Some("q1"));
    assert_eq!(restored.entities().unwrap()[0].id.as_deref(), Some("q1"));
    // Writing uses v3 only when IDs are present; the valid snapshot without them stays v1.
    let written: serde_json::Value = serde_json::from_str(&restored.to_json().unwrap()).unwrap();
    assert_eq!(written["format_version"], json!(3));
    let plain: serde_json::Value = serde_json::from_str(
        &Doc::from_json(&valid.to_string())
            .unwrap()
            .to_json()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(plain["format_version"], json!(1));
    assert_eq!(
        Doc::from_json(&restored.to_json().unwrap()).unwrap(),
        restored
    );
    for (pointer, replacement) in [
        ("/document/tokens/0/entity_id", json!("")),
        ("/document/entities/0/id", json!("")),
    ] {
        let mut invalid = ids.clone();
        *invalid.pointer_mut(pointer).unwrap() = replacement;
        assert!(
            matches!(Doc::from_json(&invalid.to_string()), Err(Error::Model(_))),
            "accepted {pointer}"
        );
    }
    for (field, label) in [("sentences", ""), ("noun_chunks", "NP")] {
        let mut invalid = ids.clone();
        invalid["document"][field] = json!([{"start": 0, "end": 1, "label": label, "id": "q1"}]);
        assert!(
            matches!(Doc::from_json(&invalid.to_string()), Err(Error::Model(_))),
            "accepted {field} id"
        );
    }
    // spaCy reads an entity's ID from its first token, so a snapshot cannot disagree with it.
    let mut other_id = ids.clone();
    other_id["document"]["entities"][0]["id"] = json!("q2");
    let mut missing_entity_id = ids.clone();
    missing_entity_id["document"]["entities"][0]
        .as_object_mut()
        .unwrap()
        .remove("id");
    entity_only["format_version"] = json!(3);
    for invalid in [other_id, missing_entity_id, entity_only] {
        assert!(
            matches!(Doc::from_json(&invalid.to_string()), Err(Error::Model(_))),
            "accepted mismatched IDs: {invalid}"
        );
    }
    // A token ID alone is valid v3: a token keeps its ID after leaving an entity.
    token_only["format_version"] = json!(3);
    assert!(Doc::from_json(&token_only.to_string()).is_ok());
    // v3 also carries contextual vectors, validated as in v2, and a document with both is
    // written as v3.
    let mut tensor = ids.clone();
    let rows = tensor["document"]["tokens"].as_array().unwrap().len();
    tensor["document"]["tensor"] = json!(vec![vec![0.5_f32, 1.5]; rows]);
    let with_tensor = Doc::from_json(&tensor.to_string()).unwrap();
    let written: serde_json::Value = serde_json::from_str(&with_tensor.to_json().unwrap()).unwrap();
    assert_eq!(written["format_version"], json!(3));
    assert_eq!(
        Doc::from_json(&with_tensor.to_json().unwrap()).unwrap(),
        with_tensor
    );
    for row in [json!([0.5]), json!([0.5, f64::MAX]), json!([])] {
        let mut invalid = tensor.clone();
        invalid["document"]["tensor"][0] = row;
        assert!(
            matches!(Doc::from_json(&invalid.to_string()), Err(Error::Model(_))),
            "accepted v3 tensor {invalid}"
        );
    }
    assert!(matches!(Doc::from_json("{"), Err(Error::Json(_))));
    let mut wrong_type = valid;
    wrong_type["document"]["tokens"][0]["start"] = json!("zero");
    assert!(matches!(
        Doc::from_json(&wrong_type.to_string()),
        Err(Error::Json(_))
    ));
}

/// The two-token snapshot with entity tags `(iob, type)` per token and the given entity list.
fn tagged_snapshot(tags: [(Value, Value); 2], entities: Value) -> Value {
    let mut snapshot = snapshot();
    for (token, (iob, kind)) in tags.into_iter().enumerate() {
        snapshot["document"]["tokens"][token]["entity_iob"] = iob;
        snapshot["document"]["tokens"][token]["entity_type"] = kind;
    }
    snapshot["document"]["entities"] = entities;
    snapshot
}

#[test]
fn snapshots_reject_entities_spacy_would_not_read_from_the_tags() {
    let entity = |start, end, label| json!({"start": start, "end": end, "label": label});
    let (b, i, o, x) = (json!("B"), json!("I"), json!("O"), json!("X"));
    let (null, empty) = (Value::Null, json!(""));
    let rejected = [
        // spaCy derives `doc.ents` from the tags, so a stored list must be that derivation.
        (
            "entity without tags",
            [(null.clone(), null.clone()), (null.clone(), null.clone())],
            json!([entity(0, 1, "X")]),
        ),
        (
            "tagged entity not listed",
            [(b.clone(), x.clone()), (o.clone(), empty.clone())],
            json!([]),
        ),
        (
            "wrong entity end",
            [(b.clone(), x.clone()), (i.clone(), x.clone())],
            json!([entity(0, 1, "X")]),
        ),
        (
            "wrong entity label",
            [(b.clone(), x.clone()), (o.clone(), empty.clone())],
            json!([entity(0, 1, "Y")]),
        ),
        // A list is absent only without entity annotation, so it cannot hide a tagged entity or
        // claim that outside tags are unannotated.
        (
            "outside tags with no list",
            [(o.clone(), empty.clone()), (o.clone(), null.clone())],
            null.clone(),
        ),
        (
            "tagged entity with no list",
            [(b.clone(), x.clone()), (o.clone(), empty.clone())],
            null.clone(),
        ),
    ];
    // spaCy's `doc.ents` raises E093 for an inside tag with no open entity: on the first token,
    // after an outside or missing tag, or after a blocked token (a beginning tag without a type).
    // Each is rejected for that reason, whether the list is empty or the one a repair to `B`
    // would give.
    for (name, tags, repaired) in [
        (
            "first-token I",
            [(i.clone(), x.clone()), (o.clone(), empty.clone())],
            entity(0, 1, "X"),
        ),
        (
            "I after O",
            [(o.clone(), empty.clone()), (i.clone(), x.clone())],
            entity(1, 2, "X"),
        ),
        (
            "I after missing",
            [(null.clone(), null.clone()), (i.clone(), x.clone())],
            entity(1, 2, "X"),
        ),
        (
            "I after blocked B",
            [(b.clone(), empty.clone()), (i.clone(), x.clone())],
            entity(1, 2, "X"),
        ),
    ] {
        for entities in [json!([]), json!([repaired])] {
            let invalid = tagged_snapshot(tags.clone(), entities);
            match Doc::from_json(&invalid.to_string()) {
                Err(Error::Model(message)) if message.contains("no entity to continue") => {}
                other => panic!("{name}: {other:?} for {invalid}"),
            }
        }
    }
    let wrongly_accepted: Vec<_> = rejected
        .into_iter()
        .filter(|(_, tags, entities)| {
            let invalid = tagged_snapshot(tags.clone(), entities.clone());
            !matches!(Doc::from_json(&invalid.to_string()), Err(Error::Model(_)))
        })
        .map(|(name, _, _)| name)
        .collect();
    assert!(wrongly_accepted.is_empty(), "accepted {wrongly_accepted:?}");
    let accepted = [
        // An inside tag continues the open entity whatever its own type: spaCy 3.8.14's `Doc.ents`
        // getter (spacy/tokens/doc.pyx) takes the label from the beginning token and reads only
        // the IOB code of the tokens after it.
        (
            "I of another type",
            [(b.clone(), x.clone()), (i.clone(), json!("Y"))],
            json!([entity(0, 2, "X")]),
        ),
        (
            "two entities",
            [(b.clone(), x.clone()), (b.clone(), json!("Y"))],
            json!([entity(0, 1, "X"), entity(1, 2, "Y")]),
        ),
        (
            "blocked and outside",
            [(b.clone(), empty.clone()), (o.clone(), null.clone())],
            json!([]),
        ),
        // A missing tag starts no entity whatever its spelling or stored type.
        (
            "entity then missing",
            [(b.clone(), x.clone()), (null.clone(), x.clone())],
            json!([entity(0, 1, "X")]),
        ),
        (
            "missing then entity",
            [(empty.clone(), x.clone()), (b.clone(), x.clone())],
            json!([entity(1, 2, "X")]),
        ),
        // spaCy's `doc.ents` is empty with or without entity annotation, so either list form
        // describes tags without entities; reference fixtures store `[]` for unannotated documents.
        (
            "no annotation, no list",
            [(null.clone(), null.clone()), (empty.clone(), x.clone())],
            null.clone(),
        ),
        (
            "no annotation, empty list",
            [(null.clone(), null.clone()), (empty.clone(), x.clone())],
            json!([]),
        ),
    ];
    for (name, tags, entities) in accepted {
        let snapshot = tagged_snapshot(tags, entities);
        let restored = Doc::from_json(&snapshot.to_string());
        assert!(restored.is_ok(), "rejected {name}: {restored:?}");
    }
}

#[test]
#[ignore = "requires official export; mandatory acceptance"]
fn long_unicode_and_alternating_calls_preserve_results() {
    let model = Model::load(ASSETS).unwrap();
    let texts = [
        "Alice met Bob in London. ".repeat(200),
        "😀 e\u{301}\t\r\n\u{a0} café 👩\u{200d}💻! ".repeat(32),
        String::new(),
        "The report, which Alice reviewed, was not approved.".into(),
    ];
    let expected: Vec<_> = texts
        .iter()
        .map(|text| model.process(text).unwrap())
        .collect();
    assert!(expected[0].tokens().len() > 1_000);
    for doc in &expected {
        assert_eq!(Doc::from_json(&doc.to_json().unwrap()).unwrap(), *doc);
        assert!(doc
            .tokens()
            .iter()
            .all(|token| token.head.is_some_and(|head| head.0 < doc.tokens().len())));
    }
    for _ in 0..3 {
        for i in (0..texts.len()).rev() {
            assert_eq!(model.process(&texts[i]).unwrap(), expected[i]);
        }
    }
    assert_eq!(
        model
            .pipe(texts.iter().map(String::as_str))
            .collect::<Result<Vec<_>, _>>()
            .unwrap(),
        expected
    );
    std::thread::scope(|scope| {
        let jobs: Vec<_> = (0..8)
            .map(|i| {
                let text = &texts[i % texts.len()];
                let model = &model;
                scope.spawn(move || model.process(text).unwrap())
            })
            .collect();
        for (i, job) in jobs.into_iter().enumerate() {
            assert_eq!(job.join().unwrap(), expected[i % texts.len()]);
        }
    });
}
