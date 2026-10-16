//! Rule-based span groups and entities, following spaCy 3.8.14's `SpanRuler` (the `span_ruler`
//! factory).
use crate::phrase_matcher::rule_key;
use crate::{
    Doc, EntityDefault, EntityUpdate, Error, Lexicon, PhraseAttribute, PhraseMatcher, Result,
    RulerPatternKind, Span, SpanGroup, TokenIndex, TokenMatcher, TokenPattern,
};
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

/// How [`SpanRuler`] resolves overlapping spans, one of spaCy's registered span filters. A filter
/// compares the ruler's matches with the existing spans: the group under
/// [`SpanRulerOptions::spans_key`], or the document's entities. There are existing spans only when
/// [`SpanRulerOptions::overwrite`] is `false`. Matches are considered longest first, then by
/// earliest start, keeping their input order among equals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanFilter {
    /// `spacy.first_longest_spans_filter.v1` (`util.filter_chain_spans`). Existing spans and
    /// matches compete together, existing spans first among equals. A candidate is dropped when
    /// its first or last token is already taken; an empty span is dropped when the token at its
    /// position or the one before it is taken. The result is ordered by start.
    FirstLongest,
    /// `spacy.prioritize_new_ents_filter.v1`. A match is accepted when none of its tokens is
    /// taken by an accepted match; existing spans overlapping an accepted match are removed. The
    /// result lists the remaining existing spans in their order, then the accepted matches in
    /// acceptance order.
    PrioritizeNew,
    /// `spacy.prioritize_existing_ents_filter.v1`. Like [`SpanFilter::PrioritizeNew`], but the
    /// tokens of existing spans are taken from the start, so every existing span is kept.
    PrioritizeExisting,
}

/// Settings fixed when a [`SpanRuler`] is created, named after spaCy's `span_ruler` config and
/// defaulting to its factory defaults. Start from [`SpanRulerOptions::default`] and assign fields;
/// code outside this crate cannot use a struct literal.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SpanRulerOptions {
    /// spaCy's `spans_key`, default `Some("ruler")`: the [`Doc::span_groups`] key the ruler
    /// writes. `None` or, as in spaCy, an empty key writes no span group.
    pub spans_key: Option<String>,
    /// spaCy's `spans_filter`, default `None`: every match is added to the span group, including
    /// overlapping ones. With a filter, the group receives the filter's result for the existing
    /// group and the matches; see [`SpanRulerOptions::overwrite`] for what that does to an existing
    /// group.
    pub spans_filter: Option<SpanFilter>,
    /// spaCy's `annotate_ents`, default `false`: whether the ruler also writes entities with
    /// [`Doc::set_entities`], using [`SpanRulerOptions::entities_filter`].
    pub annotate_entities: bool,
    /// spaCy's `ents_filter`, default [`SpanFilter::FirstLongest`]. [`SpanFilter::PrioritizeNew`]
    /// reproduces the EntityRuler's `overwrite_ents=True` and [`SpanFilter::PrioritizeExisting`]
    /// its default. Existing entities take part only when [`SpanRulerOptions::overwrite`] is
    /// `false`.
    pub entities_filter: SpanFilter,
    /// spaCy's `phrase_matcher_attr`: the token value phrase patterns compare, default
    /// [`PhraseAttribute::Orth`] (exact token text). Token patterns are unaffected. A lexical flag
    /// attribute needs [`SpanRuler::with_lexicon`]; without a lexicon every phrase addition fails.
    pub phrase_attribute: PhraseAttribute,
    /// spaCy's `overwrite`, default `true` (the `span_ruler` factory default). When `true`, the
    /// span group under [`SpanRulerOptions::spans_key`] is replaced by a new group named after the
    /// key, without attrs, and existing entities are discarded before filtering. When `false`, an
    /// existing group keeps its name and attrs and is extended, and existing entities compete
    /// with the matches in [`SpanRulerOptions::entities_filter`]; a document without entity
    /// annotation contributes none.
    ///
    /// As in spaCy, extending an existing group with a [`SpanRulerOptions::spans_filter`] appends
    /// the filter's whole result, which includes the existing spans it keeps, so those spans
    /// appear in the group twice.
    pub overwrite: bool,
}
impl Default for SpanRulerOptions {
    fn default() -> Self {
        Self {
            spans_key: Some("ruler".into()),
            spans_filter: None,
            annotate_entities: false,
            entities_filter: SpanFilter::FirstLongest,
            phrase_attribute: PhraseAttribute::Orth,
            overwrite: true,
        }
    }
}

/// What a [`SpanPattern`] matches: token conditions or a phrase.
#[derive(Debug, Clone)]
pub enum SpanPatternKind<'a> {
    /// A sequence of token conditions, matched as by [`TokenMatcher`]; spaCy's list-of-dicts
    /// pattern. Lexical flag conditions need [`SpanRuler::with_lexicon`].
    Tokens(TokenPattern),
    /// A phrase. spaCy processes phrase text with the pipeline components that run before the
    /// ruler, so pass a document processed by the same model stages as the documents the ruler
    /// will annotate. Only [`SpanRulerOptions::phrase_attribute`] is compared; for lemma, POS,
    /// tag, dependency and morphology the document needs a token with that annotation. An empty
    /// document is accepted and counted but never matches. The ruler copies what it needs.
    Phrase(&'a Doc),
}

/// One pattern for [`SpanRuler::add`], like an entry of spaCy's `add_patterns`.
#[derive(Debug, Clone)]
pub struct SpanPattern<'a> {
    /// The label of matched spans. An empty label is accepted: its matches are added to span
    /// groups and take part in filtering, but [`Doc::set_entities`] adds no entity for them.
    pub label: String,
    /// The ID written to matched spans and entities, spaCy's pattern `id`. `None` and `Some("")`
    /// match the same way and give matches no ID, but only `Some("")` is listed by
    /// [`SpanRuler::ids`] and removed by [`SpanRuler::remove_by_id`] with `""`. With
    /// [`SpanRulerOptions::annotate_entities`], an entity from a pattern without an ID writes no
    /// ID, so its tokens keep any entity ID they already had.
    pub id: Option<String>,
    pub pattern: SpanPatternKind<'a>,
}

/// A pattern as [`SpanRuler::patterns`] reports it, like an entry of spaCy's
/// `SpanRuler.patterns`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SpanRulerPattern {
    pub label: String,
    /// The ID exactly as added: `None` for a pattern added without one, `Some("")` for the empty
    /// ID.
    pub id: Option<String>,
    pub pattern: RulerPatternKind,
}
impl SpanRulerPattern {
    /// A pattern without an ID, for comparison with [`SpanRuler::patterns`]; assign `id`
    /// afterwards for one with an ID. A phrase is listed by its pattern document's full text.
    pub fn new(label: impl Into<String>, pattern: RulerPatternKind) -> Self {
        Self {
            label: label.into(),
            id: None,
            pattern,
        }
    }
}

/// The label and ID one matcher rule gives its matches, with the string-store hashes spaCy sorts
/// identical spans by.
struct Rule {
    label: String,
    id: String,
    label_key: u64,
    id_key: u64,
}

/// Adds span groups and, optionally, entities from token and phrase patterns, like spaCy's
/// `SpanRuler` (the `span_ruler` factory). Apply it to processed documents; it does not run
/// inside a model's pipeline. `docs/SPAN_RULER.md` has a runnable example.
///
/// Each annotation finds the matches ([`SpanRuler::find_matches`]), then writes the span group
/// under [`SpanRulerOptions::spans_key`] and, with [`SpanRulerOptions::annotate_entities`], the
/// entities. When entities are written, every token outside the resulting entities becomes
/// outside (IOB `O`), including tokens whose entity annotation was missing or blocked.
pub struct SpanRuler {
    options: SpanRulerOptions,
    lexicon: Option<Lexicon>,
    tokens: TokenMatcher,
    phrases: PhraseMatcher,
    patterns: Vec<SpanRulerPattern>,
    // Every label and ID pair ever added, by matcher rule name. Like spaCy's
    // `_match_label_id_map`, entries stay after removal and `clear`.
    rules: HashMap<String, Rule>,
    known_labels: HashSet<String>,
}

impl SpanRuler {
    /// A ruler without a lexicon. Token patterns with lexical flag conditions and lexical flag
    /// phrase attributes need [`SpanRuler::with_lexicon`].
    pub fn new(options: SpanRulerOptions) -> Self {
        Self::build(options, None)
    }

    /// A ruler that evaluates lexical flags with a model's language rules; obtain the lexicon with
    /// [`crate::Model::lexicon`].
    pub fn with_lexicon(options: SpanRulerOptions, lexicon: Lexicon) -> Self {
        Self::build(options, Some(lexicon))
    }

    fn build(options: SpanRulerOptions, lexicon: Option<Lexicon>) -> Self {
        let (tokens, phrases) = matchers(options.phrase_attribute, lexicon.as_ref());
        Self {
            options,
            lexicon,
            tokens,
            phrases,
            patterns: Vec::new(),
            rules: HashMap::new(),
            known_labels: HashSet::new(),
        }
    }

    /// The options given at creation. They cannot be changed afterwards, and [`SpanRuler::clear`]
    /// keeps them.
    pub fn options(&self) -> &SpanRulerOptions {
        &self.options
    }

    /// The number of patterns, counting duplicates and empty phrases, like spaCy's `len`.
    pub fn len(&self) -> usize {
        self.patterns.len()
    }

    /// Whether no patterns are listed. Labels stay known after removal and [`SpanRuler::clear`],
    /// so [`SpanRuler::contains`] can still return `true` for an empty ruler.
    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    /// Sorted distinct labels of the current patterns, like spaCy's `SpanRuler.labels`. Labels
    /// whose patterns were all removed are not listed, unlike [`SpanRuler::contains`].
    pub fn labels(&self) -> Vec<String> {
        sorted(self.patterns.iter().map(|p| p.label.as_str()).collect())
    }

    /// Sorted distinct IDs of the current patterns, like spaCy's `SpanRuler.ids`. A pattern added
    /// with `Some("")` contributes `""`; one added without an ID contributes nothing.
    pub fn ids(&self) -> Vec<String> {
        sorted(
            (self.patterns.iter())
                .filter_map(|p| p.id.as_deref())
                .collect(),
        )
    }

    /// The patterns in the order they were added, like spaCy's `SpanRuler.patterns`. Phrases are
    /// listed by their document text.
    pub fn patterns(&self) -> &[SpanRulerPattern] {
        &self.patterns
    }

    /// Whether `label` was ever added, like spaCy's `label in ruler`. As in spaCy, a label stays
    /// known after [`SpanRuler::remove`], [`SpanRuler::remove_by_id`] and [`SpanRuler::clear`]
    /// remove its patterns.
    pub fn contains(&self, label: &str) -> bool {
        self.known_labels.contains(label)
    }

    /// Add patterns, like spaCy's `add_patterns`. Patterns with the same label and ID add to one
    /// rule; duplicates are kept and counted by [`SpanRuler::len`]. Every pattern is checked
    /// before the ruler changes, so on error nothing is added:
    ///
    /// - an invalid token pattern, or a lexical flag condition without a lexicon, returns
    ///   [`Error::Pattern`];
    /// - any phrase when [`SpanRulerOptions::phrase_attribute`] is a lexical flag and the ruler
    ///   has no lexicon returns [`Error::Pattern`];
    /// - a non-empty phrase document in which no token has the annotation the phrase attribute
    ///   compares returns [`Error::MissingAnnotation`];
    /// - with [`PhraseAttribute::Morph`], a phrase document whose morphology is not in canonical
    ///   `Field=Value|…` form returns [`Error::Unsupported`].
    pub fn add(&mut self, patterns: &[SpanPattern<'_>]) -> Result<()> {
        let names: Vec<String> = patterns.iter().map(rule_name).collect();
        let mut keys = HashMap::new();
        for (name, pattern) in names.iter().zip(patterns) {
            match &pattern.pattern {
                SpanPatternKind::Tokens(tokens) => {
                    self.tokens.check(name, std::slice::from_ref(tokens))?
                }
                SpanPatternKind::Phrase(doc) => {
                    self.phrases.check(name, &[doc])?;
                    // `check` compares a new rule name only with existing rules.
                    if keys
                        .insert(rule_key(name), name.as_str())
                        .is_some_and(|other| other != name)
                    {
                        return Err(Error::Pattern("phrase rule hash collision".into()));
                    }
                }
            }
        }
        for (name, pattern) in names.into_iter().zip(patterns) {
            let listed = match &pattern.pattern {
                SpanPatternKind::Tokens(tokens) => {
                    self.tokens
                        .add(name.clone(), vec![tokens.clone()])
                        .expect("checked token pattern");
                    RulerPatternKind::Tokens(tokens.clone())
                }
                SpanPatternKind::Phrase(doc) => {
                    self.phrases
                        .add(name.as_str(), &[doc])
                        .expect("checked phrase pattern");
                    RulerPatternKind::Phrase(doc.text().to_owned())
                }
            };
            let id = pattern.id.clone().unwrap_or_default();
            self.rules.entry(name).or_insert_with(|| Rule {
                label_key: rule_key(&pattern.label),
                id_key: rule_key(&id),
                label: pattern.label.clone(),
                id,
            });
            self.known_labels.insert(pattern.label.clone());
            self.patterns.push(SpanRulerPattern {
                label: pattern.label.clone(),
                id: pattern.id.clone(),
                pattern: listed,
            });
        }
        Ok(())
    }

    /// Remove every pattern with `label`, like spaCy's `remove`. Returns [`Error::Pattern`] and
    /// changes nothing when `label` was never added; a label whose patterns were already removed
    /// or cleared is still known (see [`SpanRuler::contains`]), so removing it again succeeds and
    /// changes nothing.
    pub fn remove(&mut self, label: &str) -> Result<()> {
        if !self.contains(label) {
            return Err(Error::Pattern(format!(
                "no span pattern has the label {label:?}"
            )));
        }
        self.patterns.retain(|p| p.label != label);
        self.unregister(|rule| rule.label == label);
        Ok(())
    }

    /// Remove every pattern added with `Some(id)`, like spaCy's `remove_by_id`. Returns
    /// [`Error::Pattern`] and changes nothing when no current pattern has that ID.
    ///
    /// As in spaCy, `remove_by_id("")` also stops every pattern added without an ID from matching,
    /// whatever its label, because `None` and `Some("")` share the empty ID, while
    /// [`SpanRuler::patterns`], [`SpanRuler::len`] and [`SpanRuler::labels`] still list those
    /// patterns. Adding a pattern with the same label and no ID makes only the new one match.
    pub fn remove_by_id(&mut self, id: &str) -> Result<()> {
        let before = self.patterns.len();
        self.patterns.retain(|p| p.id.as_deref() != Some(id));
        if self.patterns.len() == before {
            return Err(Error::Pattern(format!("no span pattern has the ID {id:?}")));
        }
        self.unregister(|rule| rule.id == id);
        Ok(())
    }

    /// Remove every pattern, like spaCy's `clear`. The options, lexicon and known labels are kept.
    pub fn clear(&mut self) {
        (self.tokens, self.phrases) =
            matchers(self.options.phrase_attribute, self.lexicon.as_ref());
        self.patterns.clear();
    }

    /// The ruler's matches before filtering, like spaCy's `SpanRuler.match`: token and phrase
    /// matches without empty ones and without repeated matches of the same label and ID over the
    /// same tokens, ordered by start token, then end token. Matches over the same tokens are
    /// ordered by spaCy's string-store hashes of the label, then of the ID; that order is not
    /// alphabetical (spaCy symbol labels such as `ORG` come first), and it decides which of two
    /// identical spans a filter keeps. Matches may overlap.
    ///
    /// Returns [`Error::MissingAnnotation`] when a token pattern's `Equals` condition compares
    /// lemma, POS, tag, dependency label or morphology and no token of a nonempty `doc` has it,
    /// like spaCy's error E155 (see [`TokenMatcher::find_matches`]), and [`Error::Unsupported`]
    /// when the document has non-canonical morphology and a token pattern or the phrase
    /// attribute compares morphology.
    pub fn find_matches(&self, doc: &Doc) -> Result<Vec<Span>> {
        let tokens = self.tokens.find_matches(doc)?;
        let phrases = self.phrases.find_matches(doc)?;
        let mut found: Vec<(usize, usize, &Rule)> = tokens
            .iter()
            .map(|m| (m.rule.as_str(), m.start.0, m.end.0))
            .chain(
                phrases
                    .iter()
                    .map(|m| (m.rule.0.as_str(), m.start.0, m.end.0)),
            )
            .filter(|&(_, start, end)| start != end)
            .map(|(name, start, end)| (start, end, &self.rules[name]))
            .collect();
        let order =
            |&(start, end, rule): &(usize, usize, &Rule)| (start, end, rule.label_key, rule.id_key);
        found.sort_unstable_by_key(order);
        found.dedup_by_key(|m| order(m));
        Ok(found
            .into_iter()
            .map(|(start, end, rule)| {
                let mut span = Span::new(TokenIndex(start), TokenIndex(end), rule.label.as_str());
                span.id = (!rule.id.is_empty()).then(|| rule.id.clone());
                span
            })
            .collect())
    }

    /// Write the span group and, if enabled, the entities to `doc`, like calling spaCy's
    /// `SpanRuler` on it. Errors are those of [`SpanRuler::find_matches`]; with
    /// [`SpanRulerOptions::annotate_entities`], [`Error::Unsupported`] is also returned for a
    /// token IOB tag other than `B`, `I`, `O` or missing, which only direct edits to token fields
    /// can produce. On error `doc` is unchanged, including its span groups.
    pub fn annotate(&self, doc: &mut Doc) -> Result<()> {
        let matches = self.find_matches(doc)?;
        let group = self.span_group(doc, &matches);
        if self.options.annotate_entities {
            doc.set_entities(&self.entity_update(doc, &matches))?;
        }
        if let Some((key, group)) = group {
            doc.set_span_group(key, group)?;
        }
        Ok(())
    }

    /// Return an annotated copy, leaving `doc` unchanged, as [`SpanRuler::annotate`] would
    /// annotate it.
    pub fn annotated(&self, doc: &Doc) -> Result<Doc> {
        let mut copy = doc.clone();
        self.annotate(&mut copy)?;
        Ok(copy)
    }

    /// Remove every registered rule `selected` picks from both matchers.
    fn unregister(&mut self, selected: impl Fn(&Rule) -> bool) {
        let names: HashSet<&str> = (self.rules.iter())
            .filter(|(_, rule)| selected(rule))
            .map(|(name, _)| name.as_str())
            .collect();
        // One pass over the token rules: a label can select thousands of rules, one per ID.
        self.tokens.remove_all(&names);
        for name in names {
            if self.phrases.contains(name) {
                self.phrases.remove(name).expect("registered phrase rule");
            }
        }
    }

    /// spaCy's `set_annotations` for `doc.spans`: the key and group to store, if any.
    fn span_group<'k>(&'k self, doc: &Doc, matches: &[Span]) -> Option<(&'k str, SpanGroup)> {
        let key = self
            .options
            .spans_key
            .as_deref()
            .filter(|key| !key.is_empty())?;
        let existing = (!self.options.overwrite)
            .then(|| doc.span_groups().get(key))
            .flatten();
        let added = match self.options.spans_filter {
            Some(filter) => filter.apply(
                existing.map_or(&[][..], |group| &group.spans),
                matches,
                doc.tokens().len(),
            ),
            None => matches.to_vec(),
        };
        let group = match existing {
            Some(group) => {
                let mut group = group.clone();
                group.spans.extend(added);
                group
            }
            None => SpanGroup::new(key, added),
        };
        Some((key, group))
    }

    /// spaCy's `set_annotations` for `doc.ents`: the entity list the ruler assigns.
    fn entity_update(&self, doc: &Doc, matches: &[Span]) -> EntityUpdate {
        let existing = if self.options.overwrite {
            &[][..]
        } else {
            doc.entities().unwrap_or_default()
        };
        // spaCy sorts the spans before assigning them; `set_entities` does not depend on their
        // order, and the filters never return overlaps.
        let entities = (self.options.entities_filter).apply(existing, matches, doc.tokens().len());
        EntityUpdate {
            entities,
            default: EntityDefault::Outside,
            ..EntityUpdate::default()
        }
    }
}

impl SpanFilter {
    /// The filtered spans for `existing` spans and new `spans` over a document of `length` tokens.
    fn apply(self, existing: &[Span], spans: &[Span], length: usize) -> Vec<Span> {
        match self {
            SpanFilter::FirstLongest => first_longest(existing, spans, length),
            SpanFilter::PrioritizeNew => prioritized(existing, spans, length, false),
            SpanFilter::PrioritizeExisting => prioritized(existing, spans, length, true),
        }
    }
}

/// Longest first, then earliest; the stable sort keeps the input order among equals.
fn by_priority<'s>(spans: impl Iterator<Item = &'s Span>) -> Vec<&'s Span> {
    let mut spans: Vec<&Span> = spans.collect();
    spans.sort_by_key(|span| {
        (
            Reverse(span.end.0.saturating_sub(span.start.0)),
            span.start.0,
        )
    });
    spans
}

/// spaCy's `filter_chain_spans`, which checks only each candidate's first and last token.
fn first_longest(existing: &[Span], spans: &[Span], length: usize) -> Vec<Span> {
    let mut seen = vec![false; length];
    let mut kept = Vec::new();
    for span in by_priority(existing.iter().chain(spans)) {
        let taken = |token: Option<usize>| token.and_then(|t| seen.get(t)).copied() == Some(true);
        if !taken(Some(span.start.0)) && !taken(span.end.0.checked_sub(1)) {
            seen[span.start.0..span.end.0].fill(true);
            kept.push(span.clone());
        }
    }
    kept.sort_by_key(|span| span.start.0);
    kept
}

/// spaCy's `prioritize_new_ents_filter` and `prioritize_existing_ents_filter`.
fn prioritized(existing: &[Span], spans: &[Span], length: usize, keep_existing: bool) -> Vec<Span> {
    // The 1-based acceptance number of the match owning each token, or 0. With `keep_existing`,
    // existing spans own their tokens with `usize::MAX`.
    let mut owner = vec![0; length];
    if keep_existing {
        for span in existing {
            owner[span.start.0..span.end.0].fill(usize::MAX);
        }
    }
    let mut added = Vec::new();
    for span in by_priority(spans.iter()) {
        let tokens = &mut owner[span.start.0..span.end.0];
        if tokens.iter().all(|&o| o == 0) {
            added.push(span.clone());
            tokens.fill(added.len());
        }
    }
    let kept = existing.iter().filter(|e| {
        if keep_existing {
            return true;
        }
        // spaCy removes an existing span when `e.start < end and e.end > start` for an accepted
        // match: for a non-empty span, when one of its tokens is taken; for an empty span, when
        // it lies strictly inside one accepted match.
        let (start, end) = (e.start.0, e.end.0);
        if start < end {
            owner[start..end].iter().all(|&o| o == 0)
        } else {
            !(start > 0
                && start < length
                && owner[start - 1] != 0
                && owner[start - 1] == owner[start])
        }
    });
    kept.cloned().chain(added).collect()
}

/// A rule name for each label and ID pair, like spaCy's `repr((label, id))` key: injective, with
/// a missing ID the same as the empty one.
fn rule_name(pattern: &SpanPattern<'_>) -> String {
    serde_json::to_string(&(&pattern.label, pattern.id.as_deref().unwrap_or_default()))
        .expect("strings serialize")
}

fn matchers(
    attribute: PhraseAttribute,
    lexicon: Option<&Lexicon>,
) -> (TokenMatcher, PhraseMatcher) {
    match lexicon {
        Some(lexicon) => (
            TokenMatcher::with_lexicon(lexicon.clone()),
            PhraseMatcher::with_lexicon(attribute, lexicon.clone()),
        ),
        None => (
            TokenMatcher::new(),
            PhraseMatcher::with_attribute(attribute),
        ),
    }
}

fn sorted(values: HashSet<&str>) -> Vec<String> {
    let mut values: Vec<String> = values.into_iter().map(str::to_owned).collect();
    values.sort();
    values
}

#[cfg(test)]
mod tests;
