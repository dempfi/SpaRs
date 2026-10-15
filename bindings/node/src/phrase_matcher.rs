//! Reusable exact phrase matching over immutable native documents.
use crate::{document::NativeDocument, errors};
use napi::{
    bindgen_prelude::{AsyncTask, ClassInstance, Utf16String},
    Env, Result, Task,
};
use napi_derive::napi;
use std::sync::Arc;

#[napi(object)]
pub struct PhraseMatch {
    pub rule: String,
    #[napi(ts_type = "import('./units.js').TokenIndex")]
    pub start: u32,
    #[napi(ts_type = "import('./units.js').TokenIndex")]
    pub end: u32,
}

/// Attribute names as spaCy spells them, in both directions.
const ATTRIBUTES: [(&str, spars::PhraseAttribute); 27] = [
    ("ORTH", spars::PhraseAttribute::Orth),
    ("TEXT", spars::PhraseAttribute::Text),
    ("LOWER", spars::PhraseAttribute::Lower),
    ("NORM", spars::PhraseAttribute::Norm),
    ("LEMMA", spars::PhraseAttribute::Lemma),
    ("POS", spars::PhraseAttribute::Pos),
    ("TAG", spars::PhraseAttribute::Tag),
    ("DEP", spars::PhraseAttribute::Dep),
    ("MORPH", spars::PhraseAttribute::Morph),
    ("IS_ALPHA", spars::PhraseAttribute::IsAlpha),
    ("IS_ASCII", spars::PhraseAttribute::IsAscii),
    ("IS_DIGIT", spars::PhraseAttribute::IsDigit),
    ("IS_LOWER", spars::PhraseAttribute::IsLower),
    ("IS_UPPER", spars::PhraseAttribute::IsUpper),
    ("IS_TITLE", spars::PhraseAttribute::IsTitle),
    ("IS_PUNCT", spars::PhraseAttribute::IsPunct),
    ("IS_SPACE", spars::PhraseAttribute::IsSpace),
    ("IS_BRACKET", spars::PhraseAttribute::IsBracket),
    ("IS_QUOTE", spars::PhraseAttribute::IsQuote),
    ("IS_LEFT_PUNCT", spars::PhraseAttribute::IsLeftPunct),
    ("IS_RIGHT_PUNCT", spars::PhraseAttribute::IsRightPunct),
    ("IS_CURRENCY", spars::PhraseAttribute::IsCurrency),
    ("IS_STOP", spars::PhraseAttribute::IsStop),
    ("LIKE_NUM", spars::PhraseAttribute::LikeNum),
    ("LIKE_URL", spars::PhraseAttribute::LikeUrl),
    ("LIKE_EMAIL", spars::PhraseAttribute::LikeEmail),
    ("LENGTH", spars::PhraseAttribute::Length),
];

/// spaCy's spelling of an attribute. Exhaustive, so a new attribute must be named here;
/// [`attribute`] accepts the same names.
pub(crate) fn attribute_name(attribute: spars::PhraseAttribute) -> &'static str {
    match attribute {
        spars::PhraseAttribute::Orth => "ORTH",
        spars::PhraseAttribute::Text => "TEXT",
        spars::PhraseAttribute::Lower => "LOWER",
        spars::PhraseAttribute::Norm => "NORM",
        spars::PhraseAttribute::Lemma => "LEMMA",
        spars::PhraseAttribute::Pos => "POS",
        spars::PhraseAttribute::Tag => "TAG",
        spars::PhraseAttribute::Dep => "DEP",
        spars::PhraseAttribute::Morph => "MORPH",
        spars::PhraseAttribute::IsAlpha => "IS_ALPHA",
        spars::PhraseAttribute::IsAscii => "IS_ASCII",
        spars::PhraseAttribute::IsDigit => "IS_DIGIT",
        spars::PhraseAttribute::IsLower => "IS_LOWER",
        spars::PhraseAttribute::IsUpper => "IS_UPPER",
        spars::PhraseAttribute::IsTitle => "IS_TITLE",
        spars::PhraseAttribute::IsPunct => "IS_PUNCT",
        spars::PhraseAttribute::IsSpace => "IS_SPACE",
        spars::PhraseAttribute::IsBracket => "IS_BRACKET",
        spars::PhraseAttribute::IsQuote => "IS_QUOTE",
        spars::PhraseAttribute::IsLeftPunct => "IS_LEFT_PUNCT",
        spars::PhraseAttribute::IsRightPunct => "IS_RIGHT_PUNCT",
        spars::PhraseAttribute::IsCurrency => "IS_CURRENCY",
        spars::PhraseAttribute::IsStop => "IS_STOP",
        spars::PhraseAttribute::LikeNum => "LIKE_NUM",
        spars::PhraseAttribute::LikeUrl => "LIKE_URL",
        spars::PhraseAttribute::LikeEmail => "LIKE_EMAIL",
        spars::PhraseAttribute::Length => "LENGTH",
    }
}

/// Parse spaCy's spelling of a phrase attribute, defaulting to `ORTH`.
pub(crate) fn attribute(name: Option<&str>) -> errors::Result<spars::PhraseAttribute> {
    let name = name.unwrap_or("ORTH");
    ATTRIBUTES
        .iter()
        .find(|(known, _)| *known == name)
        .map(|&(_, attribute)| attribute)
        .ok_or_else(|| {
            spars::Error::Unsupported(format!(
                "phrase matching does not support the attribute {name:?}"
            ))
            .into()
        })
}

#[napi]
pub struct PhraseMatcher {
    inner: Arc<spars::PhraseMatcher>,
}

#[napi]
impl PhraseMatcher {
    /// Match exact token text using ORTH (default) or TEXT, pinned Unicode LOWER, NORM, the
    /// LEMMA, POS, TAG, DEP and MORPH annotations, a lexical flag such as IS_ALPHA, or LENGTH.
    /// Pass a model to enable lexical flag attributes with its language rules.
    #[napi(
        constructor,
        ts_args_type = "attribute?: PhraseAttribute | undefined | null, model?: Model | undefined | null"
    )]
    pub fn new(
        env: Env,
        attribute: Option<Utf16String>,
        model: Option<&crate::Model>,
    ) -> Result<Self> {
        let attribute = attribute
            .as_ref()
            .map(|value| errors::text(value))
            .transpose()
            .map_err(|error| error.into_napi(env))?;
        let attribute =
            self::attribute(attribute.as_deref()).map_err(|error| error.into_napi(env))?;
        let matcher = match model {
            Some(model) => spars::PhraseMatcher::with_lexicon(attribute, model.inner.lexicon()),
            None => spars::PhraseMatcher::with_attribute(attribute),
        };
        Ok(Self {
            inner: Arc::new(matcher),
        })
    }

    #[napi(getter, ts_return_type = "PhraseAttribute")]
    pub fn attribute(&self) -> &'static str {
        attribute_name(self.inner.attribute())
    }

    #[napi(getter)]
    pub fn size(&self, env: Env) -> Result<u32> {
        errors::number(self.inner.len()).map_err(|error| error.into_napi(env))
    }

    #[napi(strict)]
    pub fn contains(&self, env: Env, rule: Utf16String) -> Result<bool> {
        let rule = errors::text(&rule).map_err(|error| error.into_napi(env))?;
        Ok(self.inner.contains(&rule))
    }

    /// Return owned unique nonempty patterns of compared token values in registration order.
    #[napi(strict)]
    pub fn get(&self, env: Env, rule: Utf16String) -> Result<Option<Vec<Vec<String>>>> {
        let rule = errors::text(&rule).map_err(|error| error.into_napi(env))?;
        Ok(self.inner.get(&rule).map(|patterns| {
            patterns
                .iter()
                .map(|pattern| pattern.tokens().to_vec())
                .collect()
        }))
    }

    /// Copy the compared pattern token values into the trie. Pending searches prevent mutation.
    #[napi(strict)]
    pub fn add(
        &mut self,
        env: Env,
        rule: Utf16String,
        patterns: Vec<ClassInstance<'_, NativeDocument>>,
    ) -> Result<()> {
        let rule = errors::text(&rule).map_err(|error| error.into_napi(env))?;
        let matcher = Arc::get_mut(&mut self.inner).ok_or_else(|| errors::busy().into_napi(env))?;
        let docs: Vec<&spars::Doc> = patterns
            .iter()
            .map(|pattern| pattern.inner.as_ref())
            .collect();
        matcher
            .add(rule, &docs)
            .map_err(|error| errors::registration(error).into_napi(env))
    }

    #[napi(strict)]
    pub fn remove(&mut self, env: Env, rule: Utf16String) -> Result<()> {
        let rule = errors::text(&rule).map_err(|error| error.into_napi(env))?;
        let matcher = Arc::get_mut(&mut self.inner).ok_or_else(|| errors::busy().into_napi(env))?;
        matcher
            .remove(&rule)
            .map_err(|error| errors::BindingError::from(error).into_napi(env))
    }

    /// Return all overlapping matches in native reference order on Node's worker pool.
    #[napi(strict, ts_return_type = "Promise<Array<PhraseMatch>>")]
    pub fn find_matches(&self, doc: &NativeDocument) -> AsyncTask<PhraseMatchTask> {
        AsyncTask::new(PhraseMatchTask {
            matcher: Arc::clone(&self.inner),
            doc: Arc::clone(&doc.inner),
        })
    }
}

pub struct PhraseMatchTask {
    matcher: Arc<spars::PhraseMatcher>,
    doc: Arc<spars::Doc>,
}

#[napi]
impl Task for PhraseMatchTask {
    type Output = errors::Result<Vec<PhraseMatch>>;
    type JsValue = Vec<PhraseMatch>;

    fn compute(&mut self) -> Result<Self::Output> {
        Ok(self
            .matcher
            .find_matches(&self.doc)
            .map_err(Into::into)
            .and_then(|matches| {
                matches
                    .into_iter()
                    .map(|matched| {
                        Ok(PhraseMatch {
                            rule: matched.rule.0,
                            start: errors::number(matched.start.0)?,
                            end: errors::number(matched.end.0)?,
                        })
                    })
                    .collect()
            }))
    }

    fn resolve(&mut self, env: Env, output: Self::Output) -> Result<Self::JsValue> {
        output.map_err(|error| error.into_napi(env))
    }
}
