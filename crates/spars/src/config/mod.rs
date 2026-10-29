use serde::Deserialize;
use std::{collections::HashMap, sync::Arc};
mod capabilities;
mod processing;
mod resources;
pub(crate) use capabilities::Capabilities;
pub(crate) use processing::ProcessingDefaults;
pub(crate) use resources::*;
#[derive(Debug, Clone, Deserialize)]
#[serde(transparent)]
pub(crate) struct TensorRef(pub String);
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TensorSpec {
    pub shape: Vec<usize>,
    pub dtype: Dtype,
}
#[derive(Deserialize, PartialEq)]
pub(crate) enum Dtype {
    F32,
}
#[derive(Deserialize)]
pub(crate) struct Manifest {
    #[serde(default, rename = "config")]
    pub processing: ProcessingDefaults,
    pub format_version: u32,
    pub model: String,
    pub model_version: String,
    pub weights_sha256: String,
    pub versions: HashMap<String, String>,
    pub capabilities: Option<Capabilities>,
    pub tensors: HashMap<String, TensorSpec>,
    pub tokenizer: TokenizerConfig,
    #[serde(default)]
    pub vector_keys: HashMap<u64, usize>,
    pub norms: HashMap<u64, String>,
    pub symbols: HashMap<String, u64>,
    #[serde(deserialize_with = "shared")]
    pub lexical: Arc<Lexical>,
    pub tok2vec: Encoder,
    pub tagger: Tagger,
    pub parser: Transition,
    pub ner: Ner,
    pub pipeline: Vec<Component>,
    pub attribute_rules: Vec<AttributeRule>,
    pub lemmas: Lemmas,
}
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Component {
    Tok2vec,
    Tagger,
    Parser,
    AttributeRuler,
    Lemmatizer,
    Ner,
}
#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum Feature {
    Norm,
    Prefix,
    Suffix,
    Shape,
    Spacy,
    IsSpace,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Linear {
    #[serde(rename = "W")]
    pub w: TensorRef,
    pub b: TensorRef,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Projection {
    #[serde(rename = "W")]
    pub w: TensorRef,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Norm {
    #[serde(rename = "G")]
    pub g: TensorRef,
    pub b: TensorRef,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Block {
    pub maxout: Linear,
    pub norm: Norm,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Embedding {
    #[serde(rename = "E")]
    pub e: TensorRef,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HashEmbedding {
    pub params: Embedding,
    pub seed: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Encoder {
    pub width: usize,
    pub attrs: Vec<Feature>,
    pub hashes: Vec<HashEmbedding>,
    #[serde(rename = "static")]
    pub static_vectors: Option<Projection>,
    pub mix: Block,
    pub layers: Vec<Block>,
    pub windows: Vec<usize>,
    pub pad: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Tagger {
    pub labels: Vec<String>,
    pub params: Linear,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Lower {
    #[serde(rename = "W")]
    pub w: TensorRef,
    pub b: TensorRef,
    pub pad: TensorRef,
}
#[derive(Deserialize)]
pub(crate) struct Transition {
    pub lower: Lower,
    pub reduce: Linear,
    pub upper: Linear,
    pub actions: Vec<Action>,
}
#[derive(Deserialize)]
pub(crate) struct Ner {
    #[serde(flatten)]
    pub transition: Transition,
    pub tok2vec: Encoder,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActionKind {
    Shift,
    Reduce,
    Left,
    Right,
    Begin,
    Inside,
    Unit,
    Outside,
}
#[derive(Debug)]
pub(crate) struct Action {
    pub kind: ActionKind,
    pub label: String,
}
impl<'de> Deserialize<'de> for Action {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        let (kind, label) = s.split_once('-').unwrap_or((&s, ""));
        let kind = match kind {
            "S" => ActionKind::Shift,
            "D" => ActionKind::Reduce,
            "L" => ActionKind::Left,
            "R" => ActionKind::Right,
            "B" => ActionKind::Begin,
            "I" => ActionKind::Inside,
            "U" => ActionKind::Unit,
            "O" => ActionKind::Outside,
            _ => {
                return Err(serde::de::Error::custom(format!(
                    "unknown transition action {s}"
                )))
            }
        };
        Ok(Self {
            kind,
            label: label.into(),
        })
    }
}

// Lexical resources are shared with matchers through `Lexicon` without copying.
fn shared<'de, D, T>(d: D) -> Result<Arc<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(d).map(Arc::new)
}

#[cfg(test)]
mod tests;
