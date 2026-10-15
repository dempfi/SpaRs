//! `Doc::estimated_heap_bytes` counts the text, every token string, all three span lists and the
//! contextual tensor.
use serde_json::{json, Value};
use spars::Doc;

const STRINGS: [&str; 9] = [
    "norm",
    "tag",
    "pos",
    "morphology",
    "lemma",
    "dep",
    "entity_iob",
    "entity_type",
    "entity_id",
];

/// A snapshot with every token string, span list and, when `width > 0`, a tensor.
fn snapshot(words: usize, width: usize) -> Value {
    let tokens: Vec<_> = (0..words)
        .map(|i| {
            json!({"start": i * 5, "end": i * 5 + 4, "idx": i * 5, "whitespace": i + 1 < words,
                   "norm": "word", "tag": "NN", "pos": "NOUN", "morphology": "Number=Sing",
                   "lemma": "word", "dep": "nsubj", "head": i, "sentence_start": i == 0,
                   "entity_iob": "B", "entity_type": "PERSON", "entity_id": "q42"})
        })
        .collect();
    let spans = |label: &str| -> Value {
        (0..words)
            .map(|i| json!({"start": i, "end": i + 1, "label": label}))
            .collect()
    };
    let mut entities = spans("PERSON");
    for entity in entities.as_array_mut().unwrap() {
        entity["id"] = json!("q42");
    }
    let mut document = json!({"text": vec!["word"; words].join(" "), "tokens": tokens,
                              "entities": entities, "sentences": if words == 0 { json!([]) } else { json!([{"start": 0, "end": words, "label": ""}]) },
                              "noun_chunks": spans("NP")});
    if width > 0 {
        document["tensor"] = json!(vec![vec![0.5_f32; width]; words]);
    }
    json!({"format_version": 3, "document": document})
}

fn doc(value: &Value) -> Doc {
    Doc::from_json(&value.to_string()).unwrap()
}

/// The estimate computed independently from the snapshot, for a document whose allocations
/// have no spare capacity.
fn exact(value: &Value) -> usize {
    let document = &value["document"];
    let length = |value: &Value| value.as_str().map_or(0, str::len);
    let tokens = document["tokens"].as_array().unwrap();
    let mut bytes = length(&document["text"]) + tokens.len() * std::mem::size_of::<spars::Token>();
    for token in tokens {
        bytes += STRINGS
            .iter()
            .map(|name| length(&token[*name]))
            .sum::<usize>();
    }
    for name in ["entities", "sentences", "noun_chunks"] {
        for span in document[name].as_array().into_iter().flatten() {
            bytes +=
                std::mem::size_of::<spars::Span>() + length(&span["label"]) + length(&span["id"]);
        }
    }
    if let Some(rows) = document["tensor"].as_array() {
        bytes += rows.len() * std::mem::size_of::<Vec<f32>>();
        bytes += rows
            .iter()
            .map(|row| row.as_array().unwrap().len() * 4)
            .sum::<usize>();
    }
    bytes
}

#[test]
fn estimate_counts_every_owned_allocation() {
    for (words, width) in [(0, 0), (1, 0), (100, 0), (100, 96), (1000, 8)] {
        let value = snapshot(words, width);
        let restored = doc(&value);
        // A clone allocates exactly the length of every string and list, so it matches exactly.
        assert_eq!(
            restored.clone().estimated_heap_bytes(),
            exact(&value),
            "{words} words, width {width}"
        );
        // Deserialized lists may keep spare capacity, which is counted too.
        assert!(restored.estimated_heap_bytes() >= exact(&value));
    }
    // Each kind of allocation contributes: removing it lowers the estimate by its size.
    let full = snapshot(100, 0);
    let full_bytes = doc(&full).clone().estimated_heap_bytes();
    for name in STRINGS.iter().skip(1) {
        let mut without = full.clone();
        for token in without["document"]["tokens"].as_array_mut().unwrap() {
            token.as_object_mut().unwrap().remove(*name);
        }
        let mut removed = 100 * full["document"]["tokens"][0][*name].as_str().unwrap().len();
        // An entity's ID must match its first token's, so token IDs go with entity IDs.
        if *name == "entity_id" {
            for entity in without["document"]["entities"].as_array_mut().unwrap() {
                let id = entity.as_object_mut().unwrap().remove("id").unwrap();
                removed += id.as_str().unwrap().len();
            }
        }
        assert_eq!(
            doc(&without).clone().estimated_heap_bytes(),
            full_bytes - removed,
            "{name}"
        );
    }
    for name in ["entities", "noun_chunks"] {
        let mut without = full.clone();
        without["document"][name] = Value::Null;
        let first = &full["document"][name][0];
        let strings =
            first["label"].as_str().unwrap().len() + first["id"].as_str().map_or(0, str::len);
        let removed = 100 * (std::mem::size_of::<spars::Span>() + strings);
        assert_eq!(
            doc(&without).clone().estimated_heap_bytes(),
            full_bytes - removed,
            "{name}"
        );
    }
}
