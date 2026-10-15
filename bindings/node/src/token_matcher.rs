use crate::{
    document::NativeDocument,
    errors,
    matcher_types::{invalid, TokenConstraint},
};
use napi::{
    bindgen_prelude::{AsyncTask, Utf16String},
    Env, Result, Task,
};
use napi_derive::napi;
use std::sync::Arc;

#[napi(object)]
pub struct TokenRepetition {
    pub kind: Utf16String,
    pub min: Option<f64>,
    pub max: Option<f64>,
}
#[napi(object)]
pub struct TokenPatternItem {
    pub constraints: Vec<TokenConstraint>,
    pub repetition: TokenRepetition,
}
#[napi(object)]
pub struct TokenPattern {
    pub tokens: Vec<TokenPatternItem>,
}
#[napi(object)]
pub struct TokenMatch {
    pub rule: String,
    #[napi(ts_type = "import('./units.js').TokenIndex")]
    pub start: u32,
    #[napi(ts_type = "import('./units.js').TokenIndex")]
    pub end: u32,
}

fn bound(value: f64) -> errors::Result<usize> {
    // Native patterns expand to at most 4096 nodes. The larger u32 boundary
    // accepts every potentially executable pattern without integer truncation.
    if !value.is_finite() || value.fract() != 0.0 || value < 0.0 || value > f64::from(u32::MAX) {
        return Err(invalid(
            "repetition bounds must be integers in 0..=4294967295",
        ));
    }
    Ok(value as usize)
}
impl TokenPattern {
    pub(crate) fn into_native(self) -> errors::Result<spars::TokenPattern> {
        Ok(spars::TokenPattern {
            tokens: self
                .tokens
                .into_iter()
                .map(|item| {
                    let kind = errors::text(&item.repetition.kind)?;
                    let repetition = if kind == "range" {
                        spars::Repetition::Range {
                            min: bound(
                                item.repetition
                                    .min
                                    .ok_or_else(|| invalid("range repetition requires min"))?,
                            )?,
                            max: item.repetition.max.map(bound).transpose()?,
                        }
                    } else {
                        if item.repetition.min.is_some() || item.repetition.max.is_some() {
                            return Err(invalid("only range repetition accepts min or max"));
                        }
                        match kind.as_str() {
                            "once" => spars::Repetition::Once,
                            "optional" => spars::Repetition::Optional,
                            "zero_or_more" => spars::Repetition::ZeroOrMore,
                            "one_or_more" => spars::Repetition::OneOrMore,
                            "negated" => spars::Repetition::Negated,
                            _ => return Err(invalid("unknown repetition kind")),
                        }
                    };
                    Ok(spars::TokenPatternItem {
                        constraints: item
                            .constraints
                            .into_iter()
                            .map(TokenConstraint::into_native)
                            .collect::<errors::Result<_>>()?,
                        repetition,
                    })
                })
                .collect::<errors::Result<_>>()?,
        })
    }
    pub(crate) fn from_native(pattern: &spars::TokenPattern) -> Self {
        Self {
            tokens: pattern
                .tokens
                .iter()
                .map(|item| {
                    let (kind, min, max) = match item.repetition {
                        spars::Repetition::Once => ("once", None, None),
                        spars::Repetition::Optional => ("optional", None, None),
                        spars::Repetition::ZeroOrMore => ("zero_or_more", None, None),
                        spars::Repetition::OneOrMore => ("one_or_more", None, None),
                        spars::Repetition::Negated => ("negated", None, None),
                        spars::Repetition::Range { min, max } => {
                            ("range", Some(min as f64), max.map(|value| value as f64))
                        }
                    };
                    TokenPatternItem {
                        constraints: item
                            .constraints
                            .iter()
                            .map(TokenConstraint::from_native)
                            .collect(),
                        repetition: TokenRepetition {
                            kind: kind.to_owned().into(),
                            min,
                            max,
                        },
                    }
                })
                .collect(),
        }
    }
}

#[napi]
pub struct TokenMatcher {
    inner: Arc<spars::TokenMatcher>,
}
#[napi]
impl TokenMatcher {
    /// Pass a model to enable lexical flag conditions with its language rules.
    #[napi(constructor)]
    pub fn new(model: Option<&crate::Model>) -> Self {
        let matcher = match model {
            Some(model) => spars::TokenMatcher::with_lexicon(model.inner.lexicon()),
            None => spars::TokenMatcher::new(),
        };
        Self {
            inner: Arc::new(matcher),
        }
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
    #[napi(strict)]
    pub fn get(&self, env: Env, rule: Utf16String) -> Result<Option<Vec<TokenPattern>>> {
        let rule = errors::text(&rule).map_err(|error| error.into_napi(env))?;
        Ok(self
            .inner
            .get(&rule)
            .map(|patterns| patterns.iter().map(TokenPattern::from_native).collect()))
    }
    #[napi(strict)]
    pub fn add(&mut self, env: Env, rule: Utf16String, patterns: Vec<TokenPattern>) -> Result<()> {
        let rule = errors::text(&rule).map_err(|error| error.into_napi(env))?;
        let matcher = Arc::get_mut(&mut self.inner).ok_or_else(|| errors::busy().into_napi(env))?;
        let patterns = patterns
            .into_iter()
            .map(TokenPattern::into_native)
            .collect::<errors::Result<_>>()
            .map_err(|error| error.into_napi(env))?;
        matcher
            .add(rule, patterns)
            .map_err(|error| errors::BindingError::from(error).into_napi(env))
    }
    #[napi(strict)]
    pub fn remove(&mut self, env: Env, rule: Utf16String) -> Result<()> {
        let rule = errors::text(&rule).map_err(|error| error.into_napi(env))?;
        let matcher = Arc::get_mut(&mut self.inner).ok_or_else(|| errors::busy().into_napi(env))?;
        matcher
            .remove(&rule)
            .map_err(|error| errors::BindingError::from(error).into_napi(env))
    }
    #[napi(strict, ts_return_type = "Promise<Array<TokenMatch>>")]
    pub fn find_matches(&self, doc: &NativeDocument) -> AsyncTask<TokenMatchTask> {
        AsyncTask::new(TokenMatchTask {
            matcher: Arc::clone(&self.inner),
            doc: Arc::clone(&doc.inner),
        })
    }
}
impl Default for TokenMatcher {
    fn default() -> Self {
        Self::new(None)
    }
}
pub struct TokenMatchTask {
    matcher: Arc<spars::TokenMatcher>,
    doc: Arc<spars::Doc>,
}
#[napi]
impl Task for TokenMatchTask {
    type Output = errors::Result<Vec<TokenMatch>>;
    type JsValue = Vec<TokenMatch>;
    fn compute(&mut self) -> Result<Self::Output> {
        Ok(self
            .matcher
            .find_matches(&self.doc)
            .map_err(Into::into)
            .and_then(|matches| {
                matches
                    .into_iter()
                    .map(|matched| {
                        Ok(TokenMatch {
                            rule: matched.rule,
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
