//! Reusable patterns over contiguous tokens, with explicit repetition.
mod engine;
mod start_index;
use crate::dependency_matcher::predicates::{
    compile_conditions, CompiledConditions, LengthChecks, LexicalTest,
};
use crate::{Doc, Error, Lexicon, Result, TokenConstraint, TokenIndex};
use serde::{Deserialize, Serialize};
use start_index::StartIndex;
use std::collections::HashMap;

/// An ordered sequence of token conditions.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenPattern {
    pub tokens: Vec<TokenPatternItem>,
}
/// Conjunctive conditions and the number of tokens they may consume.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenPatternItem {
    pub constraints: Vec<TokenConstraint>,
    pub repetition: Repetition,
}
/// Negation consumes one token whose conditions do not all match.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Repetition {
    #[default]
    Once,
    Optional,
    ZeroOrMore,
    OneOrMore,
    Negated,
    /// Inclusive limits; `None` permits an unbounded number of tokens.
    Range {
        min: usize,
        max: Option<usize>,
    },
}
/// A nonempty match with an exclusive end token index.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenMatch {
    pub rule: String,
    pub start: TokenIndex,
    pub end: TokenIndex,
}
#[derive(Clone, Copy)]
enum Quantifier {
    One,
    Optional,
    Star,
    Negated,
}
struct Node {
    item: usize,
    quantifier: Quantifier,
}
struct CompiledPattern {
    name: String,
    items: Vec<CompiledConditions>,
    lengths: Vec<LengthChecks>,
    nodes: Vec<Node>,
}
struct Rule {
    patterns: Vec<TokenPattern>,
}
/// Compile rules once and reuse them across independent document calls.
#[derive(Default)]
pub struct TokenMatcher {
    rules: Vec<Rule>,
    // The position of each rule in `rules`, so lookups do not scan every rule.
    index: HashMap<String, usize>,
    compiled: Vec<CompiledPattern>,
    // Kept consistent with `compiled` by `add` and `remove`.
    starts: StartIndex,
    lexicon: Option<Lexicon>,
}
impl TokenMatcher {
    pub fn new() -> Self {
        Self::default()
    }
    /// A matcher that can also evaluate lexical flags, such as `LikeNum`, with a
    /// model's language rules. Obtain the lexicon with [`crate::Model::lexicon`].
    pub fn with_lexicon(lexicon: Lexicon) -> Self {
        Self {
            lexicon: Some(lexicon),
            ..Self::default()
        }
    }
    pub fn len(&self) -> usize {
        self.rules.len()
    }
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
    pub fn contains(&self, name: &str) -> bool {
        self.index.contains_key(name)
    }
    pub fn get(&self, name: &str) -> Option<&[TokenPattern]> {
        self.index
            .get(name)
            .map(|&i| self.rules[i].patterns.as_slice())
    }
    /// Validate every pattern before modifying the matcher. Repeated names append patterns.
    pub fn add(&mut self, name: impl Into<String>, patterns: Vec<TokenPattern>) -> Result<()> {
        let name = name.into();
        self.check_lexicon(&patterns)?;
        let compiled = patterns
            .iter()
            .map(|pattern| compile(&name, pattern))
            .collect::<Result<Vec<_>>>()?;
        if let Some(&i) = self.index.get(&name) {
            self.rules[i].patterns.extend(patterns);
        } else {
            self.index.insert(name, self.rules.len());
            self.rules.push(Rule { patterns });
        }
        // This order includes later additions to an already registered rule.
        for pattern in compiled {
            self.starts.push(self.compiled.len(), &pattern);
            self.compiled.push(pattern);
        }
        Ok(())
    }
    /// Return the error [`TokenMatcher::add`] would return for these patterns, without adding them.
    pub(crate) fn check(&self, name: &str, patterns: &[TokenPattern]) -> Result<()> {
        self.check_lexicon(patterns)?;
        patterns
            .iter()
            .try_for_each(|pattern| compile(name, pattern).map(drop))
    }
    fn check_lexicon(&self, patterns: &[TokenPattern]) -> Result<()> {
        if self.lexicon.is_none()
            && patterns
                .iter()
                .flat_map(|pattern| &pattern.tokens)
                .flat_map(|item| &item.constraints)
                .any(|constraint| constraint.attribute.needs_lexicon())
        {
            return Err(Error::Pattern(
                "lexical flag conditions require a matcher created with a lexicon".into(),
            ));
        }
        Ok(())
    }
    pub fn remove(&mut self, name: &str) -> Result<()> {
        let index = self
            .index
            .remove(name)
            .ok_or_else(|| Error::Pattern(format!("unknown token rule {name:?}")))?;
        // Each registered pattern has exactly one compiled pattern.
        let count = self.rules.remove(index).patterns.len();
        // Visiting the values directly avoids hashing every later rule name.
        for position in self.index.values_mut() {
            if *position > index {
                *position -= 1;
            }
        }
        let removed: Vec<usize> = (0..self.compiled.len())
            .filter(|&position| self.compiled[position].name == name)
            .take(count)
            .collect();
        self.starts.remove(&removed);
        if let [only] = removed[..] {
            self.compiled.remove(only);
        } else {
            let mut position = 0;
            self.compiled.retain(|_| {
                position += 1;
                removed.binary_search(&(position - 1)).is_err()
            });
        }
        Ok(())
    }
    /// Return every match, overlapping ones included, in spaCy `Matcher` order rather than
    /// sorted by start. A matcher without rules returns no matches and no error.
    ///
    /// Returns [`Error::MissingAnnotation`] when an `Equals` condition compares lemma, POS, tag,
    /// dependency label or morphology and no token of a nonempty `doc` has that annotation,
    /// like spaCy's error E155; with several such attributes it names the first missing one in
    /// spaCy's order: tag, POS, morphology, lemma, dependency label. An empty lemma, POS, tag or
    /// label counts as missing; empty morphology counts as present. Other predicates never
    /// require the annotation, and a token without it compares as the empty string (see the
    /// Token Matcher guide for missing morphology). Returns [`Error::Unsupported`] when any
    /// condition compares morphology and a token's morphology is not in spaCy's canonical
    /// `Field=Value|...` order.
    pub fn find_matches(&self, doc: &Doc) -> Result<Vec<TokenMatch>> {
        engine::find(
            &self.compiled,
            engine::Starts::Indexed(&self.starts),
            doc,
            self.lexicon.as_ref(),
        )
    }
    /// The same search, starting every pattern at every token as spaCy does.
    #[cfg(test)]
    fn find_matches_unindexed(&self, doc: &Doc) -> Result<Vec<TokenMatch>> {
        engine::find(
            &self.compiled,
            engine::Starts::All,
            doc,
            self.lexicon.as_ref(),
        )
    }
}
fn compile(name: &str, pattern: &TokenPattern) -> Result<CompiledPattern> {
    const MAX_NODES: usize = 4096;
    if pattern.tokens.is_empty() {
        return Err(Error::Pattern("token pattern must contain an item".into()));
    }
    let mut nodes = Vec::new();
    let mut items = Vec::new();
    let mut lengths = Vec::new();
    for (item, spec) in pattern.tokens.iter().enumerate() {
        let (required, optional, star, negated) = match spec.repetition {
            Repetition::Once => (1, 0, false, false),
            Repetition::Negated => (1, 0, false, true),
            Repetition::Optional => (0, 1, false, false),
            Repetition::ZeroOrMore => (0, 0, true, false),
            Repetition::OneOrMore => (1, 0, true, false),
            Repetition::Range { min, max } => match max {
                Some(max) if max >= min => (min, max - min, false, false),
                None => (min, 0, true, false),
                _ => {
                    return Err(Error::Pattern(
                        "repetition maximum must be at least its minimum".into(),
                    ))
                }
            },
        };
        let added = required
            .checked_add(optional)
            .and_then(|n| n.checked_add(usize::from(star)))
            .and_then(|n| n.checked_add(nodes.len()));
        if added.is_none_or(|n| n > MAX_NODES) {
            return Err(Error::Pattern(format!(
                "token pattern exceeds {MAX_NODES} expanded nodes"
            )));
        }
        let compiled = compile_conditions(&spec.constraints, &mut lengths)?;
        items.push(if required == 0 && optional == 0 && !star {
            CompiledConditions {
                constraints: Vec::new(),
                lexical: LexicalTest::default(),
            }
        } else {
            compiled
        });
        nodes.extend((0..required).map(|_| Node {
            item,
            quantifier: if negated {
                Quantifier::Negated
            } else {
                Quantifier::One
            },
        }));
        nodes.extend((0..optional).map(|_| Node {
            item,
            quantifier: Quantifier::Optional,
        }));
        if star {
            nodes.push(Node {
                item,
                quantifier: Quantifier::Star,
            });
        }
    }
    Ok(CompiledPattern {
        name: name.into(),
        items,
        lengths,
        nodes,
    })
}
#[cfg(test)]
mod tests;
