use napi_derive::napi;

/// A plain copy of a processed document, returned by `Model.process`, `processBatch` and
/// `NativeDocument.toObject`. Span lists are in document order.
#[napi(object, object_from_js = false, use_nullable = true)]
pub struct Document {
    pub text: String,
    pub tokens: Vec<Token>,
    #[napi(ts_type = "Array<Span<EntityLabel>>")]
    /// Null until `Ner` runs, or until `NativeDocument.withEntities` or `EntityRuler.annotate` sets
    /// entities; null again if an update leaves every token's entity annotation missing.
    pub entities: Option<Vec<Span>>,
    #[napi(ts_type = "Array<Span<'', ''>>")]
    /// Null until `Parser` runs.
    pub sentences: Option<Vec<Span>>,
    #[napi(ts_type = "Array<Span<'NP', ''>>")]
    /// Null until `AttributeRuler` runs.
    pub noun_chunks: Option<Vec<Span>>,
}

/// One token of `Document.tokens`. Offsets index `Document.text`; each `*End` is exclusive and
/// excludes `whitespace`. An annotation is null when nothing assigned it: the component that sets it
/// did not run (see `Stage`) or assigned no value to this token. `''` is spaCy's empty value.
#[napi(object, object_from_js = false, use_nullable = true)]
pub struct Token {
    #[napi(ts_type = "import('./units.js').TokenIndex")]
    pub index: u32,
    pub text: String,
    /// Exactly one trailing ASCII space, or empty. Other whitespace has its own token.
    #[napi(ts_type = "'' | ' '")]
    pub whitespace: String,
    #[napi(ts_type = "import('./units.js').ByteOffset")]
    pub byte_start: u32,
    #[napi(ts_type = "import('./units.js').ByteOffset")]
    pub byte_end: u32,
    #[napi(ts_type = "import('./units.js').CodePointOffset")]
    pub code_point_start: u32,
    #[napi(ts_type = "import('./units.js').CodePointOffset")]
    pub code_point_end: u32,
    #[napi(ts_type = "import('./units.js').Utf16Offset")]
    pub utf16_start: u32,
    #[napi(ts_type = "import('./units.js').Utf16Offset")]
    pub utf16_end: u32,
    pub norm: String,
    #[napi(ts_type = "FineGrainedTag | ''")]
    pub tag: Option<String>,
    #[napi(ts_type = "UniversalPos | ''")]
    pub pos: Option<String>,
    /// spaCy `Field=Value|...` features; `''` means no features.
    pub morphology: Option<String>,
    pub lemma: Option<String>,
    /// Index of the syntactic head; a root is its own head. Null before `Parser`.
    #[napi(ts_type = "import('./units.js').TokenIndex")]
    pub head: Option<u32>,
    #[napi(ts_type = "DependencyLabel | ''")]
    pub dep: Option<String>,
    /// Whether a sentence begins at this token; null before `Parser`.
    pub sentence_start: Option<bool>,
    #[napi(ts_type = "EntityIob | ''")]
    pub entity_iob: Option<String>,
    #[napi(ts_type = "EntityLabel | ''")]
    pub entity_type: Option<String>,
    /// spaCy's `Token.ent_id_`, or `''` without one. Set only by `withEntities` and
    /// `EntityRuler.annotate`, never by the recognizer; a token keeps its ID when it stops being part
    /// of an entity, as in spaCy.
    pub entity_id: String,
}

/// A half-open token interval: start is included; end is excluded.
#[napi(object, object_from_js = false)]
pub struct Span {
    #[napi(ts_type = "import('./units.js').TokenIndex")]
    pub start: u32,
    #[napi(ts_type = "import('./units.js').TokenIndex")]
    pub end: u32,
    pub label: String,
    /// spaCy's `Span.id_`, or `''` without one; only entities carry IDs.
    pub id: String,
}
