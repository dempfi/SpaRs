use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
mod entities;
mod traversal;
pub use entities::{EntityDefault, EntityUpdate, TokenRange};
pub use traversal::{Ancestors, Children, DependencyError, Subtree};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ByteOffset(pub usize);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodePointOffset(pub usize);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenIndex(pub usize);
/// One token of a [`Doc`] with its annotations, read through [`Doc::tokens`].
///
/// Code outside this crate cannot build a `Token` with a struct literal; call [`Token::new`] and
/// then assign the public fields. A token built this way is only useful in a native snapshot
/// passed to [`Doc::from_json`], which validates it. An `Option` annotation is `None` when no
/// pipeline component produced it.
// TODO(interned-strings): string annotations are owned per token. Store them once in a shared
// string table and read them through methods. See docs/PROGRESS.md#interned-token-strings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Token {
    /// Byte offset of the token's first byte in [`Doc::text`].
    pub start: ByteOffset,
    /// Byte offset just past the token's last byte (exclusive).
    pub end: ByteOffset,
    /// Code-point offset of `start`, spaCy's `Token.idx`.
    pub idx: CodePointOffset,
    /// Whether one ASCII space follows the token.
    pub whitespace: bool,
    /// spaCy's `Token.norm_`, the normalized form the tokenizer assigns.
    pub norm: String,
    /// spaCy's `Token.tag_`; `None` when the pipeline has no tagger.
    pub tag: Option<String>,
    /// spaCy's `Token.pos_`, a Universal POS tag or `""`; snapshots reject other values.
    pub pos: Option<String>,
    /// Morphological features in spaCy's `Feat=Value|...` form.
    pub morphology: Option<String>,
    /// spaCy's `Token.lemma_`; `None` when the pipeline has no lemmatizer.
    pub lemma: Option<String>,
    /// Index of the syntactic head; the root of a sentence is its own head. `None` when the
    /// document is unparsed.
    pub head: Option<TokenIndex>,
    /// spaCy's `Token.dep_`; `None` when the document is unparsed.
    pub dep: Option<String>,
    /// Whether the token starts a sentence; `None` when boundaries are unknown.
    pub sentence_start: Option<bool>,
    /// Entity IOB tag: `"B"`, `"I"`, `"O"`, or `None` when unknown. `"B"` without an entity type
    /// marks a token that can never be part of an entity.
    pub entity_iob: Option<String>,
    /// Label of the entity the token belongs to; `None` or empty outside an entity.
    pub entity_type: Option<String>,
    /// spaCy's `Token.ent_id_`, with `None` for the empty ID. It is independent of the IOB tag:
    /// like spaCy, a token keeps its ID when it stops being part of an entity. Only
    /// [`Doc::set_entities`] and [`Doc::with_entities`] set it; the named-entity recognizer never
    /// does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<String>,
}
impl Token {
    /// A token for document text bytes `start..end` (end exclusive), where `idx` is the
    /// code-point offset of `start` and `norm` its normalized form. `whitespace` starts `false`
    /// and every `Option` annotation starts `None`. Nothing is validated until
    /// [`Doc::from_json`].
    pub fn new(start: ByteOffset, end: ByteOffset, idx: CodePointOffset, norm: String) -> Self {
        Self {
            start,
            end,
            idx,
            whitespace: false,
            norm,
            tag: None,
            pos: None,
            morphology: None,
            lemma: None,
            head: None,
            dep: None,
            sentence_start: None,
            entity_iob: None,
            entity_type: None,
            entity_id: None,
        }
    }
}
/// A half-open token range `start..end` with a label. In a [`Doc`] it is an entity, a sentence
/// (empty label) or a noun chunk (`"NP"`). Code outside this crate cannot build a `Span` with a
/// struct literal; build one for [`EntityUpdate::entities`] with [`Span::new`], then assign public
/// fields such as `id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Span {
    pub start: TokenIndex,
    pub end: TokenIndex,
    pub label: String,
    /// spaCy's `Span.id_`. In a [`Doc`], `None` means no ID, an entity's ID is its first token's
    /// [`Token::entity_id`], and sentences and noun chunks never have one. In an
    /// [`EntityUpdate`], `None` or an empty ID leaves each token's current ID in place rather than
    /// clearing it, so the resulting entity can still report an earlier ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}
impl Span {
    /// A span over tokens `start..end` (end exclusive) without an ID. Set `id` afterwards to give
    /// an entity an ID. Bounds are checked when the span is used, for example by
    /// [`Doc::set_entities`].
    pub fn new(start: TokenIndex, end: TokenIndex, label: impl Into<String>) -> Self {
        Self {
            start,
            end,
            label: label.into(),
            id: None,
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct Doc {
    pub(crate) text: String,
    pub(crate) tokens: Vec<Token>,
    pub(crate) entities: Option<Vec<Span>>,
    pub(crate) sentences: Option<Vec<Span>>,
    pub(crate) noun_chunks: Option<Vec<Span>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) tensor: Vec<Vec<f32>>,
    // Public traversal begins after pipeline writes finish. Any future mutation
    // of dependency heads must invalidate this cache before exposing the document.
    #[serde(skip)]
    pub(crate) dependency_index:
        OnceLock<std::result::Result<traversal::DependencyIndex, DependencyError>>,
}
impl PartialEq for Doc {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text
            && self.tokens == other.tokens
            && self.entities == other.entities
            && self.sentences == other.sentences
            && self.noun_chunks == other.noun_chunks
            && self.tensor == other.tensor
    }
}
impl Doc {
    pub fn text(&self) -> &str {
        &self.text
    }
    /// Approximate heap memory owned by the document's text, tokens, spans and contextual
    /// tensor, in bytes. It counts allocated capacity, not the inline size of `Doc` itself, and
    /// excludes the dependency traversal index built on first use. It is a lower bound: the
    /// allocator's per-allocation overhead, significant for short strings, is not counted. Bindings report it to
    /// garbage-collected runtimes so they can account for native memory.
    pub fn estimated_heap_bytes(&self) -> usize {
        fn string(value: &Option<String>) -> usize {
            value.as_ref().map_or(0, String::capacity)
        }
        fn spans(spans: &Option<Vec<Span>>) -> usize {
            spans.as_ref().map_or(0, |spans| {
                spans.capacity() * std::mem::size_of::<Span>()
                    + spans
                        .iter()
                        .map(|span| span.label.capacity() + string(&span.id))
                        .sum::<usize>()
            })
        }
        let tokens = self.tokens.capacity() * std::mem::size_of::<Token>()
            + self
                .tokens
                .iter()
                .map(|token| {
                    token.norm.capacity()
                        + [
                            &token.tag,
                            &token.pos,
                            &token.morphology,
                            &token.lemma,
                            &token.dep,
                            &token.entity_iob,
                            &token.entity_type,
                            &token.entity_id,
                        ]
                        .into_iter()
                        .map(string)
                        .sum::<usize>()
                })
                .sum::<usize>();
        let tensor = self.tensor.capacity() * std::mem::size_of::<Vec<f32>>()
            + self
                .tensor
                .iter()
                .map(|row| row.capacity() * std::mem::size_of::<f32>())
                .sum::<usize>();
        self.text.capacity()
            + tokens
            + spans(&self.entities)
            + spans(&self.sentences)
            + spans(&self.noun_chunks)
            + tensor
    }
    pub fn tokens(&self) -> &[Token] {
        &self.tokens
    }
    /// Whether any token or entity carries an entity ID, which needs snapshot format v3.
    fn has_entity_ids(&self) -> bool {
        self.tokens.iter().any(|t| t.entity_id.is_some())
            || self.entities.iter().flatten().any(|e| e.id.is_some())
    }
    pub fn token_text(&self, i: TokenIndex) -> Result<&str> {
        let t = self.tokens.get(i.0).ok_or(Error::Bounds)?;
        Ok(&self.text[t.start.0..t.end.0])
    }
    pub fn span_text(&self, start: TokenIndex, end: TokenIndex) -> Result<&str> {
        if start.0 > end.0 || end.0 > self.tokens.len() {
            return Err(Error::Bounds);
        }
        if start == end {
            return Ok("");
        }
        Ok(&self.text[self.tokens[start.0].start.0..self.tokens[end.0 - 1].end.0])
    }
    pub fn entities(&self) -> Option<&[Span]> {
        self.entities.as_deref()
    }
    pub fn sentences(&self) -> Option<&[Span]> {
        self.sentences.as_deref()
    }
    pub fn noun_chunks(&self) -> Option<&[Span]> {
        self.noun_chunks.as_deref()
    }
    pub fn codepoint_offset(&self, byte: ByteOffset) -> Result<CodePointOffset> {
        self.text
            .get(..byte.0)
            .map(|s| CodePointOffset(s.chars().count()))
            .ok_or(Error::Bounds)
    }
    pub fn byte_offset(&self, point: CodePointOffset) -> Result<ByteOffset> {
        self.text
            .char_indices()
            .map(|(i, _)| i)
            .chain(std::iter::once(self.text.len()))
            .nth(point.0)
            .map(ByteOffset)
            .ok_or(Error::Bounds)
    }
}
/// spaCy 3.8.14's universal part-of-speech symbols (`spacy.parts_of_speech.IDS` without the
/// empty value).
pub(crate) const UNIVERSAL_POS: [&str; 20] = [
    "ADJ", "ADP", "ADV", "AUX", "CCONJ", "CONJ", "DET", "EOL", "INTJ", "NOUN", "NUM", "PART",
    "PRON", "PROPN", "PUNCT", "SCONJ", "SPACE", "SYM", "VERB", "X",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum SpanKind {
    Entity,
    Sentence,
    NounChunk,
}

impl Doc {
    /// Serialize an immutable document snapshot, preserving unavailable annotations.
    pub fn to_json(&self) -> Result<String> {
        // Each version adds a field older readers would otherwise ignore: v2 contextual vectors,
        // v3 entity IDs. A snapshot uses the oldest version that holds its data.
        let version = if self.has_entity_ids() {
            3
        } else if !self.tensor.is_empty() {
            2
        } else {
            1
        };
        Ok(serde_json::to_string(
            &serde_json::json!({"format_version": version, "document": self}),
        )?)
    }
    /// Restore a snapshot after validating offsets, token indices and spans.
    pub fn from_json(json: &str) -> Result<Self> {
        #[derive(Deserialize)]
        struct Snapshot {
            format_version: u32,
            document: Storage,
        }
        #[derive(Deserialize)]
        struct Storage {
            text: String,
            tokens: Vec<Token>,
            entities: Option<Vec<Span>>,
            sentences: Option<Vec<Span>>,
            noun_chunks: Option<Vec<Span>>,
            #[serde(default)]
            tensor: Vec<Vec<f32>>,
        }
        let s: Snapshot = serde_json::from_str(json)?;
        if ![1, 2, 3].contains(&s.format_version) {
            return Err(Error::Unsupported("document format version".into()));
        }
        if s.format_version == 1 && !s.document.tensor.is_empty() {
            return Err(Error::Unsupported(
                "contextual vectors require document format v2".into(),
            ));
        }
        let version = s.format_version;
        let s = s.document;
        // Entity IDs need format v3, so a reader that predates them cannot silently drop them.
        let check_id = |id: &Option<String>| -> Result<()> {
            match id.as_deref() {
                Some(_) if version < 3 => Err(Error::Unsupported(
                    "entity IDs require document format v3".into(),
                )),
                Some("") => Err(Error::Model(
                    "snapshot entity IDs must be omitted rather than empty".into(),
                )),
                _ => Ok(()),
            }
        };
        if !s.tensor.is_empty() {
            let width = s.tensor[0].len();
            if s.tensor.len() != s.tokens.len()
                || width == 0
                || width > 4096
                || s.tensor
                    .iter()
                    .any(|row| row.len() != width || row.iter().any(|x| !x.is_finite()))
            {
                return Err(Error::Model("invalid document tensor".into()));
            }
        }
        for t in &s.tokens {
            check_id(&t.entity_id)?;
            // Closed value sets, as spaCy stores them; `""` is spaCy's unset value.
            if t.entity_iob
                .as_deref()
                .is_some_and(|v| !matches!(v, "" | "B" | "I" | "O"))
            {
                return Err(Error::Model(
                    "snapshot entity IOB tags must be B, I, O or empty".into(),
                ));
            }
            if t.pos
                .as_deref()
                .is_some_and(|v| !v.is_empty() && !UNIVERSAL_POS.contains(&v))
            {
                return Err(Error::Model(
                    "snapshot POS tags must be spaCy universal POS tags".into(),
                ));
            }
        }
        let mut end = 0;
        let mut cp = 0;
        for (i, t) in s.tokens.iter().enumerate() {
            if t.start.0 != end
                || t.end.0 <= t.start.0
                || !s.text.is_char_boundary(t.start.0)
                || !s.text.is_char_boundary(t.end.0)
                || t.idx.0 != cp
                || t.head.is_some_and(|h| h.0 >= s.tokens.len())
            {
                return Err(Error::Bounds);
            }
            cp += s.text[t.start.0..t.end.0].chars().count();
            end = t.end.0;
            if t.whitespace {
                if s.text.as_bytes().get(end) != Some(&b' ') {
                    return Err(Error::Bounds);
                }
                end += 1;
                cp += 1
            }
            if let Some(next) = s.tokens.get(i + 1) {
                if next.start.0 != end {
                    return Err(Error::Bounds);
                }
            }
        }
        if end != s.text.len() {
            return Err(Error::Bounds);
        }
        for spans in [&s.entities, &s.sentences, &s.noun_chunks]
            .into_iter()
            .flatten()
        {
            let mut end = 0;
            for span in spans {
                if span.start.0 < end || span.start.0 >= span.end.0 || span.end.0 > s.tokens.len() {
                    return Err(Error::Bounds);
                }
                end = span.end.0;
            }
        }
        let labels = [
            (&s.entities, SpanKind::Entity),
            (&s.sentences, SpanKind::Sentence),
            (&s.noun_chunks, SpanKind::NounChunk),
        ];
        for (spans, kind) in labels {
            for span in spans.iter().flatten() {
                let valid = match kind {
                    SpanKind::Entity => !span.label.is_empty(),
                    SpanKind::Sentence => span.label.is_empty(),
                    SpanKind::NounChunk => span.label == "NP",
                };
                if !valid {
                    return Err(Error::Model(
                        "snapshot entities need a label, sentences none and noun chunks NP".into(),
                    ));
                }
                check_id(&span.id)?;
                // spaCy reads an entity's ID from its first token, so the two must agree; bounds
                // were checked above.
                let first = &s.tokens[span.start.0].entity_id;
                let consistent = match kind {
                    SpanKind::Entity => span.id == *first,
                    SpanKind::Sentence | SpanKind::NounChunk => span.id.is_none(),
                };
                if !consistent {
                    return Err(Error::Model(
                        "snapshot entity IDs must match their first token's, and sentences and noun chunks have none".into(),
                    ));
                }
            }
        }
        Ok(Self {
            text: s.text,
            tokens: s.tokens,
            entities: s.entities,
            sentences: s.sentences,
            noun_chunks: s.noun_chunks,
            tensor: s.tensor,
            dependency_index: OnceLock::new(),
        })
    }
}
/// Borrowed, checked token access without self-referential document storage.
#[derive(Clone, Copy)]
pub struct TokenView<'a> {
    doc: &'a Doc,
    index: TokenIndex,
}
impl<'a> TokenView<'a> {
    pub fn text(self) -> &'a str {
        self.doc.token_text(self.index).expect("checked token")
    }
    pub fn index(self) -> TokenIndex {
        self.index
    }
    pub fn annotations(self) -> &'a Token {
        &self.doc.tokens[self.index.0]
    }
    pub fn head(self) -> Option<Self> {
        self.annotations().head.map(|index| Self {
            doc: self.doc,
            index,
        })
    }
    pub fn span(self) -> SpanView<'a> {
        SpanView {
            doc: self.doc,
            start: self.index,
            end: TokenIndex(self.index.0 + 1),
        }
    }
}
/// Borrowed half-open token span; an empty span has empty text and a zero vector.
#[derive(Clone, Copy)]
pub struct SpanView<'a> {
    pub(crate) doc: &'a Doc,
    pub(crate) start: TokenIndex,
    pub(crate) end: TokenIndex,
}
impl<'a> SpanView<'a> {
    pub fn text(self) -> &'a str {
        self.doc
            .span_text(self.start, self.end)
            .expect("checked span")
    }
    pub fn start(self) -> TokenIndex {
        self.start
    }
    pub fn end(self) -> TokenIndex {
        self.end
    }
    pub fn tokens(self) -> impl Iterator<Item = TokenView<'a>> {
        (self.start.0..self.end.0).map(move |i| TokenView {
            doc: self.doc,
            index: TokenIndex(i),
        })
    }
}
impl Doc {
    pub fn token(&self, index: TokenIndex) -> Result<TokenView<'_>> {
        self.token_text(index)?;
        Ok(TokenView { doc: self, index })
    }
    pub fn span(&self, start: TokenIndex, end: TokenIndex) -> Result<SpanView<'_>> {
        self.span_text(start, end)?;
        Ok(SpanView {
            doc: self,
            start,
            end,
        })
    }
}
