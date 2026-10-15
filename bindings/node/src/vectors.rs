use crate::views::index;
use crate::{document::NativeDocument, errors, Model};
use napi::bindgen_prelude::{Either, Float32Array, Null};
use napi::{Env, Result};
use napi_derive::napi;

#[napi]
impl Model {
    /// Mean of the document's token vectors, as a copy, like spaCy `Doc.vector`. Uses this model's
    /// static vectors, where tokens without a vector count as zeros. A model without static vectors,
    /// such as en_core_web_sm, uses the document's contextual vectors instead. The result is empty
    /// only when the model has no static vectors and the document has no contextual vectors, for
    /// example after processing only through `Tokenizer`.
    #[napi(strict)]
    pub fn document_vector(&self, document: &NativeDocument) -> Float32Array {
        Float32Array::new(self.inner.document_vector(&document.inner))
    }

    /// Copy of the token's vector: a static vector looked up by exact text, or the contextual vector
    /// for a model without static vectors. Null when none exists (spaCy returns zeros instead).
    /// Throws `SPARS_BOUNDS` for an invalid index.
    #[napi(strict)]
    pub fn token_vector(
        &self,
        env: Env,
        document: &NativeDocument,
        token: f64,
    ) -> Result<Either<Float32Array, Null>> {
        let token = document
            .inner
            .token(index(token, env)?)
            .map_err(|error| errors::BindingError::from(error).into_napi(env))?;
        Ok(match self.inner.token_vector(token) {
            Some(vector) => Either::A(Float32Array::new(vector.to_vec())),
            None => Either::B(Null),
        })
    }

    /// Mean vector of tokens `start` (inclusive) to `end` (exclusive), as a copy, chosen like
    /// `documentVector`. The result is empty when the model has no static vectors and either the
    /// document has no contextual vectors or the interval is empty; with static vectors an empty
    /// interval gives a zero vector. Throws `SPARS_BOUNDS` unless `0 <= start <= end <= doc.length`.
    #[napi(strict)]
    pub fn span_vector(
        &self,
        env: Env,
        document: &NativeDocument,
        start: f64,
        end: f64,
    ) -> Result<Float32Array> {
        self.inner
            .span_vector(&document.inner, index(start, env)?, index(end, env)?)
            .map(Float32Array::new)
            .map_err(|error| errors::BindingError::from(error).into_napi(env))
    }

    /// Cosine similarity of the two document vectors, like spaCy `Doc.similarity`. Returns 1 when both
    /// have the same token texts in order, and 0 when either vector is empty or all zeros. Throws
    /// `SPARS_UNSUPPORTED` when both vectors are nonzero with different widths.
    #[napi(strict)]
    pub fn similarity(&self, env: Env, a: &NativeDocument, b: &NativeDocument) -> Result<f64> {
        check_dimensions(
            &self.inner,
            (
                &a.inner,
                spars::TokenIndex(0),
                spars::TokenIndex(a.inner.tokens().len()),
            ),
            (
                &b.inner,
                spars::TokenIndex(0),
                spars::TokenIndex(b.inner.tokens().len()),
            ),
        )
        .map_err(|error| error.into_napi(env))?;
        Ok(f64::from(self.inner.similarity(&a.inner, &b.inner)))
    }

    /// Like `similarity`, for tokens `aStart` to `aEnd` of `a` and `bStart` to `bEnd` of `b`, with
    /// exclusive ends. Throws `SPARS_BOUNDS` for an invalid interval.
    #[napi(strict)]
    #[allow(clippy::too_many_arguments)]
    pub fn span_similarity(
        &self,
        env: Env,
        a: &NativeDocument,
        a_start: f64,
        a_end: f64,
        b: &NativeDocument,
        b_start: f64,
        b_end: f64,
    ) -> Result<f64> {
        let a_start = index(a_start, env)?;
        let a_end = index(a_end, env)?;
        let b_start = index(b_start, env)?;
        let b_end = index(b_end, env)?;
        check_dimensions(
            &self.inner,
            (&a.inner, a_start, a_end),
            (&b.inner, b_start, b_end),
        )
        .map_err(|error| error.into_napi(env))?;
        let a = a
            .inner
            .span(a_start, a_end)
            .map_err(|error| errors::BindingError::from(error).into_napi(env))?;
        let b = b
            .inner
            .span(b_start, b_end)
            .map_err(|error| errors::BindingError::from(error).into_napi(env))?;
        Ok(f64::from(self.inner.span_similarity(a, b)))
    }
}

type Interval<'a> = (&'a spars::Doc, spars::TokenIndex, spars::TokenIndex);

// Official similarity short-circuits equal token sequences and zero norms
// before its dot product checks dimensions. Ordinary equal-width documents
// take the constant-time check; only incompatible candidates compute means here.
fn check_dimensions(model: &spars::Model, a: Interval<'_>, b: Interval<'_>) -> errors::Result<()> {
    let left = a.0.span(a.1, a.2)?;
    let right = b.0.span(b.1, b.2)?;
    let width = |span: spars::SpanView<'_>| {
        span.tokens()
            .next()
            .and_then(|token| model.token_vector(token))
            .map_or(0, <[f32]>::len)
    };
    if width(left) == width(right)
        || (a.2 .0 - a.1 .0 == b.2 .0 - b.1 .0
            && left
                .tokens()
                .zip(right.tokens())
                .all(|(a, b)| a.text() == b.text()))
    {
        return Ok(());
    }
    let left = model.span_vector(a.0, a.1, a.2)?;
    let right = model.span_vector(b.0, b.1, b.2)?;
    if left.len() != right.len()
        && left.iter().map(|x| x * x).sum::<f32>() > 0.
        && right.iter().map(|x| x * x).sum::<f32>() > 0.
    {
        return Err(spars::Error::Unsupported(
            "similarity requires equal nonzero vector dimensions".into(),
        )
        .into());
    }
    Ok(())
}
