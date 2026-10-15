//! Native phrase matching on token text, annotations, lexical flags or length. No shared
//! vocabulary is required; lexical flag attributes need a model lexicon.
mod terminal;
use crate::dependency_matcher::predicates::{lexical_flag, validate_canonical_morph};
use crate::{Doc, Error, Lexicon, Result, Token, TokenAttribute, TokenIndex};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use terminal::Terminal;

/// String rule identity. Reserved spaCy symbols are resolved internally for ordering.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PhraseRuleId(pub String);
impl From<String> for PhraseRuleId {
    fn from(value: String) -> Self {
        Self(value)
    }
}
impl From<&str> for PhraseRuleId {
    fn from(value: &str) -> Self {
        Self(value.into())
    }
}
impl std::borrow::Borrow<str> for PhraseRuleId {
    fn borrow(&self) -> &str {
        &self.0
    }
}

/// The token value a [`PhraseMatcher`] compares, named as in spaCy.
///
/// ORTH and TEXT match exact text; LOWER uses pinned Python Unicode lowercase. NORM uses the
/// token's norm. LEMMA, POS, TAG, DEP and MORPH compare annotations: a missing value is the
/// empty string, as in spaCy, and the empty morphological analysis is `_`.
///
/// The lexical flags, from `IS_ALPHA` to `LIKE_EMAIL`, compare `true` or `false` for each
/// token and need a matcher created with [`PhraseMatcher::with_lexicon`]. LENGTH compares
/// the number of code points in the token text.
// TODO(phrase-attributes): add spaCy's SHAPE, entity, sentence-start and SPACY attributes.
// Tracked in docs/PHRASE_MATCHER.md#rust-boundaries-and-remaining-scope
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PhraseAttribute {
    #[default]
    Orth,
    Text,
    Lower,
    Norm,
    Lemma,
    Pos,
    Tag,
    Dep,
    Morph,
    IsAlpha,
    IsAscii,
    IsDigit,
    IsLower,
    IsUpper,
    IsTitle,
    IsPunct,
    IsSpace,
    IsBracket,
    IsQuote,
    IsLeftPunct,
    IsRightPunct,
    IsCurrency,
    IsStop,
    LikeNum,
    LikeUrl,
    LikeEmail,
    Length,
}

/// Where an attribute's compared value comes from.
enum Source {
    Text,
    Computed,
    Annotation,
}
impl PhraseAttribute {
    fn source(self) -> Source {
        match self {
            Self::Orth | Self::Text => Source::Text,
            Self::Norm | Self::Lemma | Self::Pos | Self::Tag | Self::Dep | Self::Morph => {
                Source::Annotation
            }
            _ => Source::Computed,
        }
    }
}
fn missing_lexicon() -> Error {
    Error::Pattern("lexical flag attributes require a phrase matcher created with a lexicon".into())
}

// spaCy stores no value as key 0, whose string is empty, and the empty
// morphological analysis as its own symbol `_`.
const MISSING: &str = "";
const EMPTY_MORPH: &str = "_";

impl PhraseAttribute {
    /// The matcher attribute computing the same lexical flag.
    fn flag(self) -> Option<TokenAttribute> {
        Some(match self {
            Self::IsAlpha => TokenAttribute::IsAlpha,
            Self::IsAscii => TokenAttribute::IsAscii,
            Self::IsDigit => TokenAttribute::IsDigit,
            Self::IsLower => TokenAttribute::IsLower,
            Self::IsUpper => TokenAttribute::IsUpper,
            Self::IsTitle => TokenAttribute::IsTitle,
            Self::IsPunct => TokenAttribute::IsPunct,
            Self::IsSpace => TokenAttribute::IsSpace,
            Self::IsBracket => TokenAttribute::IsBracket,
            Self::IsQuote => TokenAttribute::IsQuote,
            Self::IsLeftPunct => TokenAttribute::IsLeftPunct,
            Self::IsRightPunct => TokenAttribute::IsRightPunct,
            Self::IsCurrency => TokenAttribute::IsCurrency,
            Self::IsStop => TokenAttribute::IsStop,
            Self::LikeNum => TokenAttribute::LikeNum,
            Self::LikeUrl => TokenAttribute::LikeUrl,
            Self::LikeEmail => TokenAttribute::LikeEmail,
            _ => return None,
        })
    }
    /// The value of a lexical flag attribute, computed with the model's language rules.
    fn flag_value(self, text: &str, lexicon: Option<&Lexicon>) -> Result<Option<&'static str>> {
        let Some(flag) = self.flag() else {
            return Ok(None);
        };
        let lexicon = lexicon.ok_or_else(missing_lexicon)?;
        Ok(Some(if lexical_flag(lexicon, flag, text)? {
            "true"
        } else {
            "false"
        }))
    }
    /// The compared value for LOWER, the lexical flags and LENGTH, computed from the token text.
    fn computed(self, text: &str, lexicon: Option<&Lexicon>) -> Result<String> {
        Ok(match self {
            Self::Lower => crate::unicode_lower::lower(text),
            Self::Length => text.chars().count().to_string(),
            _ => self
                .flag_value(text, lexicon)?
                .expect("only LOWER, LENGTH and the lexical flags are computed")
                .to_owned(),
        })
    }
    /// The annotation name used in missing-annotation errors, for attributes that
    /// spaCy requires on pattern documents.
    fn required_annotation(self) -> Option<&'static str> {
        match self {
            Self::Lemma => Some("lemma"),
            Self::Pos => Some("POS"),
            Self::Tag => Some("tag"),
            Self::Dep => Some("dependency label"),
            Self::Morph => Some("morphology"),
            _ => None,
        }
    }
    /// The stored value for NORM and the annotation attributes, as spaCy's string for its key.
    fn annotation(self, token: &Token) -> &str {
        match self {
            Self::Norm => &token.norm,
            Self::Lemma => token.lemma.as_deref().unwrap_or(MISSING),
            Self::Pos => token.pos.as_deref().unwrap_or(MISSING),
            Self::Tag => token.tag.as_deref().unwrap_or(MISSING),
            Self::Dep => token.dep.as_deref().unwrap_or(MISSING),
            Self::Morph => match token.morphology.as_deref() {
                None => MISSING,
                Some("") => EMPTY_MORPH,
                Some(value) => value,
            },
            _ => unreachable!("text-derived attributes read token text"),
        }
    }
    // Document morphology is compared as stored, so it must be in spaCy's canonical order.
    fn validate_document(self, doc: &Doc) -> Result<()> {
        if self == Self::Morph {
            for token in doc.tokens() {
                if let Some(value) = token.morphology.as_deref() {
                    validate_canonical_morph(value)?;
                }
            }
        }
        Ok(())
    }
}

/// Owned, unique pattern of token values retained for rule lookup.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct PhrasePattern {
    tokens: Vec<String>,
}
impl PhrasePattern {
    /// The compared value of each pattern token, as described on [`PhraseAttribute`].
    pub fn tokens(&self) -> &[String] {
        &self.tokens
    }
    fn from_doc(doc: &Doc, attribute: PhraseAttribute, lexicon: Option<&Lexicon>) -> Result<Self> {
        let texts = (0..doc.tokens().len()).map(|i| doc.token_text(TokenIndex(i)));
        let tokens: Vec<String> = match attribute.source() {
            Source::Text => texts
                .map(|text| text.map(str::to_owned))
                .collect::<Result<_>>()?,
            Source::Computed => texts
                .map(|text| attribute.computed(text?, lexicon))
                .collect::<Result<_>>()?,
            Source::Annotation => {
                attribute.validate_document(doc)?;
                doc.tokens()
                    .iter()
                    .map(|token| attribute.annotation(token).to_owned())
                    .collect()
            }
        };
        // Like spaCy, a nonempty pattern needs at least one annotated token; empty
        // patterns are skipped before this check.
        if let Some(name) = attribute.required_annotation() {
            if !tokens.is_empty() && tokens.iter().all(|value| value == MISSING) {
                return Err(Error::MissingAnnotation(name));
            }
        }
        Ok(Self { tokens })
    }
}
/// A phrase occurrence; end is an exclusive document token index.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhraseMatch {
    pub rule: PhraseRuleId,
    pub start: TokenIndex,
    pub end: TokenIndex,
}
#[derive(Default)]
struct Node {
    children: HashMap<String, usize>,
    terminal: Option<Terminal>,
}
struct Rule {
    key: u64,
    patterns: Vec<PhrasePattern>,
    // An endpoint stays allocated while this rule owns its terminal.
    unique: HashSet<usize>,
}
/// Reusable prefix trie. Read-only searches own their output and share no mutable state.
pub struct PhraseMatcher {
    attribute: PhraseAttribute,
    lexicon: Option<Lexicon>,
    rules: HashMap<PhraseRuleId, Rule>,
    identities: HashMap<u64, PhraseRuleId>,
    nodes: Vec<Node>,
    free: Vec<usize>,
}
impl Default for PhraseMatcher {
    fn default() -> Self {
        Self::with_attribute(PhraseAttribute::Orth)
    }
}
impl PhraseMatcher {
    pub fn new() -> Self {
        Self::default()
    }
    /// A matcher comparing `attribute`. Lexical flag attributes also need a lexicon; see
    /// [`PhraseMatcher::with_lexicon`].
    pub fn with_attribute(attribute: PhraseAttribute) -> Self {
        Self {
            attribute,
            lexicon: None,
            rules: HashMap::new(),
            identities: HashMap::new(),
            nodes: vec![Node::default()],
            free: Vec::new(),
        }
    }
    /// A matcher comparing `attribute` that computes lexical flags such as `IS_STOP` and
    /// `LIKE_NUM` with the model's language rules, as spaCy matchers use their vocabulary.
    /// Obtain the lexicon with [`crate::Model::lexicon`].
    pub fn with_lexicon(attribute: PhraseAttribute, lexicon: Lexicon) -> Self {
        Self {
            lexicon: Some(lexicon),
            ..Self::with_attribute(attribute)
        }
    }
    pub fn attribute(&self) -> PhraseAttribute {
        self.attribute
    }
    pub fn len(&self) -> usize {
        self.rules.len()
    }
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
    pub fn contains(&self, name: &str) -> bool {
        self.rules.contains_key(name)
    }
    /// Unique nonempty patterns in first-registration order (a Rust inspection API).
    pub fn get(&self, name: &str) -> Option<&[PhrasePattern]> {
        self.rules.get(name).map(|r| r.patterns.as_slice())
    }
    /// Copy the selected token values from validated documents. Empty patterns are ignored but
    /// register the rule. LEMMA, POS, TAG, DEP and MORPH patterns need at least one annotated token.
    /// All fallible validation completes before mutation; duplicate additions still update terminal order.
    pub fn add(&mut self, name: impl Into<PhraseRuleId>, documents: &[&Doc]) -> Result<()> {
        let name = name.into();
        let key = self.check_rule(&name.0)?;
        let patterns = documents
            .iter()
            .map(|doc| PhrasePattern::from_doc(doc, self.attribute, self.lexicon.as_ref()))
            .collect::<Result<Vec<_>>>()?;
        self.identities.insert(key, name.clone());
        self.rules.entry(name.clone()).or_insert_with(|| Rule {
            key,
            patterns: Vec::new(),
            unique: HashSet::new(),
        });
        for pattern in patterns {
            if pattern.tokens.is_empty() {
                continue;
            }
            let mut node = 0;
            for token in &pattern.tokens {
                node = if let Some(&child) = self.nodes[node].children.get(token) {
                    child
                } else {
                    let child = self.free.pop().unwrap_or_else(|| {
                        self.nodes.push(Node::default());
                        self.nodes.len() - 1
                    });
                    self.nodes[node].children.insert(token.clone(), child);
                    child
                };
            }
            self.nodes[node]
                .terminal
                .get_or_insert_with(Terminal::default)
                .insert(key, name.clone());
            let rule = self.rules.get_mut(&name).expect("registered rule");
            if rule.unique.insert(node) {
                rule.patterns.push(pattern);
            }
        }
        Ok(())
    }
    /// Return the error [`PhraseMatcher::add`] would return for these documents, without adding them.
    pub(crate) fn check(&self, name: &str, documents: &[&Doc]) -> Result<()> {
        self.check_rule(name)?;
        documents.iter().try_for_each(|doc| {
            PhrasePattern::from_doc(doc, self.attribute, self.lexicon.as_ref()).map(drop)
        })
    }
    /// The rule's hash key, rejecting a collision with another rule and a flag attribute without
    /// a lexicon.
    fn check_rule(&self, name: &str) -> Result<u64> {
        let key = rule_key(name);
        if self
            .identities
            .get(&key)
            .is_some_and(|other| other.0 != name)
        {
            return Err(Error::Pattern("phrase rule hash collision".into()));
        }
        // Reject even an empty addition, so a flag rule never exists without a lexicon.
        if self.attribute.flag().is_some() && self.lexicon.is_none() {
            return Err(missing_lexicon());
        }
        Ok(key)
    }
    /// Remove a rule and reclaim its unused trie nodes. Unknown names return an error.
    pub fn remove(&mut self, name: &str) -> Result<()> {
        let rule = self
            .rules
            .remove(name)
            .ok_or_else(|| Error::Pattern(format!("unknown phrase rule {name:?}")))?;
        self.identities.remove(&rule.key);
        for pattern in &rule.patterns {
            let mut node = 0;
            let mut path = Vec::with_capacity(pattern.tokens.len());
            for token in &pattern.tokens {
                let child = self.nodes[node].children[token];
                path.push((node, token, child));
                node = child;
            }
            if let Some(terminal) = &mut self.nodes[node].terminal {
                terminal.remove(rule.key);
                if terminal.rules().next().is_none() {
                    self.nodes[node].terminal = None;
                }
            }
            for (parent, token, child) in path.into_iter().rev() {
                if self.nodes[child].terminal.is_some() || !self.nodes[child].children.is_empty() {
                    break;
                }
                self.nodes[parent].children.remove(token);
                self.nodes[child] = Node::default();
                self.free.push(child);
            }
        }
        Ok(())
    }
    /// Return all overlapping occurrences in pinned spaCy start/end/terminal-table order.
    ///
    /// Like spaCy, the input needs no annotations: a token without the selected
    /// annotation matches only a pattern token that also lacks it.
    pub fn find_matches(&self, doc: &Doc) -> Result<Vec<PhraseMatch>> {
        // Validate even without rules, so the outcome does not depend on matcher state.
        self.attribute.validate_document(doc)?;
        if self.rules.is_empty() {
            return Ok(Vec::new());
        }
        // Dispatch once so exact matching does not branch on attributes for
        // every trie edge. Computed keys such as LOWER are made once per input token.
        let attribute = self.attribute;
        match attribute.source() {
            Source::Text => self.find_keys(doc.tokens().len(), |index| {
                doc.token_text(TokenIndex(index))
            }),
            // Plain vectors keep the per-edge key lookup branch-free.
            Source::Computed if attribute == PhraseAttribute::Lower => {
                let lowered = (0..doc.tokens().len())
                    .map(|i| {
                        doc.token_text(TokenIndex(i))
                            .map(crate::unicode_lower::lower)
                    })
                    .collect::<Result<Vec<_>>>()?;
                self.find_keys(doc.tokens().len(), |index| Ok(lowered[index].as_str()))
            }
            Source::Computed if attribute.flag().is_some() => {
                let lexicon = self.lexicon.as_ref();
                let keys = (0..doc.tokens().len())
                    .map(|i| {
                        let value =
                            attribute.flag_value(doc.token_text(TokenIndex(i))?, lexicon)?;
                        Ok(value.expect("flag attribute"))
                    })
                    .collect::<Result<Vec<&str>>>()?;
                self.find_keys(doc.tokens().len(), |index| Ok(keys[index]))
            }
            Source::Computed => {
                let keys = (0..doc.tokens().len())
                    .map(|i| attribute.computed(doc.token_text(TokenIndex(i))?, None))
                    .collect::<Result<Vec<_>>>()?;
                self.find_keys(doc.tokens().len(), |index| Ok(keys[index].as_str()))
            }
            Source::Annotation => {
                let tokens = doc.tokens();
                self.find_keys(tokens.len(), |index| {
                    Ok(attribute.annotation(&tokens[index]))
                })
            }
        }
    }

    fn find_keys<'a>(
        &self,
        count: usize,
        key: impl Fn(usize) -> Result<&'a str>,
    ) -> Result<Vec<PhraseMatch>> {
        let mut matches = Vec::new();
        for start in 0..count {
            let mut node = 0;
            for end in start..count {
                let Some(&child) = self.nodes[node].children.get(key(end)?) else {
                    break;
                };
                node = child;
                if let Some(terminal) = &self.nodes[node].terminal {
                    matches.extend(terminal.rules().map(|rule| PhraseMatch {
                        rule: rule.clone(),
                        start: TokenIndex(start),
                        end: TokenIndex(end + 1),
                    }));
                }
            }
        }
        Ok(matches)
    }
}
pub(crate) fn rule_key(name: &str) -> u64 {
    static SYMBOLS: OnceLock<HashMap<String, u64>> = OnceLock::new();
    *SYMBOLS
        .get_or_init(|| {
            serde_json::from_str(include_str!("symbols.json")).expect("pinned spaCy symbols")
        })
        .get(name)
        .unwrap_or(&crate::hash::hash(name))
}
#[cfg(test)]
mod tests;
