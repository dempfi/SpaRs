use serde::{Deserialize, Serialize};
use spars::{Doc, Error, Lexicon, PhraseAttribute, PhraseMatch, PhraseMatcher, Token};

#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    id: String,
    attribute: PhraseAttribute,
    words: Vec<String>,
    spaces: Vec<bool>,
    operations: Vec<Operation>,
    states: Vec<State>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Operation {
    action: Action,
    rule: String,
    patterns: Vec<Vec<String>>,
}
#[derive(Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Action {
    Add,
}
#[derive(Deserialize)]
struct State {
    error: Option<String>,
    rules: Vec<String>,
    patterns: Vec<Vec<Vec<u64>>>,
    matches: Vec<PhraseMatch>,
}
#[derive(Serialize)]
struct Snapshot {
    format_version: u32,
    document: Storage,
}
#[derive(Serialize)]
struct Storage {
    text: String,
    tokens: Vec<Token>,
}

fn doc(words: &[String], spaces: &[bool]) -> Doc {
    let mut text = String::new();
    let mut tokens = Vec::new();
    for (word, space) in words.iter().zip(spaces) {
        let start = text.len();
        let idx = text.chars().count();
        text.push_str(word);
        let mut token = Token::new(
            spars::ByteOffset(start),
            spars::ByteOffset(text.len()),
            spars::CodePointOffset(idx),
            word.clone(),
        );
        token.whitespace = *space;
        tokens.push(token);
        if *space {
            text.push(' ');
        }
    }
    let snapshot = Snapshot {
        format_version: 1,
        document: Storage { text, tokens },
    };
    Doc::from_json(&serde_json::to_string(&snapshot).unwrap()).unwrap()
}

// spaCy stores a flag as the integer 0 or 1 and a length as itself; SpaRs as text.
fn stored(attribute: PhraseAttribute, key: u64) -> String {
    match (attribute, key) {
        (PhraseAttribute::Length, length) => length.to_string(),
        (_, 0) => "false".into(),
        (_, 1) => "true".into(),
        (_, other) => panic!("flag key {other}"),
    }
}

// Returns (cases, states, matches) compared.
fn check(lexicon: Option<&Lexicon>, flags: bool) -> (usize, usize, usize) {
    let fixture: Fixture = serde_json::from_str(include_str!(
        "../../../fixtures/phrase-match-lexical-v1.expected.json"
    ))
    .unwrap();
    let (mut cases, mut states, mut outputs) = (0, 0, 0);
    for case in &fixture.cases {
        if (case.attribute == PhraseAttribute::Length) == flags {
            continue;
        }
        cases += 1;
        let input = doc(&case.words, &case.spaces);
        let mut matcher = match lexicon {
            Some(lexicon) => PhraseMatcher::with_lexicon(case.attribute, lexicon.clone()),
            None => PhraseMatcher::with_attribute(case.attribute),
        };
        assert_eq!(case.operations.len(), case.states.len());
        for (index, (op, expected)) in case.operations.iter().zip(&case.states).enumerate() {
            let context = format!("{} operation {index}", case.id);
            assert!(
                op.action == Action::Add && expected.error.is_none(),
                "{context}"
            );
            let patterns: Vec<Doc> = op
                .patterns
                .iter()
                .map(|words| doc(words, &vec![false; words.len()]))
                .collect();
            matcher
                .add(op.rule.as_str(), &patterns.iter().collect::<Vec<_>>())
                .unwrap();
            assert_eq!(matcher.len(), expected.rules.len(), "{context}");
            for (rule, keys) in expected.rules.iter().zip(&expected.patterns) {
                let mut actual: Vec<Vec<String>> = matcher
                    .get(rule)
                    .unwrap()
                    .iter()
                    .map(|p| p.tokens().to_vec())
                    .collect();
                let mut keys: Vec<Vec<String>> = keys
                    .iter()
                    .map(|p| p.iter().map(|&k| stored(case.attribute, k)).collect())
                    .collect();
                actual.sort();
                keys.sort();
                assert_eq!(actual, keys, "{context} rule {rule:?}");
            }
            std::thread::scope(|scope| {
                let other = scope.spawn(|| matcher.find_matches(&input).unwrap());
                assert_eq!(
                    matcher.find_matches(&input).unwrap(),
                    expected.matches,
                    "{context}"
                );
                assert_eq!(other.join().unwrap(), expected.matches, "{context}");
            });
            states += 1;
            outputs += expected.matches.len();
        }
    }
    (cases, states, outputs)
}

#[test]
fn official_length_phrases_need_no_lexicon() {
    assert_eq!(check(None, false), (2, 10, 196));
}

#[test]
#[ignore = "requires official export; mandatory acceptance"]
fn official_lexical_flag_phrases_use_the_model_lexicon() {
    let model = spars::Model::load("../../assets/en_core_web_md-3.8.0").unwrap();
    let lexicon = model.lexicon();
    let counts = check(Some(&lexicon), true);
    assert_eq!(counts, (34, 170, 11760));
    eprintln!("PhraseMatcher lexical flags: {counts:?} cases / states / ordered matches");
    // LENGTH needs no lexicon, but a lexicon does not change it.
    assert_eq!(check(Some(&lexicon), false), (2, 10, 196));
}

#[test]
fn flag_attributes_require_a_lexicon_before_registration() {
    let mut matcher = PhraseMatcher::with_attribute(PhraseAttribute::IsAlpha);
    let pattern = doc(&["a".into()], &[false]);
    for documents in [vec![&pattern], vec![]] {
        assert!(matches!(
            matcher.add("r", &documents),
            Err(Error::Pattern(_))
        ));
    }
    assert!(matcher.is_empty());
    assert!(matcher.find_matches(&pattern).unwrap().is_empty());
}

// After spaCy 3.8.14 removes these rules, later matching reads freed memory, because
// flag values and a one-character length are its reserved hash keys 0 and 1.
// SpaRs removes them normally, keeping rules that share the phrase prefix.
#[test]
fn rules_with_reserved_spacy_keys_are_removed_normally() {
    let words: Vec<String> = ["a", "bb", "a"].map(String::from).into();
    let input = doc(&words, &[true, true, false]);
    let one = doc(&words[..1], &[false]);
    let mut length = PhraseMatcher::with_attribute(PhraseAttribute::Length);
    length.add("one", &[&one]).unwrap();
    length
        .add("shared", &[&doc(&words[..2], &[false, false])])
        .unwrap();
    assert_eq!(length.find_matches(&input).unwrap().len(), 3);
    length.remove("one").unwrap();
    let remaining: Vec<_> = length
        .find_matches(&input)
        .unwrap()
        .into_iter()
        .map(|m| (m.rule.0, m.start.0, m.end.0))
        .collect();
    assert_eq!(remaining, [("shared".to_string(), 0, 2)]);
    length.remove("shared").unwrap();
    assert!(length.is_empty());
    assert!(length.find_matches(&input).unwrap().is_empty());
    length.add("one", &[&one]).unwrap();
    assert_eq!(length.find_matches(&input).unwrap().len(), 2);
}

#[test]
#[ignore = "requires official export; mandatory acceptance"]
fn flag_rules_are_removed_normally() {
    let model = spars::Model::load("../../assets/en_core_web_md-3.8.0").unwrap();
    let words: Vec<String> = ["Hello", "123", "x"].map(String::from).into();
    let input = doc(&words, &[true, true, false]);
    let mut matcher = PhraseMatcher::with_lexicon(PhraseAttribute::IsAlpha, model.lexicon());
    matcher
        .add("alpha", &[&doc(&words[..1], &[false])])
        .unwrap();
    matcher
        .add("digits", &[&doc(&words[1..2], &[false])])
        .unwrap();
    assert_eq!(matcher.find_matches(&input).unwrap().len(), 3);
    matcher.remove("alpha").unwrap();
    matcher.remove("digits").unwrap();
    assert!(matcher.is_empty());
    assert!(matcher.find_matches(&input).unwrap().is_empty());
}
