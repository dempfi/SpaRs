//! EntityRuler workloads: the same patterns timed through the two matchers alone, through
//! `EntityRuler::find_matches`, and through `annotated`, so the ruler's own cost is visible.
//!
//! Usage: `entity_ruler_benchmark TOKENS PATTERNS REPEATS EXISTING`. Documents use a vocabulary of
//! 200 words; patterns are two-word phrases and one-word token patterns over it, half with IDs. With
//! `EXISTING` set to 1, every third token starts a one-token entity and the ruler overwrites entities.
use serde::Serialize;
use spars::{
    ByteOffset, CodePointOffset, Doc, EntityPattern, EntityPatternKind, EntityRuler,
    EntityRulerOptions, EntityUpdate, PhraseMatcher, Predicate, Repetition, Span, Token,
    TokenAttribute, TokenConstraint, TokenIndex, TokenMatcher, TokenPattern, TokenPatternItem,
};
use std::{hint::black_box, time::Instant};

#[derive(Serialize)]
struct Snapshot<'a> {
    format_version: u32,
    document: Storage<'a>,
}
#[derive(Serialize)]
struct Storage<'a> {
    text: &'a str,
    tokens: &'a [Token],
}

fn doc(words: &[String]) -> Doc {
    let text = words.join(" ");
    let mut tokens = Vec::with_capacity(words.len());
    let (mut start, mut cp) = (0, 0);
    for (i, word) in words.iter().enumerate() {
        let mut token = Token::new(
            ByteOffset(start),
            ByteOffset(start + word.len()),
            CodePointOffset(cp),
            word.clone(),
        );
        token.whitespace = i + 1 < words.len();
        tokens.push(token);
        start += word.len() + 1;
        cp += word.chars().count() + 1;
    }
    let snapshot = Snapshot {
        format_version: 1,
        document: Storage {
            text: &text,
            tokens: &tokens,
        },
    };
    Doc::from_json(&serde_json::to_string(&snapshot).unwrap()).unwrap()
}

fn word(i: usize) -> String {
    format!("w{}", i % 200)
}

fn median(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples[samples.len() / 2]
}

fn time(repeats: usize, mut run: impl FnMut()) -> f64 {
    run();
    median(
        (0..repeats)
            .map(|_| {
                let start = Instant::now();
                run();
                start.elapsed().as_secs_f64() * 1e3
            })
            .collect(),
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<usize> = std::env::args()
        .skip(1)
        .map(|a| a.parse())
        .collect::<Result<_, _>>()?;
    let [length, count, repeats, existing] = args[..] else {
        return Err("usage: entity_ruler_benchmark TOKENS PATTERNS REPEATS EXISTING".into());
    };
    // A deterministic, non-repeating-looking word sequence.
    let words: Vec<String> = (0..length).map(|i| word(i * 7919 % 9973)).collect();
    let mut input = doc(&words);
    if existing == 1 {
        let entities = (0..length)
            .step_by(3)
            .map(|i| Span::new(TokenIndex(i), TokenIndex(i + 1), "E"))
            .collect();
        input.set_entities(&EntityUpdate {
            entities,
            ..EntityUpdate::default()
        })?;
    }
    let phrases: Vec<Doc> = (0..count / 2)
        .map(|i| doc(&[word(i), word(i * 31 + 1)]))
        .collect();
    let tokens: Vec<TokenPattern> = (0..count - count / 2)
        .map(|i| TokenPattern {
            tokens: vec![TokenPatternItem {
                constraints: vec![TokenConstraint {
                    attribute: TokenAttribute::Text,
                    predicate: Predicate::Equals {
                        value: word(i * 13),
                    },
                }],
                repetition: Repetition::Once,
            }],
        })
        .collect();
    let setup = Instant::now();
    let mut patterns = Vec::with_capacity(count);
    for (i, phrase) in phrases.iter().enumerate() {
        patterns.push(EntityPattern {
            label: format!("P{}", i % 8),
            id: (i % 2 == 0).then(|| format!("p{i}")),
            pattern: EntityPatternKind::Phrase(phrase),
        });
    }
    for (i, pattern) in tokens.iter().enumerate() {
        patterns.push(EntityPattern {
            label: format!("T{}", i % 8),
            id: (i % 2 == 0).then(|| format!("t{i}")),
            pattern: EntityPatternKind::Tokens(pattern.clone()),
        });
    }
    let mut options = EntityRulerOptions::default();
    options.overwrite_entities = existing == 1;
    let mut ruler = EntityRuler::new(options)?;
    ruler.add(&patterns)?;
    let setup_ms = setup.elapsed().as_secs_f64() * 1e3;
    // The same rules registered directly, one rule per pattern.
    let mut phrase_matcher = PhraseMatcher::new();
    for (i, phrase) in phrases.iter().enumerate() {
        phrase_matcher.add(format!("p{i}"), &[phrase])?;
    }
    let mut token_matcher = TokenMatcher::new();
    for (i, pattern) in tokens.iter().enumerate() {
        token_matcher.add(format!("t{i}"), vec![pattern.clone()])?;
    }
    let matchers_ms = time(repeats, || {
        black_box(phrase_matcher.find_matches(&input).unwrap());
        black_box(token_matcher.find_matches(&input).unwrap());
    });
    let find_ms = time(repeats, || {
        black_box(ruler.find_matches(&input).unwrap());
    });
    let copy_ms = time(repeats, || {
        black_box(input.clone());
    });
    let annotated_ms = time(repeats, || {
        black_box(ruler.annotated(&input).unwrap());
    });
    let annotated = ruler.annotated(&input)?;
    println!(
        "{}",
        serde_json::json!({
            "tokens": length, "patterns": count, "repeats": repeats, "existing": existing,
            "matches": ruler.find_matches(&input)?.len(),
            "entities": annotated.entities().map_or(0, <[_]>::len),
            "setup_ms": setup_ms, "matchers_ms": matchers_ms, "find_matches_ms": find_ms,
            "copy_ms": copy_ms, "annotated_ms": annotated_ms,
        })
    );
    Ok(())
}
