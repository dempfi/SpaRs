use crate::{document::NativeDocument, errors};
use napi::{
    bindgen_prelude::{Either, Null},
    Env, Result,
};
use napi_derive::napi;
use std::sync::Arc;

fn boundary(error: spars::Error, env: Env) -> napi::Error {
    errors::BindingError::from(error).into_napi(env)
}

pub(crate) fn index(value: f64, env: Env) -> Result<spars::TokenIndex> {
    if !value.is_finite() || value.fract() != 0.0 || value < 0.0 || value > u32::MAX as f64 {
        return Err(boundary(spars::Error::Bounds, env));
    }
    Ok(spars::TokenIndex(value as usize))
}

/// A token handle retaining its immutable native document.
#[napi]
pub struct NativeToken {
    pub(crate) doc: Arc<spars::Doc>,
    pub(crate) index: spars::TokenIndex,
}

/// A checked half-open token interval retaining its native document.
#[napi]
pub struct NativeSpan {
    pub(crate) doc: Arc<spars::Doc>,
    pub(crate) start: spars::TokenIndex,
    pub(crate) end: spars::TokenIndex,
}

/// Independent copies of the annotations stored on a native token.
#[napi(object, object_from_js = false, use_nullable = true)]
pub struct TokenAnnotations {
    #[napi(ts_type = "import('./units.js').ByteOffset")]
    pub byte_start: u32,
    #[napi(ts_type = "import('./units.js').ByteOffset")]
    pub byte_end: u32,
    #[napi(ts_type = "import('./units.js').CodePointOffset")]
    pub code_point_start: u32,
    #[napi(ts_type = "'' | ' '")]
    pub whitespace: String,
    pub norm: String,
    #[napi(ts_type = "FineGrainedTag | ''")]
    pub tag: Option<String>,
    #[napi(ts_type = "UniversalPos | ''")]
    pub pos: Option<String>,
    pub morphology: Option<String>,
    pub lemma: Option<String>,
    #[napi(ts_type = "import('./units.js').TokenIndex")]
    pub head: Option<u32>,
    #[napi(ts_type = "DependencyLabel | ''")]
    pub dep: Option<String>,
    pub sentence_start: Option<bool>,
    #[napi(ts_type = "EntityIob | ''")]
    pub entity_iob: Option<String>,
    #[napi(ts_type = "EntityLabel | ''")]
    pub entity_type: Option<String>,
    /// spaCy's `Token.ent_id_`, or `''` without one.
    pub entity_id: String,
}

#[napi]
impl NativeDocument {
    #[napi(getter)]
    pub fn text(&self) -> String {
        self.inner.text().to_owned()
    }

    #[napi(getter)]
    pub fn length(&self, env: Env) -> Result<u32> {
        errors::number(self.inner.tokens().len()).map_err(|error| error.into_napi(env))
    }

    #[napi(strict)]
    pub fn token(&self, env: Env, value: f64) -> Result<NativeToken> {
        let index = index(value, env)?;
        self.inner
            .token(index)
            .map_err(|error| boundary(error, env))?;
        Ok(NativeToken {
            doc: Arc::clone(&self.inner),
            index,
        })
    }

    #[napi(strict)]
    pub fn span(&self, env: Env, start: f64, end: f64) -> Result<NativeSpan> {
        let (start, end) = (index(start, env)?, index(end, env)?);
        self.inner
            .span(start, end)
            .map_err(|error| boundary(error, env))?;
        Ok(NativeSpan {
            doc: Arc::clone(&self.inner),
            start,
            end,
        })
    }

    #[napi]
    pub fn tokens(&self) -> Vec<NativeToken> {
        (0..self.inner.tokens().len())
            .map(|i| NativeToken {
                doc: Arc::clone(&self.inner),
                index: spars::TokenIndex(i),
            })
            .collect()
    }

    /// Null means sentence annotations are unavailable; an empty array means no sentences.
    #[napi]
    pub fn sentence_views(&self) -> Either<Vec<NativeSpan>, Null> {
        match self.inner.sentence_views() {
            Some(spans) => Either::A(
                spans
                    .map(|span| NativeSpan {
                        doc: Arc::clone(&self.inner),
                        start: span.start(),
                        end: span.end(),
                    })
                    .collect(),
            ),
            None => Either::B(Null),
        }
    }
}

impl NativeToken {
    fn view(&self) -> spars::TokenView<'_> {
        self.doc
            .token(self.index)
            .expect("validated immutable token")
    }
    fn handles<'a>(&self, tokens: impl Iterator<Item = spars::TokenView<'a>>) -> Vec<Self> {
        tokens
            .map(|token| Self {
                doc: Arc::clone(&self.doc),
                index: token.index(),
            })
            .collect()
    }
}

#[napi]
impl NativeToken {
    #[napi(getter, ts_return_type = "import('./units.js').TokenIndex")]
    pub fn index(&self, env: Env) -> Result<u32> {
        errors::number(self.index.0).map_err(|error| error.into_napi(env))
    }
    #[napi(getter)]
    pub fn text(&self) -> String {
        self.view().text().to_owned()
    }
    #[napi]
    pub fn annotations(&self, env: Env) -> Result<TokenAnnotations> {
        let token = self.view().annotations();
        let number = |n| errors::number(n).map_err(|error| error.into_napi(env));
        Ok(TokenAnnotations {
            byte_start: number(token.start.0)?,
            byte_end: number(token.end.0)?,
            code_point_start: number(token.idx.0)?,
            whitespace: if token.whitespace { " " } else { "" }.into(),
            norm: token.norm.clone(),
            tag: token.tag.clone(),
            pos: token.pos.clone(),
            morphology: token.morphology.clone(),
            lemma: token.lemma.clone(),
            head: token.head.map(|index| number(index.0)).transpose()?,
            dep: token.dep.clone(),
            sentence_start: token.sentence_start,
            entity_iob: token.entity_iob.clone(),
            entity_type: token.entity_type.clone(),
            entity_id: token.entity_id.clone().unwrap_or_default(),
        })
    }
    #[napi]
    pub fn head(&self) -> Either<NativeToken, Null> {
        match self.view().head() {
            Some(token) => Either::A(Self {
                doc: Arc::clone(&self.doc),
                index: token.index(),
            }),
            None => Either::B(Null),
        }
    }
    #[napi]
    pub fn span(&self) -> NativeSpan {
        let span = self.view().span();
        NativeSpan {
            doc: Arc::clone(&self.doc),
            start: span.start(),
            end: span.end(),
        }
    }
    #[napi]
    pub fn children(&self, env: Env) -> Result<Vec<NativeToken>> {
        self.view()
            .children()
            .map(|tokens| self.handles(tokens))
            .map_err(|error| boundary(error, env))
    }
    #[napi]
    pub fn ancestors(&self, env: Env) -> Result<Vec<NativeToken>> {
        self.view()
            .ancestors()
            .map(|tokens| self.handles(tokens))
            .map_err(|error| boundary(error, env))
    }
    #[napi]
    pub fn subtree(&self, env: Env) -> Result<Vec<NativeToken>> {
        self.view()
            .subtree()
            .map(|tokens| self.handles(tokens))
            .map_err(|error| boundary(error, env))
    }
    #[napi]
    pub fn sentence(&self, env: Env) -> Result<NativeSpan> {
        self.view()
            .sentence()
            .map(|span| NativeSpan {
                doc: Arc::clone(&self.doc),
                start: span.start(),
                end: span.end(),
            })
            .map_err(|error| boundary(error, env))
    }
}

#[napi]
impl NativeSpan {
    #[napi(getter, ts_return_type = "import('./units.js').TokenIndex")]
    pub fn start(&self, env: Env) -> Result<u32> {
        errors::number(self.start.0).map_err(|error| error.into_napi(env))
    }
    #[napi(getter, ts_return_type = "import('./units.js').TokenIndex")]
    pub fn end(&self, env: Env) -> Result<u32> {
        errors::number(self.end.0).map_err(|error| error.into_napi(env))
    }
    #[napi(getter)]
    pub fn text(&self) -> String {
        self.doc
            .span_text(self.start, self.end)
            .expect("validated immutable span")
            .to_owned()
    }
    #[napi]
    pub fn tokens(&self) -> Vec<NativeToken> {
        (self.start.0..self.end.0)
            .map(|i| NativeToken {
                doc: Arc::clone(&self.doc),
                index: spars::TokenIndex(i),
            })
            .collect()
    }
}
