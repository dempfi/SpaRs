//! Offline model selection backed by the shared Rust store.
use crate::errors;
use napi::{
    bindgen_prelude::{AsyncTask, Utf16String},
    Env, Result, Task,
};
use napi_derive::napi;
use std::path::{Path, PathBuf};

/// An offline directory of explicitly selected model installations.
#[napi]
pub struct ModelStore {
    inner: spars::ModelStore,
}

#[napi]
impl ModelStore {
    /// Capture an explicit directory relative to the current working directory.
    /// The directory need not exist until registration or resolution.
    #[napi(constructor, strict)]
    pub fn new(env: Env, path: Utf16String) -> Result<Self> {
        absolute_path(&path)
            .map(|root| Self {
                inner: spars::ModelStore::new(root),
            })
            .map_err(|error| error.into_napi(env))
    }

    /// The store in `SPARS_MODEL_DIR` (relative to the current directory), otherwise `spars/models`
    /// inside the user cache directory (`~/Library/Caches` on macOS, `$XDG_CACHE_HOME` or `~/.cache`
    /// on Linux, `%LOCALAPPDATA%` on Windows). Throws `SPARS_INVALID_MODEL` when `SPARS_MODEL_DIR`
    /// is empty or no cache directory exists.
    #[napi(factory)]
    pub fn discover(env: Env) -> Result<Self> {
        spars::ModelStore::discover()
            .map(|inner| Self { inner })
            .map_err(|error| errors::BindingError::from(error).into_napi(env))
    }

    #[napi(getter)]
    pub fn root(&self, env: Env) -> Result<String> {
        path_text(self.inner.root()).map_err(|error| error.into_napi(env))
    }

    /// Resolve a registered model without downloading or reading its weights. Resolves to the
    /// absolute path of the installation; rejects with `SPARS_IO` when `name` is not registered.
    #[napi(
        strict,
        ts_args_type = "name: ModelName",
        ts_return_type = "Promise<string>"
    )]
    pub fn resolve(&self, name: Utf16String) -> AsyncTask<ResolveTask> {
        AsyncTask::new(ResolveTask {
            store: self.inner.clone(),
            name: errors::text(&name),
        })
    }

    /// Select `installed` (relative to the current directory), which must be a directory directly
    /// inside `root`, as the installation for `name`, replacing any earlier selection. This does not
    /// validate model weights. Names use lowercase ASCII letters, digits and underscores, starting
    /// with a letter, at most 128 bytes.
    #[napi(
        strict,
        ts_args_type = "name: ModelName, installed: string",
        ts_return_type = "Promise<void>"
    )]
    pub fn register(&self, name: Utf16String, installed: Utf16String) -> AsyncTask<RegisterTask> {
        AsyncTask::new(RegisterTask {
            store: self.inner.clone(),
            request: errors::text(&name)
                .and_then(|name| absolute_path(&installed).map(|installed| (name, installed))),
        })
    }
}

fn absolute_path(value: &Utf16String) -> errors::Result<PathBuf> {
    let value = errors::text(value)?;
    if value.is_empty() {
        return Err(spars::Error::Model("model path must not be empty".into()).into());
    }
    Ok(std::env::current_dir()
        .map_err(spars::Error::from)?
        .join(value))
}

fn path_text(path: &Path) -> errors::Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| spars::Error::Model("model path must be valid UTF-8".into()).into())
}

pub struct ResolveTask {
    store: spars::ModelStore,
    name: errors::Result<String>,
}

#[napi]
impl Task for ResolveTask {
    type Output = errors::Result<String>;
    type JsValue = String;

    fn compute(&mut self) -> Result<Self::Output> {
        Ok(self.name.as_ref().map_err(Clone::clone).and_then(|name| {
            self.store
                .resolve(name)
                .map_err(Into::into)
                .and_then(|path| path_text(&path))
        }))
    }

    fn resolve(&mut self, env: Env, output: Self::Output) -> Result<Self::JsValue> {
        output.map_err(|error| error.into_napi(env))
    }
}

pub struct RegisterTask {
    store: spars::ModelStore,
    request: errors::Result<(String, PathBuf)>,
}

#[napi]
impl Task for RegisterTask {
    type Output = errors::Result<()>;
    type JsValue = ();

    fn compute(&mut self) -> Result<Self::Output> {
        Ok(self
            .request
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|(name, path)| self.store.register(name, path).map_err(Into::into)))
    }

    fn resolve(&mut self, env: Env, output: Self::Output) -> Result<Self::JsValue> {
        output.map_err(|error| error.into_napi(env))
    }
}
