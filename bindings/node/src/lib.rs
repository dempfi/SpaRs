//! Native Node-API boundary for SpaRs.
mod convert;
mod dependency_matcher;
mod document;
mod entity_ruler;
mod errors;
mod lexical;
mod matcher_types;
mod model_store;
mod models;
mod phrase_matcher;
mod token_matcher;
mod vectors;
mod views;
pub use models::{download_model, DownloadOptions, LoadOptions};
mod output;
mod tasks;

use napi::bindgen_prelude::{AsyncTask, Either, Float32Array, Null, Utf16String};
use napi::{Env, Result};
use napi_derive::napi;
use std::sync::Arc;
use tasks::{BatchTask, LoadTask, ProcessDocumentTask, ProcessTask};

/// The last pipeline component to run, inclusive. Earlier components in the model's order also run,
/// and annotations from later ones stay null. `Tokenizer` runs no component. Processing throws
/// `SPARS_UNSUPPORTED` when the model's pipeline lacks the component.
#[napi(string_enum)]
pub enum Stage {
    Tokenizer,
    Tagger,
    Parser,
    AttributeRuler,
    Lemmatizer,
    Ner,
}

impl From<Stage> for spars::Stage {
    fn from(value: Stage) -> Self {
        match value {
            Stage::Tokenizer => Self::Tokenizer,
            Stage::Tagger => Self::Tagger,
            Stage::Parser => Self::Parser,
            Stage::AttributeRuler => Self::AttributeRuler,
            Stage::Lemmatizer => Self::Lemmatizer,
            Stage::Ner => Self::Ner,
        }
    }
}

/// A reusable model with immutable native weights and configurable processing settings. Create it with loadModel.
#[napi]
pub struct Model {
    inner: Arc<spars::Model>,
    max_length: u32,
    batch_size: u32,
}

#[napi]
impl Model {
    /// Maximum Unicode code points per document, following spaCy max_length (default 1,000,000).
    #[napi(getter)]
    pub fn max_length(&self) -> u32 {
        self.max_length
    }

    /// Longer texts reject with `SPARS_TEXT_TOO_LONG`. Set an integer from 0 to 4294967295, only
    /// while no inference is running.
    #[napi(setter)]
    pub fn set_max_length(&mut self, value: f64) -> Result<()> {
        self.max_length = setting(value, 0, "maxLength")?;
        Ok(())
    }

    /// Number of documents buffered by `pipe` and passed to each native `processBatch` chunk.
    /// Defaults to the model's configured `batch_size`.
    #[napi(getter)]
    pub fn batch_size(&self) -> u32 {
        self.batch_size
    }

    /// Set a positive integer, only while no inference is running.
    #[napi(setter)]
    pub fn set_batch_size(&mut self, value: f64) -> Result<()> {
        self.batch_size = setting(value, 1, "batchSize")?;
        Ok(())
    }

    /// Process `text` on Node's worker pool and return a plain `Document`. Omitting `stage` runs
    /// through `Ner`. Rejects with `SPARS_TEXT_TOO_LONG` when the text has more than `maxLength` code
    /// points, `SPARS_INVALID_TEXT` for an unpaired surrogate, and `SPARS_BUSY` or `SPARS_INPUT_LIMIT`
    /// under `configureExecution` and `configureInputLimits`. Throws `TypeError` for a non-string
    /// text or an unknown stage.
    #[napi(strict)]
    pub fn process(&self, text: Utf16String, stage: Option<Stage>) -> AsyncTask<ProcessTask> {
        AsyncTask::new(ProcessTask {
            model: Arc::clone(&self.inner),
            max_length: self.max_length,
            text,
            stage: stage.unwrap_or(Stage::Ner).into(),
        })
    }

    /// Like `process`, but returns a `NativeDocument` for matchers, token and span views, vectors,
    /// `withEntities` and snapshots.
    #[napi(strict)]
    pub fn process_document(
        &self,
        text: Utf16String,
        stage: Option<Stage>,
    ) -> AsyncTask<ProcessDocumentTask> {
        AsyncTask::new(ProcessDocumentTask {
            model: Arc::clone(&self.inner),
            max_length: self.max_length,
            text,
            stage: stage.unwrap_or(Stage::Ner).into(),
        })
    }

    /// Process several texts and return documents in input order. Texts are processed in chunks of
    /// `batchSize`; one failing text rejects the whole call. `configureInputLimits` checks
    /// `maxBatchSize` and `maxBatchTextLength` before any work starts.
    #[napi(strict)]
    pub fn process_batch(
        &self,
        texts: Vec<Utf16String>,
        stage: Option<Stage>,
    ) -> AsyncTask<BatchTask> {
        AsyncTask::new(BatchTask {
            model: Arc::clone(&self.inner),
            max_length: self.max_length,
            texts,
            stage: stage.unwrap_or(Stage::Ner).into(),
        })
    }

    /// Copy of the static vector for `word`, looked up by exact text, or null when no row exists.
    /// Always null for models without static vectors, such as en_core_web_sm.
    #[napi(strict)]
    pub fn vector(&self, env: Env, word: Utf16String) -> Result<Either<Float32Array, Null>> {
        let word = errors::text(&word).map_err(|error| error.into_napi(env))?;
        Ok(match self.inner.vector(&word) {
            Some(vector) => Either::A(Float32Array::new(vector.to_vec())),
            None => Either::B(Null),
        })
    }
}

/// Load an installed model on Node's worker pool, without network access. A value containing `/` or
/// `\`, or equal to `.` or `..`, is a model directory relative to the current directory. Any other
/// value is a name registered by `downloadModel` or `ModelStore.register`, looked up in
/// `options.path`, otherwise in `SPARS_MODEL_DIR` or the user cache. When `options.path` is given,
/// the value is always a name. Rejects with `SPARS_IO` when the model is not installed,
/// `SPARS_INVALID_MODEL` for invalid names or files, and `SPARS_UNSUPPORTED` for unsupported
/// pipelines.
///
/// ```js
/// const nlp = await loadModel('en_core_web_sm')
/// ```
#[napi(
    strict,
    ts_args_type = "path: ModelName, options?: LoadOptions | undefined | null"
)]
pub fn load_model(path: Utf16String, options: Option<models::LoadOptions>) -> AsyncTask<LoadTask> {
    AsyncTask::new(LoadTask {
        source: models::LoadSource::prepare(&path, options),
    })
}

/// Bytes of external memory reported through Node-API to this JavaScript thread's V8 isolate,
/// including the documents SpaRs holds. V8 counts it when deciding to collect garbage. The value is
/// an estimate and a lower bound: token and span views that outlive their document are not counted.
#[napi]
pub fn reported_native_memory(env: Env) -> Result<f64> {
    // Adjusting by zero reads the running total without changing it. This relies on V8's
    // AdjustAmountOfExternalAllocatedMemory returning that total; tests/memory.test.mts checks it.
    Ok(env.adjust_external_memory(0)? as f64)
}

fn setting(value: f64, minimum: u32, name: &str) -> Result<u32> {
    if !value.is_finite()
        || value.fract() != 0.0
        || value < f64::from(minimum)
        || value > f64::from(u32::MAX)
    {
        return Err(napi::Error::from_reason(format!(
            "{name} must be an integer in {minimum}..=4294967295"
        )));
    }
    Ok(value as u32)
}
