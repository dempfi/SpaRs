use crate::{Doc, Error, Lexicon, Result, TokenIndex, TokenView};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::{BTreeMap, HashSet};

/// Token annotations shared by token and dependency patterns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenAttribute {
    Text,
    /// Python Unicode 15 `str.lower()` of the token text, as in spaCy's LOWER.
    /// Pattern values are compared as written and are not lowercased.
    Lower,
    Norm,
    Lemma,
    Pos,
    Tag,
    Dep,
    Morphology,
    /// Lexical flags from the matcher's [`Lexicon`], compared with [`Predicate::Flag`].
    /// Like spaCy's `IS_ALPHA`, `IS_DIGIT`, `IS_SPACE`, `IS_PUNCT` and `LIKE_NUM`.
    IsAlpha,
    IsDigit,
    IsSpace,
    IsPunct,
    LikeNum,
    /// The number of Unicode code points in the token text, like spaCy's `LENGTH`.
    /// Compared with [`Predicate::Compare`] and the integer membership and set predicates
    /// such as [`Predicate::InIntegers`] and [`Predicate::IsSubsetIntegers`].
    Length,
    /// More lexical flags from the matcher's [`Lexicon`], compared with
    /// [`Predicate::Flag`]: spaCy's `IS_LOWER`, `IS_UPPER`, `IS_TITLE`, `IS_ASCII`,
    /// `IS_CURRENCY`, `IS_STOP`, `IS_BRACKET`, `IS_QUOTE`, `IS_LEFT_PUNCT`,
    /// `IS_RIGHT_PUNCT`, `LIKE_URL` and `LIKE_EMAIL`.
    IsLower,
    IsUpper,
    IsTitle,
    IsAscii,
    IsCurrency,
    IsStop,
    IsBracket,
    IsQuote,
    IsLeftPunct,
    IsRightPunct,
    LikeUrl,
    LikeEmail,
}

impl TokenAttribute {
    /// The bit for a lexical flag attribute in per-token flag masks.
    fn flag_bit(self) -> Option<u32> {
        FLAG_ATTRIBUTES
            .iter()
            .position(|attribute| *attribute == self)
            .map(|index| 1 << index)
    }
    /// Whether this attribute needs a matcher created with a [`Lexicon`].
    pub(crate) fn needs_lexicon(self) -> bool {
        self.flag_bit().is_some()
    }
}

/// A numeric comparison operator, spelled as in spaCy patterns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Comparison {
    #[serde(rename = "==")]
    Equal,
    #[serde(rename = "!=")]
    NotEqual,
    #[serde(rename = ">=")]
    GreaterOrEqual,
    #[serde(rename = "<=")]
    LessOrEqual,
    #[serde(rename = ">")]
    Greater,
    #[serde(rename = "<")]
    Less,
}

/// A finite number for numeric comparisons. NaN and infinities are rejected, so
/// comparisons have a total, well-defined result.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct FiniteNumber(f64);

impl FiniteNumber {
    pub fn new(value: f64) -> Option<Self> {
        value.is_finite().then_some(Self(value))
    }
    pub fn get(self) -> f64 {
        self.0
    }
}
// Construction rejects NaN, so equality is reflexive.
impl Eq for FiniteNumber {}
impl TryFrom<f64> for FiniteNumber {
    type Error = &'static str;
    fn try_from(value: f64) -> std::result::Result<Self, Self::Error> {
        Self::new(value).ok_or("numeric pattern values must be finite")
    }
}
impl From<FiniteNumber> for f64 {
    fn from(value: FiniteNumber) -> Self {
        value.0
    }
}

/// A comparison applied to one token attribute. String predicates apply to string
/// attributes; `Flag` applies only to lexical flag attributes; `Compare` and the
/// `*Integers` predicates apply only to `Length`.
///
/// The set predicates follow spaCy's `IS_SUBSET`, `IS_SUPERSET` and `INTERSECTS`. A
/// token's value is treated as a one-element set, except morphology, which is the set
/// of its individual `Field=Value` features. So on a string attribute `IsSubset` and
/// `Intersects` hold when the value is listed, and `IsSuperset` holds when every
/// listed value equals it, including for an empty list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Predicate {
    Equals {
        value: String,
    },
    In {
        values: Vec<String>,
    },
    NotIn {
        values: Vec<String>,
    },
    /// The same as [`Predicate::IsSuperset`] on morphology, and valid only there.
    MorphSuperset {
        values: Vec<String>,
    },
    /// The same as [`Predicate::Intersects`] on morphology, and valid only there.
    MorphIntersects {
        values: Vec<String>,
    },
    IsSubset {
        values: Vec<String>,
    },
    IsSuperset {
        values: Vec<String>,
    },
    Intersects {
        values: Vec<String>,
    },
    Flag {
        value: bool,
    },
    Compare {
        operator: Comparison,
        value: FiniteNumber,
    },
    InIntegers {
        values: Vec<i64>,
    },
    NotInIntegers {
        values: Vec<i64>,
    },
    IsSubsetIntegers {
        values: Vec<i64>,
    },
    IsSupersetIntegers {
        values: Vec<i64>,
    },
    IntersectsIntegers {
        values: Vec<i64>,
    },
}

/// All constraints on a pattern node must match the same token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenConstraint {
    pub attribute: TokenAttribute,
    pub predicate: Predicate,
}

#[derive(Debug, Clone)]
enum CompiledPredicate {
    Equals(String),
    In(HashSet<String>),
    NotIn(HashSet<String>),
    // Morphology, compared with its individual `Field=Value` features. Set predicates
    // on one-valued attributes compile to the variants above.
    Superset(HashSet<String>),
    Intersects(HashSet<String>),
}
#[derive(Debug, Clone)]
pub(crate) struct CompiledConstraint {
    attribute: TokenAttribute,
    // Set for morphology `IsSubset`, which reuses the `Superset` list: every feature of
    // the token must be listed. A flag rather than another `CompiledPredicate` variant
    // keeps the per-token comparison compiled as for the other predicates; extra
    // variants measurably slowed every text comparison.
    subset: bool,
    // Set for a direct `Equals`, the only condition for which spaCy's `Matcher` requires a
    // token with the annotation (error E155); one-value set predicates also compile to
    // `CompiledPredicate::Equals` but never require it.
    requires_annotation: bool,
    // The result for a token without the annotation, fixed at compilation; see
    // `missing_result`. Unannotated documents check it once per token and constraint.
    missing: bool,
    predicate: CompiledPredicate,
}

#[derive(Debug, Clone)]
enum LengthCheck {
    Compare(Comparison, f64),
    In(HashSet<i64>),
    NotIn(HashSet<i64>),
    // `IsSubsetIntegers` and `IntersectsIntegers` on one value are `In`.
    Superset(HashSet<i64>),
}

impl LengthCheck {
    fn matches(&self, length: u32) -> bool {
        let value = f64::from(length);
        match self {
            LengthCheck::Compare(Comparison::Equal, expected) => value == *expected,
            LengthCheck::Compare(Comparison::NotEqual, expected) => value != *expected,
            LengthCheck::Compare(Comparison::GreaterOrEqual, expected) => value >= *expected,
            LengthCheck::Compare(Comparison::LessOrEqual, expected) => value <= *expected,
            LengthCheck::Compare(Comparison::Greater, expected) => value > *expected,
            LengthCheck::Compare(Comparison::Less, expected) => value < *expected,
            LengthCheck::In(expected) => expected.contains(&i64::from(length)),
            LengthCheck::NotIn(expected) => !expected.contains(&i64::from(length)),
            // spaCy's `IS_SUPERSET` with one actual value: every listed value equals it.
            LengthCheck::Superset(expected) => {
                expected.is_empty()
                    || (expected.len() == 1 && expected.contains(&i64::from(length)))
            }
        }
    }
}

/// The text-derived conditions of one pattern item or node: lexical flags compiled
/// into one mask comparison, and a marker for `LENGTH` checks, which live in the
/// pattern's table at the item's position. It stays 8 bytes so pattern items keep
/// two per cache line; the matcher hot path visits every item for every token.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct LexicalTest {
    mask: u32,
    expected: u32,
}

/// The `LENGTH` checks of one item, stored per compiled pattern at the item's position.
#[derive(Debug, Clone, Default)]
pub(crate) struct LengthChecks(Vec<LengthCheck>);

// Set in `LexicalTest::mask` when the item has LENGTH checks. Flag bits use 0..=16.
const LENGTH_BIT: u32 = 1 << 31;
// Set in `expected`, never in `mask`, when a flag must be both true and false, so the
// comparison `flags & mask == expected` can never succeed.
const CONTRADICTION_BIT: u32 = 1 << 30;

impl LexicalTest {
    fn require(&mut self, bit: u32, value: bool) {
        if self.mask & bit != 0 && (self.expected & bit != 0) != value {
            self.expected |= CONTRADICTION_BIT;
        }
        self.mask |= bit;
        if value {
            self.expected |= bit;
        }
    }
    /// Lexical flag bits only; `LENGTH_BIT` marks length checks, not a flag.
    pub(crate) fn mask(&self) -> u32 {
        self.mask & !LENGTH_BIT
    }
    pub(crate) fn needs_length(&self) -> bool {
        self.mask & LENGTH_BIT != 0
    }
    /// `item` is the position of this item in its pattern, which indexes `lengths`.
    #[inline]
    pub(crate) fn matches(
        &self,
        values: &TokenValues<'_>,
        index: usize,
        lengths: &[LengthChecks],
        item: usize,
    ) -> Result<bool> {
        // Most items have no flag or LENGTH conditions: one comparison.
        if self.mask == 0 {
            return Ok(true);
        }
        if self.mask & LENGTH_BIT == 0 {
            let actual = values.flags.get(index).ok_or_else(missing_lexicon)?;
            return Ok(actual & self.mask == self.expected);
        }
        self.matches_lengths(values, index, lengths, item)
    }
    // Items with LENGTH checks, and any flags they also have, are handled out of line.
    #[inline(never)]
    fn matches_lengths(
        &self,
        values: &TokenValues<'_>,
        index: usize,
        lengths: &[LengthChecks],
        item: usize,
    ) -> Result<bool> {
        let flags = self.mask & !LENGTH_BIT;
        if flags != 0 {
            let actual = values.flags.get(index).ok_or_else(missing_lexicon)?;
            if actual & flags != self.expected {
                return Ok(false);
            }
        }
        let checks = lengths.get(item).ok_or_else(missing_lengths)?;
        let length = *values.lengths.get(index).ok_or_else(missing_lengths)?;
        Ok(checks.0.iter().all(|check| check.matches(length)))
    }
}

/// The compiled conditions shared by one pattern item or dependency node.
pub(crate) struct CompiledConditions {
    pub(crate) constraints: Vec<CompiledConstraint>,
    pub(crate) lexical: LexicalTest,
}

// Pattern items are scanned for every token; keep them two per 64-byte cache line.
const _: () = assert!(std::mem::size_of::<CompiledConditions>() == 32);
// Constraints are also scanned per token; the flags fit in existing padding.
const _: () = assert!(std::mem::size_of::<CompiledConstraint>() == 64);

/// Compile one item's conditions. Its `LENGTH` checks are appended to the pattern's
/// `lengths` table, which therefore has one entry per item, in item order.
pub(crate) fn compile_conditions(
    constraints: &[TokenConstraint],
    lengths: &mut Vec<LengthChecks>,
) -> Result<CompiledConditions> {
    let mut compiled = Vec::new();
    let mut lexical = LexicalTest::default();
    let mut checks = Vec::new();
    for constraint in constraints {
        let numeric = matches!(
            constraint.predicate,
            Predicate::Compare { .. }
                | Predicate::InIntegers { .. }
                | Predicate::NotInIntegers { .. }
                | Predicate::IsSubsetIntegers { .. }
                | Predicate::IsSupersetIntegers { .. }
                | Predicate::IntersectsIntegers { .. }
        );
        if (constraint.attribute == TokenAttribute::Length) != numeric {
            return Err(Error::Pattern(
                "LENGTH requires numeric predicates, and numeric predicates require LENGTH".into(),
            ));
        }
        match (constraint.attribute.flag_bit(), &constraint.predicate) {
            (Some(bit), Predicate::Flag { value }) => lexical.require(bit, *value),
            (_, Predicate::Compare { operator, value }) => {
                checks.push(LengthCheck::Compare(*operator, value.get()))
            }
            (_, Predicate::InIntegers { values })
            | (_, Predicate::IsSubsetIntegers { values })
            | (_, Predicate::IntersectsIntegers { values }) => {
                checks.push(LengthCheck::In(values.iter().copied().collect()))
            }
            (_, Predicate::IsSupersetIntegers { values }) => {
                checks.push(LengthCheck::Superset(values.iter().copied().collect()))
            }
            (_, Predicate::NotInIntegers { values }) => {
                checks.push(LengthCheck::NotIn(values.iter().copied().collect()))
            }
            _ => compiled.push(constraint.compile()?),
        }
    }
    if !checks.is_empty() {
        lexical.mask |= LENGTH_BIT;
    }
    lengths.push(LengthChecks(checks));
    Ok(CompiledConditions {
        constraints: compiled,
        lexical,
    })
}

#[cold]
fn missing_lengths() -> Error {
    Error::Pattern("LENGTH conditions require token lengths for this call".into())
}

#[cold]
fn missing_lexicon() -> Error {
    Error::Pattern("lexical flag conditions require a matcher lexicon".into())
}

/// Which per-token values a matching call needs, derived from its registered conditions.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Needs {
    pub(crate) lower: bool,
    pub(crate) flags: u32,
    pub(crate) length: bool,
}

impl Needs {
    pub(crate) fn add(&mut self, lexical: &LexicalTest) {
        self.flags |= lexical.mask();
        self.length |= lexical.needs_length();
    }
}

/// Token values derived once per matching call and shared by every constraint.
/// Lowercase text, lexical flags and lengths are computed only when in use.
pub(crate) struct TokenValues<'a> {
    lower: Vec<Cow<'a, str>>,
    flags: Vec<u32>,
    lengths: Vec<u32>,
}

impl<'a> TokenValues<'a> {
    pub(crate) fn new(doc: &'a Doc, needs: Needs, lexicon: Option<&Lexicon>) -> Result<Self> {
        let mask = needs.flags;
        let texts = || (0..doc.tokens().len()).map(|index| doc.token_text(TokenIndex(index)));
        let lower = if needs.lower {
            texts().map(|text| text.map(lower)).collect::<Result<_>>()?
        } else {
            Vec::new()
        };
        let flags = if mask == 0 {
            Vec::new()
        } else {
            let lexicon = lexicon.ok_or_else(missing_lexicon)?;
            texts()
                .map(|text| {
                    let text = text?;
                    let mut flags = 0;
                    for (position, attribute) in FLAG_ATTRIBUTES.iter().enumerate() {
                        let bit = 1 << position;
                        if mask & bit != 0 && lexical_flag(lexicon, *attribute, text)? {
                            flags |= bit;
                        }
                    }
                    Ok(flags)
                })
                .collect::<Result<_>>()?
        };
        let lengths = if needs.length {
            texts()
                .map(|text| {
                    // Python len(): code points. Token text is far below u32::MAX.
                    Ok(u32::try_from(text?.chars().count()).unwrap_or(u32::MAX))
                })
                .collect::<Result<_>>()?
        } else {
            Vec::new()
        };
        Ok(Self {
            lower,
            flags,
            lengths,
        })
    }
}

// Bit positions in per-token flag masks. Bits 30 and 31 are reserved by `LexicalTest`.
const FLAG_ATTRIBUTES: [TokenAttribute; 17] = [
    TokenAttribute::IsAlpha,
    TokenAttribute::IsDigit,
    TokenAttribute::IsSpace,
    TokenAttribute::IsPunct,
    TokenAttribute::LikeNum,
    TokenAttribute::IsLower,
    TokenAttribute::IsUpper,
    TokenAttribute::IsTitle,
    TokenAttribute::IsAscii,
    TokenAttribute::IsCurrency,
    TokenAttribute::IsStop,
    TokenAttribute::IsBracket,
    TokenAttribute::IsQuote,
    TokenAttribute::IsLeftPunct,
    TokenAttribute::IsRightPunct,
    TokenAttribute::LikeUrl,
    TokenAttribute::LikeEmail,
];

// The same functions compute these fields in `Model::lexeme`.
pub(crate) fn lexical_flag(
    lexicon: &Lexicon,
    attribute: TokenAttribute,
    text: &str,
) -> Result<bool> {
    let r = lexicon.resources();
    Ok(match attribute {
        TokenAttribute::IsAlpha => r.is_alpha(text),
        TokenAttribute::IsDigit => r.is_digit(text),
        TokenAttribute::IsSpace => r.is_space(text),
        TokenAttribute::IsPunct => r.is_punct(text),
        TokenAttribute::LikeNum => r.like_num(text),
        TokenAttribute::IsLower => r.is_lower(text),
        TokenAttribute::IsUpper => r.is_upper(text),
        TokenAttribute::IsTitle => r.is_title(text),
        TokenAttribute::IsAscii => r.is_ascii(text),
        TokenAttribute::IsCurrency => r.is_currency(text),
        TokenAttribute::IsStop => r.is_stop(text),
        TokenAttribute::IsBracket => r.is_bracket(text),
        TokenAttribute::IsQuote => r.is_quote(text),
        TokenAttribute::IsLeftPunct => r.is_left_punct(text),
        TokenAttribute::IsRightPunct => r.is_right_punct(text),
        TokenAttribute::LikeUrl => lexicon.like_url(text)?,
        TokenAttribute::LikeEmail => lexicon.like_email(text)?,
        // Listed explicitly so a new flag attribute must add its own arm above.
        TokenAttribute::Text
        | TokenAttribute::Lower
        | TokenAttribute::Norm
        | TokenAttribute::Lemma
        | TokenAttribute::Pos
        | TokenAttribute::Tag
        | TokenAttribute::Dep
        | TokenAttribute::Morphology
        | TokenAttribute::Length => false,
    })
}

impl TokenConstraint {
    pub(crate) fn compile(&self) -> Result<CompiledConstraint> {
        let is_morph = self.attribute == TokenAttribute::Morphology;
        let set = |values: &[String]| -> Result<HashSet<String>> {
            values
                .iter()
                .map(|value| {
                    if is_morph {
                        normalize_morph(value)
                    } else {
                        Ok(value.clone())
                    }
                })
                .collect()
        };
        let mismatched_flag = || {
            Error::Pattern(
                "lexical flag and LENGTH attributes require flag or numeric predicates, which apply only to them".into(),
            )
        };
        // Valid flag and LENGTH conditions compile into a `LexicalTest` in
        // `compile_conditions`; only string predicates reach this point.
        if self.attribute.needs_lexicon() || self.attribute == TokenAttribute::Length {
            return Err(mismatched_flag());
        }
        let mut subset = false;
        let predicate = match &self.predicate {
            Predicate::Flag { .. }
            | Predicate::Compare { .. }
            | Predicate::InIntegers { .. }
            | Predicate::NotInIntegers { .. }
            | Predicate::IsSubsetIntegers { .. }
            | Predicate::IsSupersetIntegers { .. }
            | Predicate::IntersectsIntegers { .. } => return Err(mismatched_flag()),
            Predicate::Equals { value } => CompiledPredicate::Equals(value.clone()),
            Predicate::In { values } => CompiledPredicate::In(set(values)?),
            Predicate::NotIn { values } => CompiledPredicate::NotIn(set(values)?),
            Predicate::MorphSuperset { values } | Predicate::MorphIntersects { values } => {
                if !is_morph {
                    return Err(Error::Pattern(
                        "morph_superset and morph_intersects require the morphology attribute; use is_superset or intersects for other attributes".into(),
                    ));
                }
                if matches!(self.predicate, Predicate::MorphSuperset { .. }) {
                    CompiledPredicate::Superset(set(values)?)
                } else {
                    CompiledPredicate::Intersects(set(values)?)
                }
            }
            Predicate::IsSubset { values } if is_morph => {
                subset = true;
                CompiledPredicate::Superset(set(values)?)
            }
            Predicate::IsSuperset { values } if is_morph => {
                CompiledPredicate::Superset(set(values)?)
            }
            Predicate::Intersects { values } if is_morph => {
                CompiledPredicate::Intersects(set(values)?)
            }
            // A one-element set is a subset of, or intersects, the list exactly when
            // its value is listed.
            Predicate::IsSubset { values } | Predicate::Intersects { values } => {
                CompiledPredicate::In(set(values)?)
            }
            // A one-element set is a superset of the list when every listed value
            // equals its value: always for an empty list, never for two distinct values.
            Predicate::IsSuperset { values } => {
                let values = set(values)?;
                let mut distinct = values.iter();
                match (distinct.next(), distinct.next()) {
                    (None, _) => CompiledPredicate::NotIn(HashSet::new()),
                    (Some(value), None) => CompiledPredicate::Equals(value.clone()),
                    (Some(_), Some(_)) => CompiledPredicate::In(HashSet::new()),
                }
            }
        };
        let missing = missing_result(self.attribute, subset, &predicate);
        Ok(CompiledConstraint {
            attribute: self.attribute,
            subset,
            requires_annotation: matches!(self.predicate, Predicate::Equals { .. }),
            missing,
            predicate,
        })
    }
}

/// A string attribute that every token has without annotation, so matchers can look up
/// conditions by a token's value of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextAttribute {
    Text,
    Lower,
    Norm,
}

impl TextAttribute {
    /// The token's value, as [`CompiledConstraint::matches`] compares it.
    pub(crate) fn value<'a>(
        self,
        token: TokenView<'a>,
        values: &'a TokenValues<'a>,
    ) -> Cow<'a, str> {
        match self {
            TextAttribute::Text => Cow::Borrowed(token.text()),
            TextAttribute::Lower => match values.lower.get(token.index().0) {
                Some(value) => Cow::Borrowed(value.as_ref()),
                None => lower(token.text()),
            },
            TextAttribute::Norm => Cow::Borrowed(&token.annotations().norm),
        }
    }
}

impl CompiledConstraint {
    #[cfg(test)]
    pub(crate) fn attribute(&self) -> TokenAttribute {
        self.attribute
    }
    /// The only values a token can have when this constraint holds, if the constraint is an
    /// exact `Text`, `Lower` or `Norm` comparison: `Equals`, or `In` and its one-valued set
    /// forms. The values are distinct; when there are none, no token matches.
    pub(crate) fn exact_values(&self) -> Option<(TextAttribute, impl Iterator<Item = &str>)> {
        let attribute = match self.attribute {
            TokenAttribute::Text => TextAttribute::Text,
            TokenAttribute::Lower => TextAttribute::Lower,
            TokenAttribute::Norm => TextAttribute::Norm,
            _ => return None,
        };
        let (one, set) = match &self.predicate {
            CompiledPredicate::Equals(value) => (Some(value.as_str()), None),
            CompiledPredicate::In(values) => (None, Some(values)),
            CompiledPredicate::NotIn(_)
            | CompiledPredicate::Superset(_)
            | CompiledPredicate::Intersects(_) => return None,
        };
        let values = one
            .into_iter()
            .chain(set.into_iter().flatten().map(String::as_str));
        Some((attribute, values))
    }
    /// Document morphology is compared as stored, so a morphology condition needs every
    /// present value in spaCy's canonical form. The result depends only on the attribute.
    fn validate_format(&self, doc: &Doc) -> Result<()> {
        if self.attribute == TokenAttribute::Morphology {
            for value in doc.tokens().iter().filter_map(|t| t.morphology.as_deref()) {
                validate_canonical_morph(value)?;
            }
        }
        Ok(())
    }
    /// spaCy's `Matcher` check before matching (error E155): a direct `Equals` on lemma, POS,
    /// tag, dependency label or morphology needs at least one token with that annotation, as
    /// `Doc.has_annotation` tests it. Other tokens compare as missing; see `missing_result`.
    /// The result depends only on the attribute.
    fn validate_presence(&self, doc: &Doc) -> Result<()> {
        if !self.requires_annotation || doc.tokens().is_empty() {
            return Ok(());
        }
        // Check presence directly; the per-check value lookup is not needed here.
        let (field, name): (fn(&crate::Token) -> Option<&str>, _) = match self.attribute {
            TokenAttribute::Lemma => (|t| t.lemma.as_deref(), "lemma"),
            TokenAttribute::Pos => (|t| t.pos.as_deref(), "POS"),
            TokenAttribute::Tag => (|t| t.tag.as_deref(), "tag"),
            TokenAttribute::Dep => (|t| t.dep.as_deref(), "dependency label"),
            TokenAttribute::Morphology => (|t| t.morphology.as_deref(), "morphology"),
            // Text-derived attributes are always available.
            _ => return Ok(()),
        };
        // spaCy stores an empty lemma, POS, tag or label as its unset value, while the
        // empty morphology analysis `_` is set. An empty document counts as annotated.
        let is_morph = self.attribute == TokenAttribute::Morphology;
        let annotated =
            |token: &crate::Token| field(token).is_some_and(|v| is_morph || !v.is_empty());
        if doc.tokens().iter().any(annotated) {
            Ok(())
        } else {
            Err(Error::MissingAnnotation(name))
        }
    }
    /// Both document checks, for tests of a single constraint.
    #[cfg(test)]
    pub(crate) fn validate_document(&self, doc: &Doc) -> Result<()> {
        self.validate_format(doc)?;
        self.validate_presence(doc)
    }
    fn value<'a>(
        &self,
        token: TokenView<'a>,
        values: Option<&'a TokenValues<'a>>,
    ) -> Result<Option<Cow<'a, str>>> {
        let t = token.annotations();
        let value = match self.attribute {
            TokenAttribute::Text => return Ok(Some(Cow::Borrowed(token.text()))),
            TokenAttribute::Lower => {
                let cached = values.and_then(|values| values.lower.get(token.index().0));
                return Ok(Some(match cached {
                    Some(value) => Cow::Borrowed(value.as_ref()),
                    None => lower(token.text()),
                }));
            }
            TokenAttribute::Norm => return Ok(Some(Cow::Borrowed(&t.norm))),
            TokenAttribute::Lemma => t.lemma.as_deref(),
            TokenAttribute::Pos => t.pos.as_deref(),
            TokenAttribute::Tag => t.tag.as_deref(),
            TokenAttribute::Dep => t.dep.as_deref(),
            TokenAttribute::Morphology => t.morphology.as_deref(),
            // Flags and LENGTH compile into `LexicalTest`, never into string constraints.
            _ => return Err(no_string_value()),
        };
        Ok(value.map(Cow::Borrowed))
    }
    pub(crate) fn matches<'a>(
        &self,
        token: TokenView<'a>,
        values: &'a TokenValues<'a>,
    ) -> Result<bool> {
        let Some(value) = self.value(token, Some(values))? else {
            return Ok(self.missing);
        };
        let value = value.as_ref();
        let feature = |wanted: &String| -> bool {
            let Some((key, val)) = wanted.split_once('=') else {
                return false;
            };
            value.split('|').any(|field| {
                field
                    .split_once('=')
                    .is_some_and(|(k, values)| k == key && values.split(',').any(|v| v == val))
            })
        };
        Ok(match &self.predicate {
            CompiledPredicate::Equals(expected) => {
                let actual = if self.attribute == TokenAttribute::Morphology && value.is_empty() {
                    "_"
                } else {
                    value
                };
                actual == expected
            }
            CompiledPredicate::In(expected) => expected.contains(value),
            CompiledPredicate::NotIn(expected) => !expected.contains(value),
            CompiledPredicate::Superset(expected) if self.subset => morph_subset(value, expected),
            CompiledPredicate::Superset(expected) => expected.iter().all(feature),
            CompiledPredicate::Intersects(expected) => expected.iter().any(feature),
        })
    }
}

/// The document checks that precede matching, for every constraint of a matcher's patterns.
/// Morphology format errors come first; then, like spaCy's `Matcher.__call__`, a missing
/// annotation required by an `Equals` condition is reported for the first attribute in
/// spaCy's order: tag, POS, morphology, lemma, dependency label. Returns whether any
/// constraint compares `Lower`, which needs per-call lowercase text.
// spaCy keeps requiring annotations compared by `Equals` conditions of removed rules; SpaRs
// checks the current patterns only, an accepted difference in docs/SPACY_DIFFERENCES.md.
pub(crate) fn validate_constraints<'c>(
    constraints: impl IntoIterator<Item = &'c CompiledConstraint>,
    doc: &Doc,
) -> Result<bool> {
    const ORDER: [TokenAttribute; 5] = [
        TokenAttribute::Tag,
        TokenAttribute::Pos,
        TokenAttribute::Morphology,
        TokenAttribute::Lemma,
        TokenAttribute::Dep,
    ];
    let mut required: [Option<&CompiledConstraint>; 5] = [None; 5];
    let (mut lower, mut morphology_checked) = (false, false);
    for constraint in constraints {
        lower |= constraint.attribute == TokenAttribute::Lower;
        if constraint.attribute == TokenAttribute::Morphology && !morphology_checked {
            morphology_checked = true;
            constraint.validate_format(doc)?;
        }
        // Membership conditions on the same attribute must not hide an `Equals`.
        if constraint.requires_annotation {
            if let Some(slot) = ORDER.iter().position(|a| *a == constraint.attribute) {
                required[slot].get_or_insert(constraint);
            }
        }
    }
    for constraint in required.into_iter().flatten() {
        constraint.validate_presence(doc)?;
    }
    Ok(lower)
}

/// The result for a token without the annotation, which spaCy stores as its unset value and
/// compares as the empty string. Missing morphology is the unset analysis: it equals only `""`,
/// and unlike the empty analysis `_` it is never a member of a normalized `In` or `NotIn` list.
/// As a feature set it is empty.
fn missing_result(attribute: TokenAttribute, subset: bool, predicate: &CompiledPredicate) -> bool {
    let is_morph = attribute == TokenAttribute::Morphology;
    match predicate {
        CompiledPredicate::Equals(expected) => expected.is_empty(),
        CompiledPredicate::In(expected) => !is_morph && expected.contains(""),
        CompiledPredicate::NotIn(expected) => is_morph || !expected.contains(""),
        CompiledPredicate::Superset(_) if subset => true,
        CompiledPredicate::Superset(expected) => expected.is_empty(),
        CompiledPredicate::Intersects(_) => false,
    }
}

/// spaCy's `IS_SUBSET` on morphology: every individual `Field=Value` feature of the
/// token is listed. Listed entries with several values or fields never equal one
/// feature, as in spaCy, and an empty analysis is a subset of any list.
fn morph_subset(value: &str, expected: &HashSet<String>) -> bool {
    if value.is_empty() {
        return true;
    }
    // Multi-valued fields are rebuilt one feature at a time in one reused buffer.
    let mut feature = String::new();
    for field in value.split('|') {
        let Some((key, values)) = field.split_once('=') else {
            return false;
        };
        if !values.contains(',') {
            if !expected.contains(field) {
                return false;
            }
            continue;
        }
        for val in values.split(',') {
            feature.clear();
            feature.push_str(key);
            feature.push('=');
            feature.push_str(val);
            if !expected.contains(feature.as_str()) {
                return false;
            }
        }
    }
    true
}

#[cold]
fn no_string_value() -> Error {
    Error::Pattern("lexical flags and LENGTH have no string value".into())
}

// Most tokens are lowercase ASCII already; avoid allocating for them.
fn lower(text: &str) -> Cow<'_, str> {
    if text.is_ascii() && !text.bytes().any(|byte| byte.is_ascii_uppercase()) {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(crate::unicode_lower::lower(text))
    }
}

// spaCy's set predicates normalize complete morphology strings; direct equality
// instead compares the supplied string. Feature-set matching uses atomic values.
// TODO(morph-validation): spaCy accepts entries with an empty field or value, such
// as `Number=`, `=Sing` or `PronType=Int,`, and entries with spaces, and they never
// match; SpaRs rejects them as pattern errors, and rejects document morphology with
// empty values such as `POS=` as unsupported. Tracked in docs/COMPATIBILITY.md.
fn normalize_morph(value: &str) -> Result<String> {
    if value.is_empty() || value == "_" {
        return Ok(String::new());
    }
    let mut raw_fields: Vec<(&str, Vec<String>)> = Vec::new();
    for field in value.split('|') {
        let (key, values) = field
            .split_once('=')
            .ok_or_else(|| Error::Pattern("morphology entries must use Field=Value".into()))?;
        if key.is_empty()
            || values.is_empty()
            || field.chars().any(char::is_whitespace)
            || values.contains('=')
        {
            return Err(Error::Pattern("malformed morphology field".into()));
        }
        let mut items = Vec::new();
        for val in values.split(',') {
            if val.is_empty() {
                return Err(Error::Pattern("empty morphology value".into()));
            }
            items.push(val.to_owned());
        }
        items.sort_unstable();
        if let Some((_, existing)) = raw_fields
            .iter_mut()
            .find(|(existing_key, _)| *existing_key == key)
        {
            *existing = items;
        } else {
            raw_fields.push((key, items));
        }
    }
    // Preserve first field insertion order while replacing duplicate fields,
    // before normalizing aliases such as POS. This matches Python dict updates.
    let mut fields: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (key, mut items) in raw_fields {
        let key = if is_pos_key(key) {
            if items.len() == 1 {
                let upper = items[0].to_uppercase();
                if known_pos(&upper) {
                    items[0] = upper;
                }
            }
            "POS"
        } else {
            key
        };
        fields.insert(key.to_owned(), items);
    }
    // spaCy sorts whole `Field=Value` strings, not field names: `Case2=Acc` sorts
    // before `Case=Nom` because '2' sorts before '='.
    let mut joined: Vec<String> = fields
        .into_iter()
        .map(|(key, values)| format!("{key}={}", values.join(",")))
        .collect();
    joined.sort_unstable();
    Ok(joined.join("|"))
}

/// spaCy's POS alias test, `key.upper() == "POS"`, with Unicode case mapping, so the
/// long s in `poſ` also qualifies. Compares without allocating.
fn is_pos_key(key: &str) -> bool {
    key.chars().flat_map(char::to_uppercase).eq("POS".chars())
}

fn known_pos(value: &str) -> bool {
    matches!(
        value,
        "ADJ"
            | "ADP"
            | "ADV"
            | "AUX"
            | "CONJ"
            | "CCONJ"
            | "DET"
            | "INTJ"
            | "NOUN"
            | "NUM"
            | "PART"
            | "PRON"
            | "PROPN"
            | "PUNCT"
            | "SCONJ"
            | "SYM"
            | "VERB"
            | "X"
            | "EOL"
            | "SPACE"
    )
}

// Check normal document morphology without allocating normalization tables for
// every token. Pattern-set normalization above happens only during registration.
pub(crate) fn validate_canonical_morph(value: &str) -> Result<()> {
    if value.is_empty() {
        return Ok(());
    }
    let mut previous: Option<(&str, &str)> = None;
    for field in value.split('|') {
        let Some((key, values)) = field.split_once('=') else {
            return Err(Error::Unsupported("malformed document morphology".into()));
        };
        // Canonical fields are sorted as whole `Field=Value` strings, so fields with
        // the same name are adjacent; a repeated name is not canonical.
        if key.is_empty()
            || (is_pos_key(key)
                && (key != "POS" || {
                    let upper = values.to_uppercase();
                    known_pos(&upper) && values != upper
                }))
            || previous.is_some_and(|(previous_field, previous_key)| {
                previous_field >= field || previous_key == key
            })
            || field.chars().any(char::is_whitespace)
            || values.contains('=')
        {
            return Err(Error::Unsupported(
                "matcher requires canonical morphology".into(),
            ));
        }
        previous = Some((field, key));
        let mut previous_value = None;
        for item in values.split(',') {
            if item.is_empty() || previous_value.is_some_and(|previous| previous > item) {
                return Err(Error::Unsupported(
                    "matcher requires canonical morphology values".into(),
                ));
            }
            previous_value = Some(item);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
