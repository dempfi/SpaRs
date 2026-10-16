use serde::{Deserialize, Serialize};
use spars::{Doc, PhraseMatcher, Token};

#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    id: String,
    words: Vec<String>,
    spaces: Vec<bool>,
    operations: Vec<Operation>,
    states: Vec<State>,
}
#[derive(Deserialize)]
struct Operation {
    action: String,
    rule: String,
    patterns: Vec<Vec<String>>,
}
#[derive(Deserialize)]
struct State {
    error: Option<String>,
    rules: Vec<String>,
    matches: Vec<spars::PhraseMatch>,
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
    annotated_doc(words, spaces, &[])
}
fn annotated_doc(words: &[String], spaces: &[bool], starts: &[bool]) -> Doc {
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
        token.sentence_start = starts.get(tokens.len()).copied();
        tokens.push(token);
        if *space {
            text.push(' ');
        }
    }
    Doc::from_json(
        &serde_json::to_string(&Snapshot {
            format_version: 1,
            document: Storage { text, tokens },
        })
        .unwrap(),
    )
    .unwrap()
}
fn check(raw: &str, cases: usize, states: usize, outputs: usize) {
    let fixture: Fixture = serde_json::from_str(raw).unwrap();
    assert_eq!(fixture.cases.len(), cases);
    assert_eq!(
        fixture.cases.iter().map(|c| c.states.len()).sum::<usize>(),
        states
    );
    assert_eq!(
        fixture
            .cases
            .iter()
            .flat_map(|c| &c.states)
            .map(|s| s.matches.len())
            .sum::<usize>(),
        outputs
    );
    for case in fixture.cases {
        let input = doc(&case.words, &case.spaces);
        let input = Doc::from_json(&input.to_json().unwrap()).unwrap();
        let mut matcher = PhraseMatcher::new();
        assert_eq!(case.operations.len(), case.states.len());
        let mut labels = Vec::new();
        for (index, (op, expected)) in case.operations.iter().zip(&case.states).enumerate() {
            if !labels.contains(&op.rule) {
                labels.push(op.rule.clone());
            }
            let result = if op.action == "add" {
                let patterns: Vec<_> = op
                    .patterns
                    .iter()
                    .map(|words| doc(words, &vec![false; words.len()]))
                    .collect();
                matcher.add(op.rule.clone(), &patterns.iter().collect::<Vec<_>>())
            } else {
                matcher.remove(&op.rule)
            };
            assert_eq!(
                result.is_err(),
                expected.error.is_some(),
                "{} operation {index}",
                case.id
            );
            assert_eq!(matcher.len(), expected.rules.len());
            assert_eq!(matcher.is_empty(), expected.rules.is_empty());
            for label in &labels {
                assert_eq!(matcher.contains(label), expected.rules.contains(label));
                assert_eq!(matcher.get(label).is_some(), expected.rules.contains(label));
            }
            assert_eq!(
                matcher.find_matches(&input).unwrap(),
                expected.matches,
                "{} operation {index}",
                case.id
            );
            std::thread::scope(|scope| {
                let other = scope.spawn(|| matcher.find_matches(&input).unwrap());
                assert_eq!(matcher.find_matches(&input).unwrap(), expected.matches);
                assert_eq!(other.join().unwrap(), expected.matches);
            });
        }
    }
    eprintln!("PhraseMatcher: {cases} cases / {states} lifecycle states / {outputs} exact ordered matches");
}
#[test]
fn official_development_19_cases_529_states_14835_matches() {
    check(
        include_str!("../../../fixtures/phrase-match-v1.expected.json"),
        19,
        529,
        14835,
    );
}
#[test]
fn official_reserved_24_cases_720_states_17084_matches() {
    check(
        include_str!("../../../fixtures/phrase-match-holdout-v1.expected.json"),
        24,
        720,
        17084,
    );
}

#[test]
fn official_sentence_boundary_one_case_one_match() {
    #[derive(Deserialize)]
    struct Edges {
        sentence_starts: Vec<bool>,
        sentence: Case,
    }
    let edges: Edges = serde_json::from_str(include_str!(
        "../../../fixtures/phrase-match-edges-v1.expected.json"
    ))
    .unwrap();
    let case = edges.sentence;
    let input = annotated_doc(&case.words, &case.spaces, &edges.sentence_starts);
    let mut matcher = PhraseMatcher::new();
    let pattern = doc(&case.operations[0].patterns[0], &case.spaces);
    matcher.add("cross", &[&pattern]).unwrap();
    assert_eq!(
        matcher.find_matches(&input).unwrap(),
        case.states[0].matches
    );
    assert_eq!(case.states[0].matches.len(), 1);
}
