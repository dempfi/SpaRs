use super::*;
use serde_json::Value;
fn compare_floats(actual: &Value, expected: &Value, abs: f64, rel: f64) -> f64 {
    if let (Some(a), Some(b)) = (actual.as_array(), expected.as_array()) {
        assert_eq!(a.len(), b.len());
        a.iter()
            .zip(b)
            .map(|(a, b)| compare_floats(a, b, abs, rel))
            .fold(0., f64::max)
    } else {
        let a = actual.as_f64().unwrap();
        let b = expected.as_f64().unwrap();
        let delta = (a - b).abs();
        assert!(
            a.is_finite() && b.is_finite() && delta <= abs + rel * b.abs(),
            "{a} != {b}, delta {delta}"
        );
        delta
    }
}
#[test]
fn hash_keys_match_thinc() {
    let cases: Value =
        serde_json::from_str(include_str!("../../../fixtures/hash.expected.json")).unwrap();
    for c in cases.as_array().unwrap() {
        assert_eq!(
            serde_json::json!(hash::keys(
                c["id"].as_u64().unwrap(),
                c["seed"].as_u64().unwrap()
            )),
            c["keys"]
        );
    }
}
#[test]
#[ignore = "requires official export; mandatory CI stage"]
fn official_stage_parity() {
    let m = Model::load("../../assets/en_core_web_md-3.8.0").unwrap();
    let cases: Value = serde_json::from_str(
        &std::fs::read_to_string("../../fixtures/stages.expected.json").unwrap(),
    )
    .unwrap();
    let mut max_activation = 0f64;
    let mut max_score = 0f64;
    let mut actions = 0;
    for c in cases.as_array().unwrap() {
        let text = c["text"].as_str().unwrap();
        let mut d = m.tokenize(text).unwrap();
        let mut stages = vec![];
        let x = m.encode_traced(&d, &m.config.tok2vec, Some(&mut stages));
        max_activation = max_activation.max(compare_floats(
            &serde_json::json!(stages),
            &c["tok2vec_stages"],
            2e-5,
            2e-5,
        ));
        let mut trace = vec![];
        m.parse_traced(&mut d, &x, Some(&mut trace)).unwrap();
        for name in ["parser", "ner"] {
            if name == "ner" {
                d = m.process(text).unwrap();
                for t in &mut d.tokens {
                    t.entity_iob = None;
                    t.entity_type = None
                }
                trace.clear();
                stages.clear();
                m.encode_traced(&d, &m.config.ner.tok2vec, Some(&mut stages));
                max_activation = max_activation.max(compare_floats(
                    &serde_json::json!(stages),
                    &c["ner_stages"],
                    2e-5,
                    2e-5,
                ));
                m.ner_traced(&mut d, Some(&mut trace)).unwrap();
            }
            let expected = c[name].as_array().unwrap();
            assert_eq!(trace.len(), expected.len(), "{name} {text}");
            for (step, (a, b)) in trace.iter().zip(expected).enumerate() {
                let a = serde_json::to_value(a).unwrap();
                for field in ["ids", "valid", "action"] {
                    assert_eq!(a[field], b[field], "{name} step {step} {field} text={text}");
                }
                max_score = max_score.max(compare_floats(&a["scores"], &b["scores"], 2e-4, 2e-5));
                actions += 1;
            }
        }
    }
    std::fs::create_dir_all("../../target/reports").unwrap();
    std::fs::write("../../target/reports/stages.json",serde_json::to_string_pretty(&serde_json::json!({"cases":cases.as_array().unwrap().len(),"actions":actions,"max_activation_absolute_error":max_activation,"max_score_absolute_error":max_score})).unwrap()).unwrap();
}
#[test]
#[ignore = "requires official export; mandatory CI stage"]
fn tokenizer_and_features() {
    let m = Model::load("../../assets/en_core_web_md-3.8.0").unwrap();
    let cases: Value = serde_json::from_str(
        &std::fs::read_to_string("../../fixtures/tokenizer.expected.json").unwrap(),
    )
    .unwrap();
    let mut failures = vec![];
    let mut count = 0;
    for c in cases.as_array().unwrap() {
        let d = m.tokenize(c["text"].as_str().unwrap()).unwrap();
        let actual:Vec<Value>=d.tokens.iter().enumerate().map(|(i,t)|serde_json::json!({"text":d.token_text(TokenIndex(i)).unwrap(),"idx":t.idx.0,"whitespace":t.whitespace,"norm":t.norm})).collect();
        let features: Vec<Vec<u64>> = (0..d.tokens.len())
            .map(|i| m.features(&d, i, &m.config.tok2vec.attrs))
            .collect();
        count += d.tokens.len();
        if serde_json::json!(actual) != c["tokens"] || serde_json::json!(features) != c["features"]
        {
            failures.push(serde_json::json!({"text":c["text"],"actual":actual,"expected":c["tokens"],"features":features,"expected_features":c["features"]}));
        }
    }
    std::fs::create_dir_all("../../target/reports").unwrap();
    std::fs::write("../../target/reports/tokenizer-mismatches.json",serde_json::to_string_pretty(&serde_json::json!({"cases":cases.as_array().unwrap().len(),"tokens":count,"differences":failures})).unwrap()).unwrap();
    assert!(
        failures.is_empty(),
        "{} tokenizer/feature mismatches",
        failures.len()
    );
}
#[test]
#[ignore = "requires official export; mandatory CI"]
fn lexical_and_vectors() {
    let m = Model::load("../../assets/en_core_web_md-3.8.0").unwrap();
    let cases: Value = serde_json::from_str(
        &std::fs::read_to_string("../../fixtures/lexical.expected.json").unwrap(),
    )
    .unwrap();
    let mut failures = vec![];
    for c in cases.as_array().unwrap() {
        let actual = serde_json::to_value(m.lexeme(c["text"].as_str().unwrap()).unwrap()).unwrap();
        if actual != c["expected"] {
            failures.push(
                serde_json::json!({"text":c["text"],"actual":actual,"expected":c["expected"]}),
            );
        }
    }
    std::fs::create_dir_all("../../target/reports").unwrap();
    std::fs::write(
        "../../target/reports/lexical-mismatches.json",
        serde_json::to_string_pretty(
            &serde_json::json!({"cases":cases.as_array().unwrap().len(),"differences":failures}),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(cases.as_array().unwrap().len(), 4374);
    assert!(failures.is_empty(), "{} lexical mismatches", failures.len());
    let vectors: Value = serde_json::from_str(
        &std::fs::read_to_string("../../fixtures/vectors.expected.json").unwrap(),
    )
    .unwrap();
    for v in vectors["words"].as_array().unwrap() {
        let a = m.vector(v["text"].as_str().unwrap());
        assert_eq!(a.is_some(), v["has_vector"].as_bool().unwrap());
        if let Some(a) = a {
            for (a, b) in a.iter().zip(v["vector"].as_array().unwrap()) {
                assert_eq!(*a, b.as_f64().unwrap() as f32)
            }
        }
    }
    for c in vectors["similarities"].as_array().unwrap() {
        let a = m.tokenize(c["a"].as_str().unwrap()).unwrap();
        let b = m.tokenize(c["b"].as_str().unwrap()).unwrap();
        assert!((m.similarity(&a, &b) as f64 - c["expected"].as_f64().unwrap()).abs() < 2e-6);
    }
}
#[test]
fn document_snapshots_validate_offsets() {
    assert!(Doc::from_json(r#"{"format_version":1,"document":{"text":"bad","tokens":[],"entities":null,"sentences":null,"noun_chunks":null}}"#).is_err());
    assert!(matches!(
        Doc::from_json(
            r#"{"format_version":4,"document":{"text":"","tokens":[],"entities":null,"sentences":null,"noun_chunks":null}}"#
        ),
        Err(Error::Unsupported(_))
    ));
}
#[test]
#[ignore = "requires official export; mandatory CI"]
fn regex_span_semantics() {
    let m = Model::load("../../assets/en_core_web_md-3.8.0").unwrap();
    let cases: Value = serde_json::from_str(
        &std::fs::read_to_string("../../fixtures/regex.expected.json").unwrap(),
    )
    .unwrap();
    let mut failures = vec![];
    for c in cases.as_array().unwrap() {
        for kind in ["prefix", "suffix", "infix", "url"] {
            let actual = serde_json::json!(m
                .tokenizer
                .regex_spans(c["text"].as_str().unwrap(), kind)
                .unwrap());
            if actual != c[kind] {
                failures.push(serde_json::json!({"text":c["text"],"regex":kind,"actual":actual,"expected":c[kind]}));
            }
        }
    }
    std::fs::create_dir_all("../../target/reports").unwrap();
    std::fs::write("../../target/reports/regex-mismatches.json",serde_json::to_string_pretty(&serde_json::json!({"cases":cases.as_array().unwrap().len(),"comparisons":cases.as_array().unwrap().len()*4,"differences":failures})).unwrap()).unwrap();
    assert!(
        failures.is_empty(),
        "{} regex span mismatches",
        failures.len()
    );
}
#[test]
#[ignore = "requires official export; mandatory CI"]
fn model_and_matcher_lowercase_agree() {
    // Matcher LOWER uses the bundled Unicode table; LIKE_NUM and lemmas use the
    // model's exported table. Both must reproduce the same Python str.lower().
    let m = Model::load("../../assets/en_core_web_md-3.8.0").unwrap();
    let mut samples: Vec<String> = m.config.lexical.lower.keys().cloned().collect();
    samples.extend(
        [
            "ΟΣ",
            "Σ",
            "AΣ\u{301}",
            "AΣ\u{301}A",
            "İ",
            "TEN",
            "Twelfth",
            "ẞ",
            "\u{212a}",
        ]
        .map(String::from),
    );
    assert!(samples.len() > 1000);
    for sample in samples {
        assert_eq!(
            m.lower(&sample),
            crate::unicode_lower::lower(&sample),
            "{sample:?}"
        );
    }
}
