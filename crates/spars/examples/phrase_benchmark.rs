//! Reproducible matcher-only workloads; setup and matching are timed separately.
use serde::Serialize;
use spars::{
    ByteOffset, CodePointOffset, Doc, PhraseMatcher, Predicate, Repetition, Token, TokenAttribute,
    TokenConstraint, TokenMatcher, TokenPattern, TokenPatternItem,
};
use std::{hint::black_box, time::Instant};
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
fn doc(words: &[String]) -> Doc {
    let mut text = String::new();
    let mut tokens = Vec::new();
    let mut cp = 0;
    for word in words {
        let start = text.len();
        let idx = cp;
        cp += word.chars().count() + 1;
        text.push_str(word);
        let mut token = Token::new(
            ByteOffset(start),
            ByteOffset(text.len()),
            CodePointOffset(idx),
            word.clone(),
        );
        token.whitespace = true;
        tokens.push(token);
        text.push(' ');
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
#[derive(Serialize)]
struct Measurement {
    engine: String,
    scenario: String,
    patterns: usize,
    pattern_tokens: usize,
    input_tokens: usize,
    outputs: usize,
    compile_ms: f64,
    match_ms: Vec<f64>,
    remove_ms: f64,
    cold_symbols_ms: f64,
    cold_match_ms: f64,
    prune_ms: f64,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 5 {
        return Err("expected ENGINE SCENARIO PATTERNS INPUT_TOKENS PHRASE_LENGTH".into());
    }
    let n: usize = args[2].parse()?;
    let count: usize = args[3].parse()?;
    let length: usize = args[4].parse()?;
    if n == 0 || length == 0 {
        return Err("sizes must be positive".into());
    }
    let patterns: Vec<Vec<String>> = (0..n)
        .map(|i| {
            (0..length)
                .map(|j| match args[1].as_str() {
                    "dense" | "pruned" => "é".into(),
                    "shared" if j + 1 < length => "prefix".into(),
                    _ => format!("term-{i}-{j}"),
                })
                .collect()
        })
        .collect();
    let words: Vec<_> = (0..count)
        .map(|i| match args[1].as_str() {
            "miss" => format!("unknown-{i}"),
            "shared" if i % length + 1 < length => "prefix".into(),
            "shared" => format!("term-{}-{}", (i / length) % n, length - 1),
            _ => patterns[(i / length) % n][i % length].clone(),
        })
        .collect();
    // `lower-upper` capitalizes the input so every token needs a lowercase copy.
    let words: Vec<_> = if args[0] == "lower-upper" {
        words.iter().map(|word| word.to_uppercase()).collect()
    } else {
        words
    };
    let docs: Vec<_> = patterns.iter().map(|p| doc(p)).collect();
    let input = doc(&words);
    // Resolve immutable symbol data before timing either matcher.
    let symbol_start = Instant::now();
    PhraseMatcher::new().add("warmup", &[])?;
    let cold_symbols_ms = symbol_start.elapsed().as_secs_f64() * 1000.;
    let mut phrase = PhraseMatcher::new();
    let mut token = TokenMatcher::new();
    let start = Instant::now();
    for (i, p) in patterns.iter().enumerate() {
        let label = format!("rule-{i}");
        if args[0] == "phrase" {
            phrase.add(label, &[&docs[i]])?;
        } else if matches!(args[0].as_str(), "token" | "lower" | "lower-upper") {
            let attribute = if args[0] == "token" {
                TokenAttribute::Text
            } else {
                TokenAttribute::Lower
            };
            token.add(
                label,
                vec![TokenPattern {
                    tokens: p
                        .iter()
                        .map(|s| TokenPatternItem {
                            constraints: vec![TokenConstraint {
                                attribute,
                                predicate: Predicate::Equals { value: s.clone() },
                            }],
                            repetition: Repetition::Once,
                        })
                        .collect(),
                }],
            )?;
        } else {
            return Err("engine must be phrase, token, lower or lower-upper".into());
        }
    }
    let compile_ms = start.elapsed().as_secs_f64() * 1000.;
    let prune_start = Instant::now();
    if args[1] == "pruned" {
        for i in 1..n {
            let label = format!("rule-{i}");
            if args[0] == "phrase" {
                phrase.remove(&label)?;
            } else {
                token.remove(&label)?;
            }
        }
    }
    let prune_ms = prune_start.elapsed().as_secs_f64() * 1000.;
    let expected = if args[1] == "pruned" {
        count.saturating_sub(length - 1)
    } else if args[1] == "dense" {
        count.saturating_sub(length - 1) * n
    } else if args[1] == "miss" {
        0
    } else {
        count / length
    };
    let mut times = Vec::new();
    let mut outputs = 0;
    let mut cold_match_ms = 0.;
    for iteration in 0..7 {
        let start = Instant::now();
        outputs = if args[0] == "phrase" {
            black_box(phrase.find_matches(black_box(&input))?).len()
        } else {
            black_box(token.find_matches(black_box(&input))?).len()
        };
        if iteration == 0 {
            cold_match_ms = start.elapsed().as_secs_f64() * 1000.;
        }
        if iteration >= 2 {
            times.push(start.elapsed().as_secs_f64() * 1000.);
        }
        assert_eq!(outputs, expected);
    }
    let start = Instant::now();
    for i in 0..if args[1] == "pruned" { 1 } else { n } {
        let label = format!("rule-{i}");
        if args[0] == "phrase" {
            phrase.remove(&label)?;
        } else {
            token.remove(&label)?;
        }
    }
    let report = Measurement {
        engine: args[0].clone(),
        scenario: args[1].clone(),
        patterns: n,
        pattern_tokens: n * length,
        input_tokens: count,
        outputs,
        compile_ms,
        match_ms: times,
        remove_ms: start.elapsed().as_secs_f64() * 1000.,
        cold_symbols_ms,
        cold_match_ms,
        prune_ms,
    };
    println!("{}", serde_json::to_string(&report)?);
    Ok(())
}
