use serde::Deserialize;
use std::collections::{HashMap, HashSet};
#[derive(Deserialize)]
pub(crate) struct TokenizerConfig {
    pub prefix: String,
    pub suffix: String,
    pub infix: String,
    pub url: String,
    pub token_match: Option<String>,
    #[serde(default, deserialize_with = "present")]
    pub faster_heuristics: Option<bool>,
    pub regex_dialect: String,
    pub rules: HashMap<String, Vec<Exception>>,
    #[serde(default)]
    pub matcher_patterns: Option<HashMap<String, Vec<Vec<String>>>>,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Exception {
    #[serde(rename = "ORTH")]
    pub orth: String,
    #[serde(rename = "NORM")]
    pub norm: Option<String>,
}
#[derive(Deserialize)]
pub(crate) struct Lexical {
    pub email_regex: String,
    pub lower: HashMap<String, String>,
    pub ranges: Ranges,
    pub stops: Vec<String>,
    pub number_words: Vec<String>,
    pub tlds: Vec<String>,
    pub is_bracket: Vec<String>,
    pub is_quote: Vec<String>,
    pub is_left_punct: Vec<String>,
    pub is_right_punct: Vec<String>,
}
#[derive(Clone, Copy)]
pub(crate) enum CharFlag {
    Alpha,
    Digit,
    Lower,
    Upper,
    Title,
    Punct,
    Currency,
    CaseIgnorable,
}
#[derive(Deserialize)]
pub(crate) struct Ranges {
    pub alpha: Vec<[u32; 2]>,
    pub digit: Vec<[u32; 2]>,
    pub lower: Vec<[u32; 2]>,
    pub upper: Vec<[u32; 2]>,
    pub title: Vec<[u32; 2]>,
    pub punct: Vec<[u32; 2]>,
    pub currency: Vec<[u32; 2]>,
    pub case_ignorable: Vec<[u32; 2]>,
}
impl Ranges {
    pub fn get(&self, f: CharFlag) -> &[[u32; 2]] {
        match f {
            CharFlag::Alpha => &self.alpha,
            CharFlag::Digit => &self.digit,
            CharFlag::Lower => &self.lower,
            CharFlag::Upper => &self.upper,
            CharFlag::Title => &self.title,
            CharFlag::Punct => &self.punct,
            CharFlag::Currency => &self.currency,
            CharFlag::CaseIgnorable => &self.case_ignorable,
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AttributeRule {
    pub index: i64,
    pub patterns: Vec<Vec<TokenPattern>>,
    pub attrs: HashMap<OutputAttribute, String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) struct TokenPattern {
    #[serde(default, deserialize_with = "present")]
    pub lower: Option<Constraint>,
    #[serde(default, deserialize_with = "present")]
    pub tag: Option<Constraint>,
    #[serde(default, deserialize_with = "present")]
    pub dep: Option<Constraint>,
    #[serde(default, deserialize_with = "present")]
    pub is_space: Option<bool>,
}
pub(crate) enum Constraint {
    Exact(String),
    Operators(Operators),
}
impl<'de> Deserialize<'de> for Constraint {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct ConstraintVisitor;
        impl<'de> serde::de::Visitor<'de> for ConstraintVisitor {
            type Value = Constraint;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("an exact string or attribute operator map")
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Constraint, E> {
                Ok(Constraint::Exact(value.to_owned()))
            }

            fn visit_string<E: serde::de::Error>(self, value: String) -> Result<Constraint, E> {
                Ok(Constraint::Exact(value))
            }

            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                map: M,
            ) -> Result<Constraint, M::Error> {
                Operators::deserialize(serde::de::value::MapAccessDeserializer::new(map))
                    .map(Constraint::Operators)
            }
        }
        d.deserialize_any(ConstraintVisitor)
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) struct Operators {
    #[serde(rename = "IN")]
    #[serde(default, deserialize_with = "present")]
    pub included: Option<Vec<String>>,
    #[serde(default, deserialize_with = "present")]
    pub not_in: Option<Vec<String>>,
    #[serde(default, deserialize_with = "compiled_regex")]
    pub regex: Option<fancy_regex::Regex>,
}
#[derive(Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum OutputAttribute {
    Pos,
    Tag,
    Morph,
    Lemma,
    Dep,
}
#[derive(Deserialize)]
pub(crate) struct Lemmas {
    #[serde(deserialize_with = "membership_tables")]
    pub lemma_index: HashMap<String, HashSet<String>>,
    pub lemma_exc: HashMap<String, HashMap<String, Vec<String>>>,
    #[serde(deserialize_with = "tables")]
    pub lemma_rules: HashMap<String, Vec<[String; 2]>>,
}
// Only membership is observed for the index; rule and exception order stays intact.
fn membership_tables<'de, D>(d: D) -> Result<HashMap<String, HashSet<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    tables::<D, String>(d).map(|tables| {
        tables
            .into_iter()
            .map(|(pos, words)| (pos, words.into_iter().collect()))
            .collect()
    })
}

fn tables<'de, D, T>(d: D) -> Result<HashMap<String, Vec<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Table<T> {
        Items(Vec<T>),
        Empty(HashMap<String, serde::de::IgnoredAny>),
    }
    let raw = HashMap::<String, Table<T>>::deserialize(d)?;
    raw.into_iter()
        .map(|(k, v)| {
            Ok((
                k,
                match v {
                    Table::Items(v) => v,
                    Table::Empty(v) if v.is_empty() => Vec::new(),
                    Table::Empty(_) => {
                        return Err(serde::de::Error::custom("expected list or empty table"))
                    }
                },
            ))
        })
        .collect()
}

// A missing optional field is different from an explicitly null field.
fn present<'de, D, T>(d: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(d).map(Some)
}

fn compiled_regex<'de, D>(d: D) -> Result<Option<fancy_regex::Regex>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let pattern = String::deserialize(d)?;
    fancy_regex::Regex::new(&pattern)
        .map(Some)
        .map_err(serde::de::Error::custom)
}
