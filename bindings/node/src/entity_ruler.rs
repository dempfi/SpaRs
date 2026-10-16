//! Rule-based entity annotation over immutable native documents, following spaCy's
//! `entity_ruler`.
use crate::{document::NativeDocument, errors, phrase_matcher, token_matcher::TokenPattern};
use napi::{
    bindgen_prelude::{AsyncTask, ClassInstance, JsObjectValue, Object, Utf16String},
    Env, Result, Status, Task,
};
use napi_derive::napi;
use std::sync::Arc;

/// A candidate entity from `findMatches`, over tokens `start` to `end` (exclusive).
#[napi(object)]
pub struct EntityMatch {
    /// The pattern's label as added. `''` comes from a pattern with an empty label, for which
    /// `annotate` adds no entity.
    #[napi(ts_type = "EntityLabel | ''")]
    pub label: String,
    /// The pattern's ID, or `''` without one. `annotate` gives the entity this ID only when it is
    /// nonempty.
    pub id: String,
    #[napi(ts_type = "import('./units.js').TokenIndex")]
    pub start: u32,
    #[napi(ts_type = "import('./units.js').TokenIndex")]
    pub end: u32,
}

// scripts/declarations.mts replaces this interface and documents it.
#[napi(object)]
pub struct RulerPattern {
    #[napi(ts_type = "EntityLabel | ''")]
    pub label: String,
    pub id: Option<String>,
    pub tokens: Option<TokenPattern>,
    pub phrase: Option<String>,
}

/// napi objects ignore undeclared properties; reject them so a misspelled field such as `pattern`
/// cannot silently drop a rule.
fn known_fields(object: &Object, kind: &str, allowed: &[&str]) -> Result<()> {
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

enum Kind {
    Tokens(spars::TokenPattern),
    Phrase(Arc<spars::Doc>),
}

struct Pattern {
    label: String,
    id: Option<String>,
    kind: Kind,
}

impl Pattern {
    fn from_object(env: Env, object: &Object) -> Result<Self> {
        known_fields(
            object,
            "entity pattern",
            &["label", "id", "tokens", "phrase"],
        )?;
        let text = |value: Utf16String| errors::text(&value).map_err(|error| error.into_napi(env));
        let label = text(object.get_named_property("label")?)?;
        let id = object
            .get_named_property::<Option<Utf16String>>("id")?
            .map(text)
            .transpose()?;
        let tokens: Option<TokenPattern> = object.get_named_property("tokens")?;
        let phrase: Option<ClassInstance<'_, NativeDocument>> =
            object.get_named_property("phrase")?;
        let kind = match (tokens, phrase) {
            (Some(tokens), None) => {
                Kind::Tokens(tokens.into_native().map_err(|error| error.into_napi(env))?)
            }
            (None, Some(phrase)) => Kind::Phrase(Arc::clone(&phrase.inner)),
            _ => {
                return Err(napi::Error::new(
                    Status::InvalidArg,
                    "an entity pattern needs exactly one of `tokens` and `phrase`",
                ))
            }
        };
        Ok(Self { label, id, kind })
    }
}

/// Adds entities from token patterns and phrases, like spaCy's `entity_ruler` placed after the
/// named-entity recognizer. It does not run inside a model's pipeline: call `annotate` on a
/// `NativeDocument` from `Model.processDocument` or `NativeDocument.fromSnapshot`; to use it without
/// the recognizer, process with a stage before `Ner`. Longer matches win, then earlier ones. Of
/// matches by different rules (label and ID pairs) over exactly the same tokens, the rule added
/// first wins; spaCy leaves that tie to Python set order (see docs/ENTITY_RULER.md, "Differences
/// from spaCy"). Documents are immutable: `annotate` returns a new document.
///
/// ```js
/// const nlp = await loadModel('en_core_web_sm')
/// const ruler = new EntityRuler()
/// ruler.add([{ label: 'ORG', phrase: await nlp.processDocument('Acme Corp', 'Lemmatizer') }])
/// const doc = await ruler.annotate(await nlp.processDocument('Acme Corp hired Alice.', 'Lemmatizer'))
/// doc.toObject().entities // [{ start: 0, end: 2, label: 'ORG', id: '' }]
/// ```
#[napi]
pub struct EntityRuler {
    inner: Arc<spars::EntityRuler>,
}

#[napi]
impl EntityRuler {
    /// Create an empty ruler; omitted options take spaCy's defaults. Pass a model to evaluate
    /// lexical flags such as `IS_ALPHA` in token conditions and as `phraseAttribute`; only its
    /// lexical rules are used, and it is never run. Without one, adding a flag condition, or any
    /// phrase under a flag `phraseAttribute`, throws `SPARS_INVALID_PATTERN`. An empty
    /// `idSeparator` throws `SPARS_INVALID_PATTERN`, an `idSeparator` or `phraseAttribute` with an
    /// unpaired surrogate `SPARS_INVALID_TEXT`, an unknown `phraseAttribute` `SPARS_UNSUPPORTED`, and an unknown option or a value of the wrong type `InvalidArg`.
    #[napi(
        constructor,
        ts_args_type = "options?: EntityRulerOptions | undefined | null, model?: Model | undefined | null"
    )]
    pub fn new(env: Env, options: Option<Object>, model: Option<&crate::Model>) -> Result<Self> {
        let mut native = spars::EntityRulerOptions::default();
        let mut attribute = None;
        if let Some(options) = options {
            known_fields(
                &options,
                "EntityRuler option",
                &["overwriteEntities", "phraseAttribute", "idSeparator"],
            )?;
            let text =
                |value: Utf16String| errors::text(&value).map_err(|error| error.into_napi(env));
            if let Some(overwrite) =
                options.get_named_property::<Option<bool>>("overwriteEntities")?
            {
                native.overwrite_entities = overwrite;
            }
            attribute = options
                .get_named_property::<Option<Utf16String>>("phraseAttribute")?
                .map(text)
                .transpose()?;
            if let Some(separator) =
                options.get_named_property::<Option<Utf16String>>("idSeparator")?
            {
                native.id_separator = text(separator)?;
            }
        }
        let native = (|| -> errors::Result<spars::EntityRuler> {
            native.phrase_attribute = phrase_matcher::attribute(attribute.as_deref())?;
            Ok(match model {
                Some(model) => spars::EntityRuler::with_lexicon(native, model.inner.lexicon())?,
                None => spars::EntityRuler::new(native)?,
            })
        })()
        .map_err(|error| error.into_napi(env))?;
        Ok(Self {
            inner: Arc::new(native),
        })
    }

    /// The `overwriteEntities` option, or its default `false`. Options are fixed at creation.
    #[napi(getter)]
    pub fn overwrite_entities(&self) -> bool {
        self.inner.options().overwrite_entities
    }

    /// The `phraseAttribute` option, or its default `'ORTH'`.
    #[napi(getter, ts_return_type = "PhraseAttribute")]
    pub fn phrase_attribute(&self) -> &'static str {
        phrase_matcher::attribute_name(self.inner.options().phrase_attribute)
    }

    /// The `idSeparator` option, or its default `'||'`.
    #[napi(getter)]
    pub fn id_separator(&self) -> String {
        self.inner.options().id_separator.clone()
    }

    /// The number of added patterns, including duplicates and empty phrases, like spaCy's
    /// `len(ruler)`.
    #[napi(getter)]
    pub fn size(&self, env: Env) -> Result<u32> {
        errors::number(self.inner.len()).map_err(|error| error.into_napi(env))
    }

    /// Distinct labels of the current patterns in code-point order, like spaCy's `labels`.
    /// Includes `''` when a pattern has an empty label. A label containing `idSeparator` is
    /// listed split at its last separator.
    #[napi(getter, ts_return_type = "Array<EntityLabel | ''>")]
    pub fn labels(&self) -> Vec<String> {
        self.inner.labels()
    }

    /// Distinct IDs of the current patterns in code-point order, like spaCy's `ent_ids`. Includes
    /// `''` for a token pattern added with an empty ID. A label containing `idSeparator` is split
    /// at its last separator, so its tail is listed as an ID.
    #[napi(getter)]
    pub fn ids(&self) -> Vec<String> {
        self.inner.ids()
    }

    /// The current patterns, like spaCy's `patterns`: all token patterns, then all phrases as their
    /// document text. Within each kind, patterns with the same label and ID are listed together, in
    /// the order each pair first received a pattern of that kind; a pair removed and added again
    /// moves to the end. `id` is absent for a pattern without an ID or with an empty one. Each read
    /// returns a new array; changing it does not change the ruler.
    #[napi(getter, ts_return_type = "Array<RulerPattern>")]
    pub fn patterns(&self) -> Vec<RulerPattern> {
        self.inner
            .patterns()
            .into_iter()
            .map(|pattern| {
                let (tokens, phrase) = match pattern.pattern {
                    spars::RulerPatternKind::Tokens(tokens) => {
                        (Some(TokenPattern::from_native(&tokens)), None)
                    }
                    spars::RulerPatternKind::Phrase(text) => (None, Some(text)),
                };
                RulerPattern {
                    label: pattern.label,
                    id: pattern.id,
                    tokens,
                    phrase,
                }
            })
            .collect()
    }

    /// Add patterns, like spaCy's `add_patterns`. Patterns with the same label and ID form one rule;
    /// duplicates are kept and counted by `size`. Every pattern is checked before any is added, so
    /// a failure adds nothing. `SPARS_INVALID_PATTERN` is thrown for a non-array argument, an
    /// unknown field, a `phrase` that is not a `NativeDocument`, a `label` or
    /// `id` that is not a string, both or neither of `tokens` and `phrase`, an invalid token
    /// pattern, a lexical flag condition or flag-attribute phrase on a ruler created without a
    /// model, and a nonempty phrase document in which no token has the annotation
    /// `phraseAttribute` compares. A label or ID with an unpaired surrogate throws
    /// `SPARS_INVALID_TEXT`. Throws `SPARS_BUSY` while a `findMatches` or `annotate` call on this
    /// ruler is queued or running.
    #[napi(strict, ts_args_type = "patterns: Array<EntityPattern>")]
    pub fn add(&mut self, env: Env, patterns: Vec<Object>) -> Result<()> {
        let ruler = Arc::get_mut(&mut self.inner).ok_or_else(|| errors::busy().into_napi(env))?;
        let patterns = patterns
            .iter()
            .map(|object| Pattern::from_object(env, object))
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
        let native: Vec<spars::EntityPattern<'_>> = patterns
            .into_iter()
            .map(|pattern| spars::EntityPattern {
                label: pattern.label,
                id: pattern.id,
                pattern: match pattern.kind {
                    Kind::Tokens(tokens) => spars::EntityPatternKind::Tokens(tokens),
                    Kind::Phrase(_) => spars::EntityPatternKind::Phrase(
                        documents.next().expect("one document per phrase pattern"),
                    ),
                },
            })
            .collect();
        ruler
            .add(&native)
            .map_err(|error| errors::registration(error).into_napi(env))
    }

    /// Remove every token pattern and phrase added with this ID, under any label, like spaCy's
    /// `remove`. Throws `SPARS_INVALID_PATTERN` and changes nothing when no pattern was ever added
    /// with the ID, or when a label once used with it has no patterns left: as in spaCy, removed
    /// rules stay known until `clear`, so removing an ID twice fails, and so does removing an ID
    /// re-added under only some of its earlier labels. `remove('')` removes token patterns added
    /// with an empty ID. Unlike spaCy, a rule with both token patterns and phrases loses both (see
    /// docs/ENTITY_RULER.md, "Differences from spaCy"). An ID with an unpaired surrogate throws
    /// `SPARS_INVALID_TEXT`. Throws `SPARS_BUSY` while matching on this ruler is pending.
    #[napi(strict)]
    pub fn remove(&mut self, env: Env, id: Utf16String) -> Result<()> {
        let id = errors::text(&id).map_err(|error| error.into_napi(env))?;
        let ruler = Arc::get_mut(&mut self.inner).ok_or_else(|| errors::busy().into_napi(env))?;
        ruler
            .remove(&id)
            .map_err(|error| errors::BindingError::from(error).into_napi(env))
    }

    /// Remove every pattern, like spaCy's `clear`, keeping the options and model. Removed IDs are
    /// forgotten, so `remove` behaves as on a new ruler. Throws `SPARS_BUSY` while matching on this
    /// ruler is pending.
    #[napi]
    pub fn clear(&mut self, env: Env) -> Result<()> {
        let ruler = Arc::get_mut(&mut self.inner).ok_or_else(|| errors::busy().into_napi(env))?;
        ruler.clear();
        Ok(())
    }

    /// Find candidate entities, like spaCy's `EntityRuler.match`, on Node's worker pool: token and
    /// phrase matches without empty ones and duplicates, longest first, then earliest, and matches
    /// over the same tokens in the order their rules were added. Matches may overlap one another
    /// and existing entities; `annotate` applies that filtering. Rejects with `SPARS_INFERENCE` when
    /// an `equals` token condition on `lemma`, `pos`, `tag`, `dep` or `morphology` meets a nonempty
    /// document in which no token has that annotation (`''` counts as missing except for
    /// `morphology`), for example `lemma` before `Lemmatizer`, like spaCy's error E155; with
    /// `SPARS_UNSUPPORTED` when a token condition or a `MORPH` phrase attribute compares
    /// noncanonical document morphology; and with `SPARS_BUSY` or `SPARS_INPUT_LIMIT` under
    /// `configureExecution` and `configureInputLimits`. Throws
    /// `TypeError` when `doc` is not a `NativeDocument`.
    #[napi(strict, ts_return_type = "Promise<Array<EntityMatch>>")]
    pub fn find_matches(&self, doc: &NativeDocument) -> AsyncTask<EntityMatchTask> {
        AsyncTask::new(EntityMatchTask {
            ruler: Arc::clone(&self.inner),
            doc: Arc::clone(&doc.inner),
        })
    }

    /// Return a new document with the ruler's entities, like applying spaCy's ruler, on Node's
    /// worker pool; the given document and its views are unchanged. Matches are taken in
    /// `findMatches` order, overlapping ones are dropped, and existing entities are kept or
    /// removed according to `overwriteEntities`. Entities get their pattern's label. A pattern's
    /// nonempty ID is written to the entity and its tokens; without one, the tokens keep their
    /// current `entityId`, as in spaCy. Every token outside the resulting entities becomes outside
    /// (IOB `O`), including tokens with missing or blocked annotation, even when nothing matches. A
    /// match with an empty label claims its tokens but adds no entity. Rejects like `findMatches`.
    #[napi(strict, ts_return_type = "Promise<NativeDocument>")]
    pub fn annotate(&self, doc: &NativeDocument) -> AsyncTask<AnnotateTask> {
        AsyncTask::new(AnnotateTask {
            ruler: Arc::clone(&self.inner),
            doc: Arc::clone(&doc.inner),
        })
    }
}

pub struct EntityMatchTask {
    ruler: Arc<spars::EntityRuler>,
    doc: Arc<spars::Doc>,
}

#[napi]
impl Task for EntityMatchTask {
    type Output = errors::Result<Vec<EntityMatch>>;
    type JsValue = Vec<EntityMatch>;

    fn compute(&mut self) -> Result<Self::Output> {
        Ok(self
            .ruler
            .find_matches(&self.doc)
            .map_err(Into::into)
            .and_then(|matches| {
                matches
                    .into_iter()
                    .map(|matched| {
                        Ok(EntityMatch {
                            label: matched.label,
                            id: matched.id.unwrap_or_default(),
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

pub struct AnnotateTask {
    ruler: Arc<spars::EntityRuler>,
    doc: Arc<spars::Doc>,
}

#[napi]
impl Task for AnnotateTask {
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
