use crate::{errors, Model};
use napi::bindgen_prelude::{BigInt, Utf16String};
use napi::{Env, Result};
use napi_derive::napi;

/// Owned lexical properties from the loaded model's pinned language resources.
#[napi(object, object_from_js = false)]
pub struct Lexeme {
    /// spaCy's string-store hash for the text (`Lexeme.orth`), as an exact unsigned 64-bit bigint.
    pub orth: BigInt,
    pub norm: String,
    pub shape: String,
    pub prefix: String,
    pub suffix: String,
    pub is_alpha: bool,
    pub is_digit: bool,
    pub is_lower: bool,
    pub is_upper: bool,
    pub is_title: bool,
    pub is_space: bool,
    pub is_ascii: bool,
    pub is_punct: bool,
    pub is_currency: bool,
    pub is_stop: bool,
    pub is_bracket: bool,
    pub is_quote: bool,
    pub is_left_punct: bool,
    pub is_right_punct: bool,
    pub like_num: bool,
    pub like_email: bool,
    pub like_url: bool,
    pub has_vector: bool,
}

impl From<spars::Lexeme> for Lexeme {
    fn from(value: spars::Lexeme) -> Self {
        Self {
            orth: BigInt {
                sign_bit: false,
                words: vec![value.orth],
            },
            norm: value.norm,
            shape: value.shape,
            prefix: value.prefix,
            suffix: value.suffix,
            is_alpha: value.is_alpha,
            is_digit: value.is_digit,
            is_lower: value.is_lower,
            is_upper: value.is_upper,
            is_title: value.is_title,
            is_space: value.is_space,
            is_ascii: value.is_ascii,
            is_punct: value.is_punct,
            is_currency: value.is_currency,
            is_stop: value.is_stop,
            is_bracket: value.is_bracket,
            is_quote: value.is_quote,
            is_left_punct: value.is_left_punct,
            is_right_punct: value.is_right_punct,
            like_num: value.like_num,
            like_email: value.like_email,
            like_url: value.like_url,
            has_vector: value.has_vector,
        }
    }
}

#[napi]
impl Model {
    /// Lexical properties of `text` as one lexeme, like spaCy `nlp.vocab[text]`, without tokenizing
    /// or running the pipeline. Throws `SPARS_INVALID_TEXT` for an unpaired surrogate.
    #[napi(strict)]
    pub fn lexeme(&self, env: Env, text: Utf16String) -> Result<Lexeme> {
        let text = errors::text(&text).map_err(|error| error.into_napi(env))?;
        self.inner
            .lexeme(&text)
            .map(Lexeme::from)
            .map_err(|error| errors::BindingError::from(error).into_napi(env))
    }
}
