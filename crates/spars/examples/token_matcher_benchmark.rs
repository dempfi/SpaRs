//! Token Matcher workloads with many rules: registration, matching, and removal are timed
//! separately, and a hash of the ordered matches lets two builds be compared for identical output.
//!
//! Usage: `token_matcher_benchmark TOKENS PATTERNS MIX REPEATS [VOCABULARY]`. Patterns and documents
//! use words from a vocabulary of `VOCABULARY` words (default 200); documents use at most the first
//! 9,973 of them. Each pattern is registered as its own rule. `MIX` is `indexed` (every pattern
//! starts with an exact `Text` or `Lower` value), `mixed` (half of the patterns start with an exact
//! value) or `unindexed` (no pattern starts with an exact value). Matching reports the median,
//! minimum and maximum of `REPEATS` calls after one warmup call; removal deletes every tenth rule.
use serde::Serialize;
use spars::{
    ByteOffset, CodePointOffset, Doc, Predicate, Repetition, Token, TokenAttribute,
    TokenConstraint, TokenMatch, TokenMatcher, TokenPattern, TokenPatternItem,
};
use std::{hint::black_box, sync::OnceLock, time::Instant};

static VOCABULARY: OnceLock<usize> = OnceLock::new();

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

#[derive(Clone, Copy, PartialEq)]
enum Mix {
    Indexed,
    Mixed,
    Unindexed,
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
    format!("w{}", i % VOCABULARY.get().copied().unwrap_or(200))
}

fn item(
    attribute: TokenAttribute,
    predicate: Predicate,
    repetition: Repetition,
) -> TokenPatternItem {
    TokenPatternItem {
        constraints: vec![TokenConstraint {
            attribute,
            predicate,
        }],
        repetition,
    }
}

fn equals(attribute: TokenAttribute, value: String) -> TokenPatternItem {
    item(attribute, Predicate::Equals { value }, Repetition::Once)
}

// Patterns whose first item has an exact `Text` or `Lower` value.
fn indexed(i: usize) -> TokenPattern {
    let tokens = match i % 4 {
        0 => vec![equals(TokenAttribute::Text, word(i * 13))],
        1 => vec![equals(TokenAttribute::Lower, word(i * 17))],
        2 => vec![
            equals(TokenAttribute::Text, word(i * 13)),
            equals(TokenAttribute::Text, word(i * 31 + 1)),
        ],
        _ => vec![item(
            TokenAttribute::Text,
            Predicate::In {
                values: vec![word(i * 7), word(i * 7 + 3)],
            },
            Repetition::OneOrMore,
        )],
    };
    TokenPattern { tokens }
}

// Patterns whose first item can match many different tokens or no token at all.
fn unindexed(i: usize) -> TokenPattern {
    let tokens = match i % 4 {
        0 => vec![
            item(
                TokenAttribute::Text,
                Predicate::NotIn {
                    values: vec![word(i * 11)],
                },
                Repetition::Once,
            ),
            equals(TokenAttribute::Text, word(i * 29 + 5)),
        ],
        1 => vec![
            item(
                TokenAttribute::Text,
                Predicate::Equals { value: word(i * 3) },
                Repetition::Optional,
            ),
            equals(TokenAttribute::Text, word(i * 23 + 2)),
        ],
        2 => vec![
            item(
                TokenAttribute::Text,
                Predicate::Equals { value: word(i * 5) },
                Repetition::Negated,
            ),
            equals(TokenAttribute::Lower, word(i * 19 + 4)),
        ],
        _ => vec![
            item(
                TokenAttribute::Text,
                Predicate::Equals { value: word(i) },
                Repetition::ZeroOrMore,
            ),
            equals(TokenAttribute::Text, word(i * 37 + 6)),
        ],
    };
    TokenPattern { tokens }
}

// The middle sample, or the mean of the two middle samples for an even count.
fn median(samples: &mut [f64]) -> f64 {
    samples.sort_by(f64::total_cmp);
    let middle = samples.len() / 2;
    if samples.len() % 2 == 0 {
        (samples[middle - 1] + samples[middle]) / 2.0
    } else {
        samples[middle]
    }
}

// FNV-1a over the ordered matches, so two builds can be checked for identical output.
fn fingerprint(matches: &[TokenMatch]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for m in matches {
        for byte in m
            .rule
            .bytes()
            .chain(m.start.0.to_le_bytes())
            .chain(m.end.0.to_le_bytes())
        {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    }
    format!("{hash:016x}")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let usage = "usage: token_matcher_benchmark TOKENS PATTERNS MIX REPEATS [VOCABULARY]";
    let (length, count, mix, repeats, vocabulary) = match &args[..] {
        [length, count, mix, repeats] => (length, count, mix, repeats, 200),
        [length, count, mix, repeats, vocabulary] => {
            (length, count, mix, repeats, vocabulary.parse()?)
        }
        _ => return Err(usage.into()),
    };
    if vocabulary == 0 {
        return Err("VOCABULARY must be positive".into());
    }
    VOCABULARY
        .set(vocabulary)
        .map_err(|_| "vocabulary already set")?;
    let (length, count, repeats): (usize, usize, usize) =
        (length.parse()?, count.parse()?, repeats.parse()?);
    let mix = match mix.as_str() {
        "indexed" => Mix::Indexed,
        "mixed" => Mix::Mixed,
        "unindexed" => Mix::Unindexed,
        _ => return Err("MIX must be indexed, mixed or unindexed".into()),
    };
    if repeats == 0 {
        return Err("REPEATS must be positive".into());
    }
    // A deterministic, non-repeating-looking word sequence.
    let words: Vec<String> = (0..length).map(|i| word(i * 7919 % 9973)).collect();
    let input = doc(&words);
    let patterns: Vec<TokenPattern> = (0..count)
        .map(|i| match mix {
            Mix::Indexed => indexed(i),
            Mix::Unindexed => unindexed(i),
            Mix::Mixed if i % 2 == 0 => indexed(i / 2),
            Mix::Mixed => unindexed(i / 2),
        })
        .collect();
    let names: Vec<String> = (0..count).map(|i| format!("r{i}")).collect();

    let start = Instant::now();
    let mut matcher = TokenMatcher::new();
    for (name, pattern) in names.iter().zip(&patterns) {
        matcher.add(name.clone(), vec![pattern.clone()])?;
    }
    let add_ms = start.elapsed().as_secs_f64() * 1e3;

    let matches = matcher.find_matches(&input)?;
    let mut samples: Vec<f64> = (0..repeats)
        .map(|_| {
            let start = Instant::now();
            black_box(matcher.find_matches(black_box(&input)).unwrap());
            start.elapsed().as_secs_f64() * 1e3
        })
        .collect();
    let find_ms = median(&mut samples);

    let start = Instant::now();
    for name in names.iter().step_by(10) {
        matcher.remove(name)?;
    }
    let remove_ms = start.elapsed().as_secs_f64() * 1e3;
    let after_removal = matcher.find_matches(&input)?;

    println!(
        "{}",
        serde_json::json!({
            "tokens": length, "patterns": count, "mix": args[2], "repeats": repeats,
            "vocabulary": vocabulary,
            "matches": matches.len(), "fingerprint": fingerprint(&matches),
            "matches_after_removal": after_removal.len(),
            "fingerprint_after_removal": fingerprint(&after_removal),
            "add_ms": add_ms, "find_ms": find_ms,
            "find_min_ms": samples[0], "find_max_ms": samples[samples.len() - 1],
            "remove_ms": remove_ms,
        })
    );
    Ok(())
}
