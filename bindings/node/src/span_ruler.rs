//! Rule-based span groups and entities over immutable native documents, following spaCy's
//! `span_ruler`.
use crate::{
    document::NativeDocument,
    entity_ruler::{known_fields, Kind, Pattern},
    errors, output, phrase_matcher,
    token_matcher::TokenPattern,
};
use napi::{
    bindgen_prelude::{AsyncTask, JsObjectValue, Object, Unknown, Utf16String},
    Env, Result, Task, ValueType,
};
use napi_derive::napi;
use std::sync::Arc;

// scripts/declarations.mts replaces this interface and documents it.
#[napi(object)]
pub struct SpanRulerPattern {
    #[napi(ts_type = "SpanLabel | ''")]
    pub label: String,
    pub id: Option<String>,
    pub tokens: Option<TokenPattern>,
    pub phrase: Option<String>,
}

/// spaCy's registered filter names, in the order of `spars::SpanFilter`.
const FILTERS: [(&str, spars::SpanFilter); 3] = [
    ("first_longest", spars::SpanFilter::FirstLongest),
    ("prioritize_new", spars::SpanFilter::PrioritizeNew),
    ("prioritize_existing", spars::SpanFilter::PrioritizeExisting),
];

fn filter(name: &str) -> errors::Result<spars::SpanFilter> {
    FILTERS
        .iter()
        .find(|(known, _)| *known == name)
        .map(|&(_, filter)| filter)
        .ok_or_else(|| {
            spars::Error::Unsupported(format!("the span ruler has no filter named {name:?}")).into()
        })
}

fn filter_name(filter: spars::SpanFilter) -> &'static str {
    FILTERS
        .iter()
        .find(|(_, known)| *known == filter)
        .map(|&(name, _)| name)
        .expect("every filter is named")
}

/// The options as read from JavaScript, before names are resolved.
#[derive(Default)]
struct Settings {
    // `None` keeps the default key; `Some(None)` writes no span group.
    spans_key: Option<Option<String>>,
    spans_filter: Option<String>,
    annotate_entities: Option<bool>,
    entities_filter: Option<String>,
    phrase_attribute: Option<String>,
    overwrite: Option<bool>,
}

impl Settings {
    fn from_object(env: Env, options: &Object) -> Result<Self> {
        known_fields(
            options,
            "SpanRuler option",
            &[
                "spansKey",
                "spansFilter",
                "annotateEntities",
                "entitiesFilter",
                "phraseAttribute",
                "overwrite",
            ],
        )?;
        let text = |name: &str| -> Result<Option<String>> {
            options
                .get_named_property::<Option<Utf16String>>(name)?
                .map(|value| errors::text(&value).map_err(|error| error.into_napi(env)))
                .transpose()
        };
        // Unlike the other options, `null` differs from an omitted key: it disables the group.
        let key: Unknown<'_> = options.get_named_property("spansKey")?;
        let spans_key = match key.get_type()? {
            ValueType::Undefined => None,
            ValueType::Null => Some(None),
            _ => Some(text("spansKey")?),
        };
        Ok(Self {
            spans_key,
            spans_filter: text("spansFilter")?,
            annotate_entities: options.get_named_property("annotateEntities")?,
            entities_filter: text("entitiesFilter")?,
            phrase_attribute: text("phraseAttribute")?,
            overwrite: options.get_named_property("overwrite")?,
        })
    }

    fn native(self) -> errors::Result<spars::SpanRulerOptions> {
        let mut native = spars::SpanRulerOptions::default();
        if let Some(key) = self.spans_key {
            native.spans_key = key;
        }
        native.spans_filter = self.spans_filter.as_deref().map(filter).transpose()?;
        if let Some(annotate) = self.annotate_entities {
            native.annotate_entities = annotate;
        }
        if let Some(name) = self.entities_filter {
            native.entities_filter = filter(&name)?;
        }
        native.phrase_attribute = phrase_matcher::attribute(self.phrase_attribute.as_deref())?;
        if let Some(overwrite) = self.overwrite {
            native.overwrite = overwrite;
        }
        Ok(native)
    }
}

/// Adds a span group and, optionally, entities from token patterns and phrases, like spaCy's
/// `span_ruler`. It does not run inside a model's pipeline: call `annotate` on a `NativeDocument`
/// from `Model.processDocument` or `NativeDocument.fromSnapshot`. Documents are immutable:
/// `annotate` returns a new document. docs/SPAN_RULER.md describes filtering and the differences
/// from spaCy.
///
/// ```js
/// const nlp = await loadModel('en_core_web_sm')
/// const ruler = new SpanRuler()
/// ruler.add([{ label: 'ORG', phrase: await nlp.processDocument('Acme Corp', 'Tokenizer') }])
/// const doc = await ruler.annotate(await nlp.processDocument('Acme Corp hired Alice.', 'Tokenizer'))
/// doc.toObject().spanGroups // [{ key: 'ruler', name: 'ruler', attrs: {}, spans: [{ start: 0, end: 2, label: 'ORG', id: '' }] }]
/// ```
#[napi]
pub struct SpanRuler {
    inner: Arc<spars::SpanRuler>,
}

#[napi]
impl SpanRuler {
    /// Create an empty ruler; omitted options take the defaults of spaCy's `span_ruler` factory.
    /// Pass a model to evaluate lexical flags, such as `is_alpha` in token conditions or
    /// `'IS_ALPHA'` as `phraseAttribute`; only its lexical rules are used, and it is never run.
    /// Without one, adding a flag condition, or any phrase under a flag `phraseAttribute`, throws
    /// `SPARS_INVALID_PATTERN`. An unknown filter or `phraseAttribute` throws `SPARS_UNSUPPORTED`,
    /// a string option with an unpaired surrogate `SPARS_INVALID_TEXT`, and an unknown option or a
    /// value of the wrong type `InvalidArg`.
    #[napi(
        constructor,
        ts_args_type = "options?: SpanRulerOptions | undefined | null, model?: Model | undefined | null"
    )]
    pub fn new(env: Env, options: Option<Object>, model: Option<&crate::Model>) -> Result<Self> {
        let settings = match options {
            Some(options) => Settings::from_object(env, &options)?,
            None => Settings::default(),
        };
        let options = settings.native().map_err(|error| error.into_napi(env))?;
        let native = match model {
            Some(model) => spars::SpanRuler::with_lexicon(options, model.inner.lexicon()),
            None => spars::SpanRuler::new(options),
        };
        Ok(Self {
            inner: Arc::new(native),
        })
    }

    /// The `spansKey` option: `'ruler'` by default, or `null` when the ruler writes no span group.
    /// `''` also writes none, as in spaCy. Options are fixed at creation.
    #[napi(getter, ts_return_type = "SpanGroupKey | null")]
    pub fn spans_key(&self) -> Option<String> {
        self.inner.options().spans_key.clone()
    }

    /// The `spansFilter` option, or `null` (the default) when every match goes into the group.
    #[napi(getter, ts_return_type = "SpanFilter | null")]
    pub fn spans_filter(&self) -> Option<&'static str> {
        self.inner.options().spans_filter.map(filter_name)
    }

    /// The `annotateEntities` option, or its default `false`.
    #[napi(getter)]
    pub fn annotate_entities(&self) -> bool {
        self.inner.options().annotate_entities
    }

    /// The `entitiesFilter` option, or its default `'first_longest'`.
    #[napi(getter, ts_return_type = "SpanFilter")]
    pub fn entities_filter(&self) -> &'static str {
        filter_name(self.inner.options().entities_filter)
    }

    /// The `phraseAttribute` option, or its default `'ORTH'`.
    #[napi(getter, ts_return_type = "PhraseAttribute")]
    pub fn phrase_attribute(&self) -> &'static str {
        phrase_matcher::attribute_name(self.inner.options().phrase_attribute)
    }

    /// The `overwrite` option, or its default `true`.
    #[napi(getter)]
    pub fn overwrite(&self) -> bool {
        self.inner.options().overwrite
    }

    /// The number of added patterns, including duplicates and empty phrases, like spaCy's
    /// `len(ruler)`.
    #[napi(getter)]
    pub fn size(&self, env: Env) -> Result<u32> {
        errors::number(self.inner.len()).map_err(|error| error.into_napi(env))
    }

    /// Distinct labels of the current patterns in code-point order, like spaCy's `labels`.
    /// Includes `''` when a pattern has an empty label. Labels whose patterns were all removed are
    /// not listed, unlike `contains`.
    #[napi(getter, ts_return_type = "Array<SpanLabel | ''>")]
    pub fn labels(&self) -> Vec<String> {
        self.inner.labels()
    }

    /// Distinct IDs of the current patterns in code-point order, like spaCy's `ids`. A pattern
    /// added with `id: ''` contributes `''`; one added without an ID contributes nothing.
    #[napi(getter)]
    pub fn ids(&self) -> Vec<String> {
        self.inner.ids()
    }

    /// The current patterns in the order they were added, like spaCy's `patterns`, with phrases
    /// as their document text. `id` is exactly as added: absent without one and `''` for the empty
    /// ID. Each read returns a new array; changing it does not change the ruler.
    #[napi(getter, ts_return_type = "Array<SpanRulerPattern>")]
    pub fn patterns(&self) -> Vec<SpanRulerPattern> {
        self.inner
            .patterns()
            .iter()
            .map(|pattern| {
                let (tokens, phrase) = match &pattern.pattern {
                    spars::RulerPatternKind::Tokens(tokens) => {
                        (Some(TokenPattern::from_native(tokens)), None)
                    }
                    spars::RulerPatternKind::Phrase(text) => (None, Some(text.clone())),
                };
                SpanRulerPattern {
                    label: pattern.label.clone(),
                    id: pattern.id.clone(),
                    tokens,
                    phrase,
                }
            })
            .collect()
    }

    /// Whether `label` was ever added, like spaCy's `label in ruler`. As in spaCy, a label stays
    /// known after `remove`, `removeById` and `clear` remove its patterns. A label with an unpaired
    /// surrogate throws `SPARS_INVALID_TEXT`.
    #[napi(strict, ts_args_type = "label: SpanLabel | ''")]
    pub fn contains(&self, env: Env, label: Utf16String) -> Result<bool> {
        let label = errors::text(&label).map_err(|error| error.into_napi(env))?;
        Ok(self.inner.contains(&label))
    }

    /// Add patterns, like spaCy's `add_patterns`. Patterns with the same label and ID form one rule;
    /// duplicates are kept and counted by `size`. Every pattern is checked before any is added, so
    /// a failure adds nothing; spaCy can keep earlier patterns of a failed call (see
    /// docs/SPACY_DIFFERENCES.md). `SPARS_INVALID_PATTERN` is thrown for a non-array argument, an
    /// unknown field, a `phrase` that is not a `NativeDocument`, a `label` or `id` that is not a
    /// string, both or neither of `tokens` and `phrase`, an invalid token pattern, a lexical flag
    /// condition or flag-attribute phrase on a ruler created without a model, and a nonempty phrase
    /// document in which no token has the annotation `phraseAttribute` compares. A `MORPH` phrase
    /// with noncanonical morphology throws `SPARS_UNSUPPORTED`, and a label or ID with an unpaired
    /// surrogate `SPARS_INVALID_TEXT`. Throws `SPARS_BUSY` while a `findMatches` or `annotate`
    /// call on this ruler is queued or running.
    #[napi(strict, ts_args_type = "patterns: Array<SpanPattern>")]
    pub fn add(&mut self, env: Env, patterns: Vec<Object>) -> Result<()> {
        let ruler = Arc::get_mut(&mut self.inner).ok_or_else(|| errors::busy().into_napi(env))?;
        let patterns = patterns
            .iter()
            .map(|object| Pattern::from_object(env, object, "span pattern"))
            .collect::<Result<Vec<_>>>()?;
        // Phrase documents are borrowed from their own list, so token patterns can move.
        let documents: Vec<Arc<spars::Doc>> = patterns
            .iter()
            .filter_map(|pattern| match &pattern.kind {
                Kind::Phrase(doc) => Some(Arc::clone(doc)),
                Kind::Tokens(_) => None,
            })
            .collect();
        let mut documents = documents.iter();
        let native: Vec<spars::SpanPattern<'_>> = patterns
            .into_iter()
            .map(|pattern| spars::SpanPattern {
                label: pattern.label,
                id: pattern.id,
                pattern: match pattern.kind {
                    Kind::Tokens(tokens) => spars::SpanPatternKind::Tokens(tokens),
                    Kind::Phrase(_) => spars::SpanPatternKind::Phrase(
                        documents.next().expect("one document per phrase pattern"),
                    ),
                },
            })
            .collect();
        ruler
            .add(&native)
            .map_err(|error| errors::registration(error).into_napi(env))
    }

    /// Remove every pattern with `label`, like spaCy's `remove`. Throws `SPARS_INVALID_PATTERN` and
    /// changes nothing when `label` was never added; a label whose patterns were already removed
    /// is still known (see `contains`), so removing it again succeeds and changes nothing. A label
    /// with an unpaired surrogate throws `SPARS_INVALID_TEXT`. Throws `SPARS_BUSY` while matching on
    /// this ruler is pending.
    #[napi(strict, ts_args_type = "label: SpanLabel | ''")]
    pub fn remove(&mut self, env: Env, label: Utf16String) -> Result<()> {
        let label = errors::text(&label).map_err(|error| error.into_napi(env))?;
        let ruler = Arc::get_mut(&mut self.inner).ok_or_else(|| errors::busy().into_napi(env))?;
        ruler
            .remove(&label)
            .map_err(|error| errors::BindingError::from(error).into_napi(env))
    }

    /// Remove every pattern added with this ID, like spaCy's `remove_by_id`. Throws
    /// `SPARS_INVALID_PATTERN` and changes nothing when no current pattern was added with exactly
    /// this ID; for `''`, patterns added without an ID do not count. As in
    /// spaCy, `removeById('')` also stops every pattern added without an ID from matching, while
    /// `patterns`, `size` and `labels` still list them; adding a pattern with the same label and no
    /// ID makes only the new one match. An ID with an unpaired surrogate throws
    /// `SPARS_INVALID_TEXT`. Throws `SPARS_BUSY` while matching on this ruler is pending.
    #[napi(strict)]
    pub fn remove_by_id(&mut self, env: Env, id: Utf16String) -> Result<()> {
        let id = errors::text(&id).map_err(|error| error.into_napi(env))?;
        let ruler = Arc::get_mut(&mut self.inner).ok_or_else(|| errors::busy().into_napi(env))?;
        ruler
            .remove_by_id(&id)
            .map_err(|error| errors::BindingError::from(error).into_napi(env))
    }

    /// Remove every pattern, like spaCy's `clear`, keeping the options and model. Labels stay
    /// known to `contains`. Throws `SPARS_BUSY` while matching on this ruler is pending.
    #[napi]
    pub fn clear(&mut self, env: Env) -> Result<()> {
        let ruler = Arc::get_mut(&mut self.inner).ok_or_else(|| errors::busy().into_napi(env))?;
        ruler.clear();
        Ok(())
    }

    /// Find the spans the ruler would add, like spaCy's `SpanRuler.match`, on Node's worker pool,
    /// before any filter. Results are ordered by start token, then end token; a label and ID pair
    /// matching the same tokens twice is listed once. Matches of different rules over the same
    /// tokens are ordered by spaCy's string hashes of the label, then the ID, which is not
    /// alphabetical and decides which identical span a filter keeps. Matches may overlap; empty
    /// matches are dropped. A pattern without an ID, or with `id: ''`, gives matches `id: ''`.
    /// Rejects with `SPARS_INFERENCE` when an `equals` token condition on `lemma`, `pos`, `tag`,
    /// `dep` or `morphology` meets a nonempty document in which no token has that annotation, like
    /// spaCy's error E155; with `SPARS_UNSUPPORTED` when a token condition or a `MORPH` phrase
    /// attribute compares noncanonical document morphology; with `SPARS_BUSY` when the execution
    /// queue is full (see `configureExecution`); and with `SPARS_INPUT_LIMIT` when
    /// `doc.utf16Length` exceeds the `maxTextLength` of `configureInputLimits`. Throws
    /// `TypeError` when `doc` is not a `NativeDocument`.
    #[napi(strict, ts_return_type = "Promise<Array<Span<SpanLabel | ''>>>")]
    pub fn find_matches(&self, doc: &NativeDocument) -> AsyncTask<SpanMatchTask> {
        AsyncTask::new(SpanMatchTask {
            ruler: Arc::clone(&self.inner),
            doc: Arc::clone(&doc.inner),
        })
    }

    /// Return a new document with the ruler's span group and, with `annotateEntities`, its
    /// entities, like applying spaCy's ruler, on Node's worker pool; the given document and its
    /// views are unchanged. With `overwrite` (the default), the group under `spansKey` is replaced
    /// by one named after the key, without attrs, and existing entities are discarded before
    /// filtering. Without it, an existing group keeps its name and attrs and is extended, and
    /// existing entities compete with the matches in `entitiesFilter`. As in spaCy, extending a
    /// group through `spansFilter` repeats the existing spans the filter keeps. Without
    /// `spansFilter`, the group receives the matches in `findMatches` order; with `spansKey` `null`
    /// or `''`, no span group is written or changed. When entities are written, every token
    /// outside them becomes outside (IOB `O`). Rejects like `findMatches`.
    #[napi(strict, ts_return_type = "Promise<NativeDocument>")]
    pub fn annotate(&self, doc: &NativeDocument) -> AsyncTask<SpanAnnotateTask> {
        AsyncTask::new(SpanAnnotateTask {
            ruler: Arc::clone(&self.inner),
            doc: Arc::clone(&doc.inner),
        })
    }
}

pub struct SpanMatchTask {
    ruler: Arc<spars::SpanRuler>,
    doc: Arc<spars::Doc>,
}

#[napi]
impl Task for SpanMatchTask {
    type Output = errors::Result<Vec<output::Span>>;
    type JsValue = Vec<output::Span>;

    fn compute(&mut self) -> Result<Self::Output> {
        Ok(self
            .ruler
            .find_matches(&self.doc)
            .map_err(Into::into)
            .and_then(|matches| {
                matches
                    .into_iter()
                    .map(|matched| {
                        Ok(output::Span {
                            start: errors::number(matched.start.0)?,
                            end: errors::number(matched.end.0)?,
                            label: matched.label,
                            id: matched.id.unwrap_or_default(),
                        })
                    })
                    .collect()
            }))
    }

    fn resolve(&mut self, env: Env, output: Self::Output) -> Result<Self::JsValue> {
        output.map_err(|error| error.into_napi(env))
    }
}

pub struct SpanAnnotateTask {
    ruler: Arc<spars::SpanRuler>,
    doc: Arc<spars::Doc>,
}

#[napi]
impl Task for SpanAnnotateTask {
    type Output = errors::Result<NativeDocument>;
    type JsValue = NativeDocument;

    fn compute(&mut self) -> Result<Self::Output> {
        Ok(self
            .ruler
            .annotated(&self.doc)
            .map_err(Into::into)
            .and_then(NativeDocument::from_doc))
    }

    fn resolve(&mut self, env: Env, output: Self::Output) -> Result<Self::JsValue> {
        output.map_err(|error| error.into_napi(env))?.reported(&env)
    }
}
