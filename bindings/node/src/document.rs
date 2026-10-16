use crate::{convert, errors, output::Document, views};
use napi::bindgen_prelude::{JsObjectValue, Object, ObjectFinalize, Utf16String};
use napi::{Env, Result, Status};
use napi_derive::napi;
use std::sync::Arc;

/// An immutable processed document kept in native memory, independent of the model that made it.
/// Create it with `Model.processDocument`, `NativeDocument.fromSnapshot` or `withEntities`. Pass it
/// to matchers and vector methods, or read it through token and span views.
#[napi(custom_finalize)]
pub struct NativeDocument {
    pub(crate) inner: Arc<spars::Doc>,
    utf16_length: u32,
    /// Native bytes to report to V8 for this object, estimated where the document is built.
    estimated_bytes: i64,
    /// Native bytes reported to V8 for this object, released when it is collected.
    external_bytes: i64,
}

impl NativeDocument {
    /// Wrap a document, estimating its memory on the calling thread (a worker thread for
    /// inference); call [`NativeDocument::reported`] on the JavaScript thread before returning it.
    pub(crate) fn from_doc(doc: spars::Doc) -> errors::Result<Self> {
        let utf16_length = errors::number(doc.text().encode_utf16().count())?;
        let estimated_bytes = i64::try_from(doc.estimated_heap_bytes()).unwrap_or(i64::MAX);
        Ok(Self {
            inner: Arc::new(doc),
            utf16_length,
            estimated_bytes,
            external_bytes: 0,
        })
    }

    /// Report the document's native memory to V8, so garbage collection accounts for it. The
    /// object owning the document carries the report; token and span views that share the
    /// document add nothing, so memory they keep alive after the document object is collected
    /// is unreported.
    pub(crate) fn reported(mut self, env: &Env) -> Result<Self> {
        env.adjust_external_memory(self.estimated_bytes)?;
        self.external_bytes = self.estimated_bytes;
        Ok(self)
    }
}

impl ObjectFinalize for NativeDocument {
    fn finalize(self, env: Env) -> Result<()> {
        if self.external_bytes > 0 {
            env.adjust_external_memory(-self.external_bytes)?;
        }
        Ok(())
    }
}

/// A labeled half-open token interval to annotate as an entity.
#[napi(object)]
pub struct EntityInput {
    pub start: f64,
    pub end: f64,
    /// An empty label ignores the interval after its bounds are checked, as spaCy does; such an
    /// interval may overlap other intervals.
    #[napi(ts_type = "EntityLabel | ''")]
    pub label: Utf16String,
    /// Written to the entity's tokens. Without one, or with `''`, each token keeps its current ID,
    /// as in spaCy.
    pub id: Option<Utf16String>,
}

/// An unlabeled half-open token interval.
#[napi(object)]
pub struct TokenRange {
    pub start: f64,
    pub end: f64,
}

/// How tokens outside every given interval are annotated, like spaCy's `Doc.set_ents` default.
#[napi(string_enum = "lowercase")]
pub enum EntityDefault {
    /// Outside any entity: `entityIob` `'O'`, `entityType` `''`.
    Outside,
    /// Unknown: `entityIob` and `entityType` null.
    Missing,
    /// Never part of an entity: `entityIob` `'B'`, `entityType` `''`.
    Blocked,
    /// Keep the current tags. Existing entities that overlap a new interval are split at its
    /// boundaries, as in spaCy.
    Unmodified,
}

/// A replacement of entity annotation. Intervals in all lists, except entities with an empty label,
/// must not share tokens; sharing throws `SPARS_BOUNDS`.
#[napi(object)]
pub struct EntityUpdate {
    pub entities: Option<Vec<EntityInput>>,
    /// Tokens that can never be part of an entity; they appear as `entityIob` `'B'` with
    /// `entityType` `''`.
    pub blocked: Option<Vec<TokenRange>>,
    /// Tokens whose entity annotation is unknown; they appear with `entityIob` and `entityType` null.
    pub missing: Option<Vec<TokenRange>>,
    /// Tokens outside any entity; they appear as `entityIob` `'O'` with `entityType` `''`.
    pub outside: Option<Vec<TokenRange>>,
    /// Defaults to `outside`.
    pub default: Option<EntityDefault>,
}

/// napi objects ignore undeclared properties; reject them so a misspelled field such as `ents`
/// or an unsupported one such as `kbId` cannot silently change the update.
fn known_fields(object: &Object, allowed: &[&str], kind: &str) -> Result<()> {
    let names = object.get_property_names()?;
    for i in 0..names.get_array_length()? {
        let name: String = names.get_element(i)?;
        if !allowed.contains(&name.as_str()) {
            return Err(napi::Error::new(
                Status::InvalidArg,
                format!("unknown {kind} field `{name}`"),
            ));
        }
    }
    Ok(())
}

/// A span to store in a span group: tokens `start` to `end` (exclusive), as token indices, not
/// character offsets. It needs `0 <= start <= end <=` the number of tokens, may be empty
/// (`start === end`), and need not line up with entities or sentences.
#[napi(object)]
pub struct SpanInput {
    pub start: f64,
    pub end: f64,
    /// Defaults to `''`.
    #[napi(ts_type = "SpanLabel | ''")]
    pub label: Option<Utf16String>,
    /// spaCy's `Span.id_`. Without one, or with `''`, the span has no ID.
    pub id: Option<Utf16String>,
}

/// The native form of a span group, built by the package wrapper from `SpanGroupInput` after it
/// checks the attrs and writes them as JSON text.
struct NativeSpanGroupInput {
    name: Option<Utf16String>,
    attrs: Option<String>,
    spans: Vec<SpanInput>,
}

impl NativeSpanGroupInput {
    fn from_object(object: &Object) -> Result<Self> {
        known_fields(object, &["name", "attrs", "spans"], "span group")?;
        // Read each span's fields from the object already in hand, rather than converting the
        // list a second time.
        let spans = object
            .get_named_property::<Vec<Object>>("spans")?
            .iter()
            .map(|span| {
                known_fields(span, &["start", "end", "label", "id"], "span")?;
                Ok(SpanInput {
                    start: span.get_named_property("start")?,
                    end: span.get_named_property("end")?,
                    label: span.get_named_property("label")?,
                    id: span.get_named_property("id")?,
                })
            })
            .collect::<Result<_>>()?;
        Ok(Self {
            name: object.get_named_property("name")?,
            attrs: object.get_named_property("attrs")?,
            spans,
        })
    }

    fn native(self, key: &str, env: Env) -> Result<spars::SpanGroup> {
        let text = |value: &Utf16String| errors::text(value).map_err(|error| error.into_napi(env));
        let name = self.name.as_ref().map(text).transpose()?;
        let spans = self
            .spans
            .into_iter()
            .map(|input| {
                let mut span = spars::Span::new(
                    views::index(input.start, env)?,
                    views::index(input.end, env)?,
                    input
                        .label
                        .as_ref()
                        .map(text)
                        .transpose()?
                        .unwrap_or_default(),
                );
                span.id = input.id.as_ref().map(text).transpose()?;
                Ok(span)
            })
            .collect::<Result<_>>()?;
        // spaCy names a group after its key when a plain list of spans is assigned.
        let mut group = spars::SpanGroup::new(name.unwrap_or_else(|| key.to_owned()), spans);
        if let Some(attrs) = self.attrs {
            // The package wrapper passes checked JSON text, so this fails only when the native
            // method is called directly with text that is not a JSON object.
            group.attrs = serde_json::from_str(&attrs).map_err(|error| {
                napi::Error::new(
                    Status::InvalidArg,
                    format!("invalid span group attrs: {error}"),
                )
            })?;
        }
        Ok(group)
    }
}

impl EntityUpdate {
    fn from_object(object: &Object) -> Result<Self> {
        known_fields(
            object,
            &["entities", "blocked", "missing", "outside", "default"],
            "entity update",
        )?;
        for (name, allowed) in [
            ("entities", &["start", "end", "label", "id"][..]),
            ("blocked", &["start", "end"]),
            ("missing", &["start", "end"]),
            ("outside", &["start", "end"]),
        ] {
            for item in object
                .get_named_property::<Option<Vec<Object>>>(name)?
                .unwrap_or_default()
            {
                known_fields(&item, allowed, "entity update")?;
            }
        }
        Ok(Self {
            entities: object.get_named_property("entities")?,
            blocked: object.get_named_property("blocked")?,
            missing: object.get_named_property("missing")?,
            outside: object.get_named_property("outside")?,
            default: object.get_named_property("default")?,
        })
    }

    fn native(self, env: Env) -> Result<spars::EntityUpdate> {
        let ranges = |ranges: Option<Vec<TokenRange>>| -> Result<Vec<spars::TokenRange>> {
            ranges
                .unwrap_or_default()
                .into_iter()
                .map(|range| {
                    Ok(spars::TokenRange {
                        start: views::index(range.start, env)?,
                        end: views::index(range.end, env)?,
                    })
                })
                .collect()
        };
        let entities = self
            .entities
            .unwrap_or_default()
            .into_iter()
            .map(|entity| {
                let text =
                    |value: &Utf16String| errors::text(value).map_err(|error| error.into_napi(env));
                let mut span = spars::Span::new(
                    views::index(entity.start, env)?,
                    views::index(entity.end, env)?,
                    text(&entity.label)?,
                );
                span.id = entity.id.as_ref().map(text).transpose()?;
                Ok(span)
            })
            .collect::<Result<_>>()?;
        Ok(spars::EntityUpdate {
            entities,
            blocked: ranges(self.blocked)?,
            missing: ranges(self.missing)?,
            outside: ranges(self.outside)?,
            default: match self.default.unwrap_or(EntityDefault::Outside) {
                EntityDefault::Outside => spars::EntityDefault::Outside,
                EntityDefault::Missing => spars::EntityDefault::Missing,
                EntityDefault::Blocked => spars::EntityDefault::Blocked,
                EntityDefault::Unmodified => spars::EntityDefault::Unmodified,
            },
        })
    }
}

impl NativeDocument {
    /// Wrap an edited copy of this document's text. The text is unchanged, so its UTF-16 length
    /// is reused; the memory estimate is recomputed because a span group can add any amount.
    fn edited(&self, doc: spars::Doc, env: Env) -> Result<Self> {
        Self {
            utf16_length: self.utf16_length,
            estimated_bytes: i64::try_from(doc.estimated_heap_bytes()).unwrap_or(i64::MAX),
            inner: Arc::new(doc),
            external_bytes: 0,
        }
        .reported(&env)
    }
}

#[napi]
impl NativeDocument {
    /// Text length in JavaScript UTF-16 units without constructing an output copy.
    #[napi(getter)]
    pub fn utf16_length(&self) -> u32 {
        self.utf16_length
    }

    /// Restore a document from `toSnapshot` output; this is not spaCy's JSON format. `entities` may
    /// be null only when no token has an entity IOB tag. Throws `SPARS_INVALID_TEXT` for an unpaired
    /// surrogate; `SPARS_INVALID_MODEL` for malformed JSON, invalid labels or tags, invalid
    /// contextual vectors, an `I` tag with no entity to continue, an entity list (or null) other
    /// than the one the token IOB tags describe, an empty entity ID, or an entity whose `id` differs
    /// from its first token's `entity_id`; `SPARS_BOUNDS` for inconsistent offsets or token indices;
    /// and `SPARS_UNSUPPORTED` for an unknown format version or data the stated version cannot hold
    /// (contextual vectors need version 2, entity IDs version 3).
    #[napi(factory, strict)]
    pub fn from_snapshot(env: Env, json: Utf16String) -> Result<Self> {
        let json = errors::text(&json).map_err(|error| error.into_napi(env))?;
        spars::Doc::from_json(&json)
            .map_err(errors::BindingError::from)
            .and_then(Self::from_doc)
            .map_err(|error| error.into_napi(env))?
            .reported(&env)
    }

    /// A JSON string with the text and all annotations, plus contextual vectors when the document has
    /// them. Restore it with `NativeDocument.fromSnapshot`.
    #[napi]
    pub fn to_snapshot(&self, env: Env) -> Result<String> {
        self.inner
            .to_json()
            .map_err(|error| errors::BindingError::from(error).into_napi(env))
    }

    /// Return a new document with replaced entity annotation, like spaCy's `Doc.set_ents`. This
    /// document and its token and span views are unchanged. Tokens no interval covers are set
    /// according to `default` (`'outside'` when omitted). Throws `SPARS_BOUNDS` for a non-integer,
    /// negative, reversed or past-the-end interval, or for intervals sharing a token (entities with
    /// an empty label are exempt); `SPARS_INVALID_TEXT` for a label or ID with an unpaired
    /// surrogate; `SPARS_UNSUPPORTED` when the update blocks the token before an `I` that has no
    /// entity type and leaves that `I` unwritten (`default: 'unmodified'`), where spaCy instead
    /// fails when the entities are next read; and `InvalidArg` for undeclared fields.
    #[napi(strict, ts_args_type = "update: EntityUpdate")]
    pub fn with_entities(&self, env: Env, update: Object) -> Result<NativeDocument> {
        let update = EntityUpdate::from_object(&update)?.native(env)?;
        // Checked before copying, so an invalid update does not copy the document.
        let doc = self
            .inner
            .with_entities(&update)
            .map_err(|error| errors::BindingError::from(error).into_napi(env))?;
        // Reuse the source's estimate instead of walking every token again. A copy has no spare
        // capacity, so it usually owns less than its source; it owns more only when the update
        // adds entity tags and spans the source lacked, such as an update to a document that
        // never ran the recognizer. That under-reports by the new tags and spans, not by a
        // multiple of the document.
        Self {
            inner: Arc::new(doc),
            utf16_length: self.utf16_length,
            estimated_bytes: self.estimated_bytes,
            external_bytes: 0,
        }
        .reported(&env)
    }

    /// Return a new document with `group` stored under `key`, like assigning to spaCy's
    /// `Doc.spans[key]`. A new key goes last; an existing key keeps its position and its group
    /// is replaced. This document and its views are unchanged. A span whose `start` or `end` is
    /// not a whole number, or that breaks `0 <= start <= end <=` the number of tokens, throws
    /// `SPARS_BOUNDS`. A key, name, label, ID or attrs string with an unpaired surrogate throws
    /// `SPARS_INVALID_TEXT`. An unknown property on the group or on a span throws `InvalidArg`,
    /// and attrs that are not plain JSON throw `TypeError`.
    #[napi(strict, ts_args_type = "key: SpanGroupKey, group: SpanGroupInput")]
    pub fn with_span_group(
        &self,
        env: Env,
        key: Utf16String,
        group: Object,
    ) -> Result<NativeDocument> {
        let key = errors::text(&key).map_err(|error| error.into_napi(env))?;
        let group = NativeSpanGroupInput::from_object(&group)?.native(&key, env)?;
        // Check bounds as `Doc::set_span_group` does before copying, so an invalid group does
        // not copy the document.
        let length = self.inner.tokens().len();
        if group
            .spans
            .iter()
            .any(|span| span.start.0 > span.end.0 || span.end.0 > length)
        {
            return Err(errors::BindingError::from(spars::Error::Bounds).into_napi(env));
        }
        let mut doc = (*self.inner).clone();
        doc.set_span_group(key, group)
            .map_err(|error| errors::BindingError::from(error).into_napi(env))?;
        self.edited(doc, env)
    }

    /// Return a new document without the group under `key`, like `del doc.spans[key]` in spaCy;
    /// the other keys keep their order. A key with no group throws `InvalidArg`, as spaCy raises
    /// `KeyError`.
    #[napi(strict, ts_args_type = "key: SpanGroupKey")]
    pub fn without_span_group(&self, env: Env, key: Utf16String) -> Result<NativeDocument> {
        let key = errors::text(&key).map_err(|error| error.into_napi(env))?;
        if !self.inner.span_groups().contains_key(&key) {
            return Err(napi::Error::new(
                Status::InvalidArg,
                format!("no span group with key {key:?}"),
            ));
        }
        let mut doc = (*self.inner).clone();
        doc.remove_span_group(&key);
        self.edited(doc, env)
    }

    /// The same plain `Document` that `Model.process` returns, as an independent copy.
    #[napi]
    pub fn to_object(&self, env: Env) -> Result<Document> {
        convert::document(&self.inner).map_err(|error| error.into_napi(env))
    }
}
