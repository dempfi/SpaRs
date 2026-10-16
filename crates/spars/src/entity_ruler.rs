//! Rule-based entity annotation, following spaCy 3.8.14's `EntityRuler` (the `entity_ruler`
//! factory).
use crate::{
    Doc, EntityDefault, EntityUpdate, Error, Lexicon, PhraseAttribute, PhraseMatcher, Result, Span,
    TokenIndex, TokenMatcher, TokenPattern,
};
use std::collections::{HashMap, HashSet};

/// Settings fixed when an [`EntityRuler`] is created, named after spaCy's `entity_ruler` config.
/// Start from [`EntityRulerOptions::default`] and assign fields; code outside this crate cannot use
/// a struct literal.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct EntityRulerOptions {
    /// spaCy's `overwrite_ents`, default `false`. When `false`, a match is skipped if any of its
    /// tokens already has an entity type; blocked tokens (IOB `B` without a type) have none. When
    /// `true`, every entity already in the document that an accepted match overlaps is removed
    /// whole.
    pub overwrite_entities: bool,
    /// spaCy's `phrase_matcher_attr`: the token value phrase patterns compare, default
    /// [`PhraseAttribute::Orth`] (exact token text). Token patterns are unaffected. A lexical flag
    /// attribute needs [`EntityRuler::with_lexicon`]; without a lexicon every phrase addition
    /// fails.
    pub phrase_attribute: PhraseAttribute,
    /// spaCy's `ent_id_sep`, default `"||"`; [`EntityRuler::new`] rejects an empty separator. A
    /// pattern with an ID is stored under the rule name `label + id_separator + id`, and
    /// [`EntityRuler::labels`], [`EntityRuler::ids`] and [`EntityRuler::patterns`] split rule names
    /// at the last separator, as spaCy does. A label or ID containing the separator can therefore
    /// be reported split differently from how it was added, and two label and ID pairs that form
    /// the same rule name share one rule.
    pub id_separator: String,
}
impl Default for EntityRulerOptions {
    fn default() -> Self {
        Self {
            overwrite_entities: false,
            phrase_attribute: PhraseAttribute::Orth,
            id_separator: "||".into(),
        }
    }
}

/// The rule of an [`EntityPattern`].
#[derive(Debug, Clone)]
pub enum EntityPatternKind<'a> {
    /// A sequence of token conditions, matched as by [`TokenMatcher`]; spaCy's list-of-dicts
    /// pattern. Lexical flag conditions need [`EntityRuler::with_lexicon`].
    Tokens(TokenPattern),
    /// A phrase. spaCy processes phrase text with the pipeline components that run before the
    /// ruler, so pass a document processed by the same model stages as the documents the ruler
    /// will annotate. Only [`EntityRulerOptions::phrase_attribute`] is compared; for lemma, POS,
    /// tag, dependency and morphology the document needs a token with that annotation. An empty
    /// document is accepted and counted but never matches. The ruler copies what it needs.
    Phrase(&'a Doc),
}

/// One pattern for [`EntityRuler::add`], like an entry of spaCy's `add_patterns`.
#[derive(Debug, Clone)]
pub struct EntityPattern<'a> {
    /// The label of matched entities. As in spaCy, an empty label is accepted: its matches still
    /// claim their tokens and remove overwritten entities, but no entity is added for them.
    pub label: String,
    /// The ID written to matched entities, spaCy's pattern `id`, and the key for
    /// [`EntityRuler::remove`]. `None` gives matches no ID. On a phrase, `Some("")` is the same as
    /// `None`. On a token pattern, `Some("")` also gives matches no ID, but as in spaCy
    /// [`EntityRuler::ids`] lists `""` and `remove("")` removes the pattern.
    pub id: Option<String>,
    pub pattern: EntityPatternKind<'a>,
}

/// The stored rule of a [`RulerPattern`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RulerPatternKind {
    Tokens(TokenPattern),
    /// The full text of the phrase's pattern document, including whitespace.
    Phrase(String),
}

/// A pattern as [`EntityRuler::patterns`] reports it, like spaCy's `EntityRuler.patterns`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct RulerPattern {
    /// The label read back from the pattern's rule name; see
    /// [`EntityRulerOptions::id_separator`] for labels containing the separator.
    pub label: String,
    /// `None` when the pattern has no ID or an empty one.
    pub id: Option<String>,
    pub pattern: RulerPatternKind,
}
impl RulerPattern {
    /// A pattern without an ID, for comparison with [`EntityRuler::patterns`].
    pub fn new(label: impl Into<String>, pattern: RulerPatternKind) -> Self {
        Self {
            label: label.into(),
            id: None,
            pattern,
        }
    }
}

/// A candidate entity found by [`EntityRuler::find_matches`], over tokens `start..end` (end
/// exclusive).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct EntityMatch {
    pub label: String,
    /// `None` when the matching pattern has no ID or an empty one.
    pub id: Option<String>,
    pub start: TokenIndex,
    pub end: TokenIndex,
}
impl EntityMatch {
    /// A match without an ID, for comparison with [`EntityRuler::find_matches`].
    pub fn new(label: impl Into<String>, start: TokenIndex, end: TokenIndex) -> Self {
        Self {
            label: label.into(),
            id: None,
            start,
            end,
        }
    }
}

/// Rule names with their patterns, in the order each name first received a pattern of this kind,
/// as spaCy's pattern dictionaries keep them.
struct Rules<T> {
    entries: Vec<(String, Vec<T>)>,
    index: HashMap<String, usize>,
}
impl<T> Default for Rules<T> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            index: HashMap::new(),
        }
    }
}
impl<T> Rules<T> {
    fn push(&mut self, name: &str, value: T) {
        let index = match self.index.get(name) {
            Some(&index) => index,
            None => {
                self.index.insert(name.to_owned(), self.entries.len());
                self.entries.push((name.to_owned(), Vec::new()));
                self.entries.len() - 1
            }
        };
        self.entries[index].1.push(value);
    }
    fn remove(&mut self, names: &HashSet<String>) {
        self.entries.retain(|(name, _)| !names.contains(name));
        self.index = (self.entries.iter().enumerate())
            .map(|(i, (name, _))| (name.clone(), i))
            .collect();
    }
    fn count(&self) -> usize {
        self.entries.iter().map(|(_, values)| values.len()).sum()
    }
    fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|(name, _)| name.as_str())
    }
}

/// Adds entities from token and phrase patterns, like spaCy's `EntityRuler` added after the
/// named-entity recognizer. Apply it to documents you have already processed; to use it without
/// the recognizer, process with [`crate::Model::process_until`] and a stage before
/// [`crate::Stage::Ner`]. It does not run inside a model's pipeline. `docs/ENTITY_RULER.md` has a
/// runnable example.
///
/// Matches are filtered and written with [`Doc::set_entities`]: longer matches win, then earlier
/// ones; a match overlapping an accepted one is dropped; existing entities are kept or removed
/// according to [`EntityRulerOptions::overwrite_entities`]. Every token outside the resulting
/// entities becomes outside (IOB `O`), including tokens whose entity annotation was missing or
/// blocked, even when nothing matches.
///
/// When patterns with different labels or IDs match exactly the same tokens, the rule added
/// earliest wins; a rule removed and added again counts from its new addition. spaCy's choice
/// there depends on Python set ordering; see `docs/ENTITY_RULER.md#differences-from-spacy`.
pub struct EntityRuler {
    options: EntityRulerOptions,
    lexicon: Option<Lexicon>,
    tokens: TokenMatcher,
    phrases: PhraseMatcher,
    token_patterns: Rules<TokenPattern>,
    phrase_patterns: Rules<String>,
    // The label and ID of each rule name added with an ID. Like spaCy's `_ent_ids`, entries stay
    // after the rule is removed.
    entity_ids: HashMap<String, (String, String)>,
    // Each live rule's rank, which breaks ties between identical spans, and the rule name of
    // every rank given out.
    ranks: HashMap<String, usize>,
    ranked: Vec<String>,
}

impl EntityRuler {
    /// A ruler without a lexicon. Token patterns with lexical flag conditions and lexical flag
    /// phrase attributes need [`EntityRuler::with_lexicon`]. An empty
    /// [`EntityRulerOptions::id_separator`] returns [`Error::Pattern`].
    pub fn new(options: EntityRulerOptions) -> Result<Self> {
        Self::build(options, None)
    }

    /// A ruler that evaluates lexical flags with a model's language rules; obtain the lexicon with
    /// [`crate::Model::lexicon`]. Returns [`Error::Pattern`] for an empty
    /// [`EntityRulerOptions::id_separator`], like [`EntityRuler::new`].
    pub fn with_lexicon(options: EntityRulerOptions, lexicon: Lexicon) -> Result<Self> {
        Self::build(options, Some(lexicon))
    }

    fn build(options: EntityRulerOptions, lexicon: Option<Lexicon>) -> Result<Self> {
        if options.id_separator.is_empty() {
            return Err(Error::Pattern(
                "the entity ID separator must not be empty".into(),
            ));
        }
        let (tokens, phrases) = matchers(options.phrase_attribute, lexicon.as_ref());
        Ok(Self {
            options,
            lexicon,
            tokens,
            phrases,
            token_patterns: Rules::default(),
            phrase_patterns: Rules::default(),
            entity_ids: HashMap::new(),
            ranks: HashMap::new(),
            ranked: Vec::new(),
        })
    }

    /// The options given at creation. They cannot be changed afterwards, and
    /// [`EntityRuler::clear`] keeps them.
    pub fn options(&self) -> &EntityRulerOptions {
        &self.options
    }

    /// The number of added patterns, counting duplicates and empty phrases, like spaCy's `len`.
    pub fn len(&self) -> usize {
        self.token_patterns.count() + self.phrase_patterns.count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Sorted distinct labels, like spaCy's `EntityRuler.labels`; see
    /// [`EntityRulerOptions::id_separator`] for labels containing the separator.
    pub fn labels(&self) -> Vec<String> {
        let labels: HashSet<&str> = self
            .rule_names()
            .map(|name| self.split(name).map_or(name, |(label, _)| label))
            .collect();
        sorted(labels)
    }

    /// Sorted distinct IDs read from rule names that contain the separator, like spaCy's
    /// `EntityRuler.ent_ids` (which is unordered). A token pattern added with an empty ID
    /// contributes `""`.
    pub fn ids(&self) -> Vec<String> {
        let ids: HashSet<&str> = self
            .rule_names()
            .filter_map(|name| self.split(name).map(|(_, id)| id))
            .collect();
        sorted(ids)
    }

    /// The added patterns, like spaCy's `EntityRuler.patterns`: token patterns grouped by rule
    /// name in the order each name first received a token pattern, then phrases grouped the same
    /// way. A rule removed and added again moves to the end.
    pub fn patterns(&self) -> Vec<RulerPattern> {
        let listed = |name: &str, pattern: RulerPatternKind| {
            let (label, id) = self.split(name).unwrap_or((name, ""));
            RulerPattern {
                label: label.to_owned(),
                id: (!id.is_empty()).then(|| id.to_owned()),
                pattern,
            }
        };
        let tokens = self
            .token_patterns
            .entries
            .iter()
            .flat_map(|(name, patterns)| {
                patterns
                    .iter()
                    .map(|p| listed(name, RulerPatternKind::Tokens(p.clone())))
            });
        let phrases = self
            .phrase_patterns
            .entries
            .iter()
            .flat_map(|(name, texts)| {
                texts
                    .iter()
                    .map(|text| listed(name, RulerPatternKind::Phrase(text.clone())))
            });
        tokens.chain(phrases).collect()
    }

    /// Add patterns, like spaCy's `add_patterns`. Patterns with the same label and ID add to one
    /// rule; duplicates are kept and counted by [`EntityRuler::len`]. Every pattern is checked
    /// before the ruler changes, so on error nothing is added:
    ///
    /// - an invalid token pattern, or a lexical flag condition without a lexicon, returns
    ///   [`Error::Pattern`];
    /// - any phrase when [`EntityRulerOptions::phrase_attribute`] is a lexical flag and the ruler
    ///   has no lexicon returns [`Error::Pattern`];
    /// - a non-empty phrase document in which no token has the annotation the phrase attribute
    ///   compares returns [`Error::MissingAnnotation`].
    pub fn add(&mut self, patterns: &[EntityPattern<'_>]) -> Result<()> {
        let names: Vec<String> = patterns.iter().map(|p| self.rule_name(p)).collect();
        let mut keys = HashMap::new();
        for (name, pattern) in names.iter().zip(patterns) {
            match &pattern.pattern {
                EntityPatternKind::Tokens(tokens) => {
                    self.tokens.check(name, std::slice::from_ref(tokens))?
                }
                EntityPatternKind::Phrase(doc) => {
                    self.phrases.check(name, &[doc])?;
                    // `check` compares a new rule name only with existing rules.
                    let key = crate::phrase_matcher::rule_key(name);
                    if keys
                        .insert(key, name.as_str())
                        .is_some_and(|other| other != name)
                    {
                        return Err(Error::Pattern("phrase rule hash collision".into()));
                    }
                }
            }
        }
        for name in &names {
            if !self.ranks.contains_key(name) {
                self.ranks.insert(name.clone(), self.ranked.len());
                self.ranked.push(name.clone());
            }
        }
        // spaCy registers an addition's token patterns before its phrases, which decides the
        // label and ID kept when two pairs form the same rule name.
        let tokens_first = (names.iter().zip(patterns))
            .filter(|(_, p)| matches!(p.pattern, EntityPatternKind::Tokens(_)))
            .chain(
                (names.iter().zip(patterns))
                    .filter(|(_, p)| matches!(p.pattern, EntityPatternKind::Phrase(_))),
            );
        for (name, pattern) in tokens_first {
            if let Some(id) = self.stored_id(pattern) {
                self.entity_ids
                    .insert(name.clone(), (pattern.label.clone(), id.to_owned()));
            }
            match &pattern.pattern {
                EntityPatternKind::Tokens(tokens) => {
                    self.tokens
                        .add(name.clone(), vec![tokens.clone()])
                        .expect("checked token pattern");
                    self.token_patterns.push(name, tokens.clone());
                }
                EntityPatternKind::Phrase(doc) => {
                    self.phrases
                        .add(name.as_str(), &[doc])
                        .expect("checked phrase pattern");
                    self.phrase_patterns.push(name, doc.text().to_owned());
                }
            }
        }
        Ok(())
    }

    /// Remove every pattern added with `id`, like spaCy's `remove`. Returns [`Error::Pattern`] and
    /// changes nothing when no pattern was ever added with `id`, or when any label once used with
    /// `id` no longer has patterns: as in spaCy, removed rule names stay known, so after
    /// `remove(id)`, adding `id` under a different label makes a later `remove(id)` fail.
    /// [`EntityRuler::clear`] forgets removed rule names.
    ///
    /// Unlike spaCy, a rule with both token and phrase patterns loses both, and a failed removal
    /// changes nothing. See `docs/ENTITY_RULER.md#differences-from-spacy`.
    pub fn remove(&mut self, id: &str) -> Result<()> {
        let names: HashSet<String> = self
            .entity_ids
            .values()
            .filter(|(_, known)| known == id)
            .map(|(label, known)| format!("{label}{}{known}", self.options.id_separator))
            .collect();
        if names.is_empty() {
            return Err(Error::Pattern(format!(
                "no entity pattern has the ID {id:?}"
            )));
        }
        if let Some(name) = names
            .iter()
            .find(|name| !self.tokens.contains(name) && !self.phrases.contains(name))
        {
            return Err(Error::Pattern(format!(
                "the entity rule {name:?} was already removed"
            )));
        }
        for name in &names {
            if self.tokens.contains(name) {
                self.tokens.remove(name).expect("registered token rule");
            }
            if self.phrases.contains(name) {
                self.phrases.remove(name).expect("registered phrase rule");
            }
            self.ranks.remove(name);
        }
        self.token_patterns.remove(&names);
        self.phrase_patterns.remove(&names);
        Ok(())
    }

    /// Remove every pattern and forget removed rule names, like spaCy's `clear`. The options and
    /// lexicon are kept.
    pub fn clear(&mut self) {
        (self.tokens, self.phrases) =
            matchers(self.options.phrase_attribute, self.lexicon.as_ref());
        self.token_patterns = Rules::default();
        self.phrase_patterns = Rules::default();
        self.entity_ids.clear();
        self.ranks.clear();
        self.ranked.clear();
    }

    /// Candidate entities, like spaCy's `EntityRuler.match`: token and phrase matches without
    /// empty ones and duplicates, longest first, then earliest. Matches of different rules over
    /// the same tokens are ordered by when their rules were added. Matches may overlap one
    /// another and existing entities; [`EntityRuler::annotate`] applies the overlap and
    /// [`EntityRulerOptions::overwrite_entities`] filtering. Errors are those of
    /// [`TokenMatcher::find_matches`] and [`PhraseMatcher::find_matches`].
    pub fn find_matches(&self, doc: &Doc) -> Result<Vec<EntityMatch>> {
        Ok(self
            .ranked_matches(doc)?
            .into_iter()
            .map(|(rank, start, end)| {
                let (label, id) = self.entity(rank);
                EntityMatch {
                    label: label.to_owned(),
                    id: id.map(str::to_owned),
                    start: TokenIndex(start),
                    end: TokenIndex(end),
                }
            })
            .collect())
    }

    /// Add matched entities to `doc`, like calling spaCy's `EntityRuler` on it. Errors are those
    /// of [`EntityRuler::find_matches`] and [`Doc::set_entities`]; on error `doc` is unchanged.
    pub fn annotate(&self, doc: &mut Doc) -> Result<()> {
        let update = self.entity_update(doc)?;
        doc.set_entities(&update)
    }

    /// Return an annotated copy, leaving `doc` unchanged, as [`EntityRuler::annotate`] would
    /// annotate it.
    pub fn annotated(&self, doc: &Doc) -> Result<Doc> {
        doc.with_entities(&self.entity_update(doc)?)
    }

    /// Matches as `(rank, start, end)`, deduplicated and in spaCy's order. A rank identifies one
    /// rule, so integer keys sort and deduplicate exactly as rule names would.
    fn ranked_matches(&self, doc: &Doc) -> Result<Vec<(usize, usize, usize)>> {
        let tokens = self.tokens.find_matches(doc)?;
        let phrases = self.phrases.find_matches(doc)?;
        let mut matches: Vec<(usize, usize, usize)> = tokens
            .iter()
            .map(|m| (m.rule.as_str(), m.start.0, m.end.0))
            .chain(
                phrases
                    .iter()
                    .map(|m| (m.rule.0.as_str(), m.start.0, m.end.0)),
            )
            .filter(|&(_, start, end)| start != end)
            .map(|(name, start, end)| (self.ranks[name], start, end))
            .collect();
        matches.sort_unstable_by_key(|&(rank, start, end)| {
            (std::cmp::Reverse(end - start), start, rank)
        });
        matches.dedup();
        Ok(matches)
    }

    /// The label and ID a rule gives its matches: the pair recorded for a rule added with an ID,
    /// otherwise the rule name with no ID.
    fn entity(&self, rank: usize) -> (&str, Option<&str>) {
        let name = &self.ranked[rank];
        match self.entity_ids.get(name) {
            Some((label, id)) => (label, (!id.is_empty()).then_some(id.as_str())),
            None => (name, None),
        }
    }

    /// spaCy's `set_annotations`: the entity list the ruler assigns to `doc.ents`.
    fn entity_update(&self, doc: &Doc) -> Result<EntityUpdate> {
        let matches = self.ranked_matches(doc)?;
        let tokens = doc.tokens();
        // Typed tokens before each position, so each match is checked in constant time.
        // Overwriting ignores existing entity types.
        let typed: Option<Vec<usize>> = if self.options.overwrite_entities {
            None
        } else {
            let counts = tokens.iter().scan(0, |count, t| {
                *count += usize::from(t.entity_type.as_deref().is_some_and(|k| !k.is_empty()));
                Some(*count)
            });
            Some(std::iter::once(0).chain(counts).collect())
        };
        let mut added = Vec::new();
        let mut seen = vec![false; tokens.len()];
        for (rank, start, end) in matches {
            if typed
                .as_ref()
                .is_some_and(|typed| typed[end] > typed[start])
            {
                continue;
            }
            // spaCy checks only the endpoints; with longer matches first this finds every overlap.
            if !seen[start] && !seen[end - 1] {
                seen[start..end].fill(true);
                let (label, id) = self.entity(rank);
                let mut span = Span::new(TokenIndex(start), TokenIndex(end), label);
                span.id = id.map(str::to_owned);
                added.push(span);
            }
        }
        // spaCy drops each existing entity an accepted match overlaps as it accepts the match.
        // Acceptance does not depend on that list, so one pass afterwards removes the same
        // entities without rescanning them for every match.
        let mut entities: Vec<Span> = doc
            .entities()
            .unwrap_or_default()
            .iter()
            .filter(|e| !seen[e.start.0..e.end.0].contains(&true))
            .cloned()
            .collect();
        entities.extend(added);
        Ok(EntityUpdate {
            entities,
            default: EntityDefault::Outside,
            ..EntityUpdate::default()
        })
    }

    fn rule_names(&self) -> impl Iterator<Item = &str> {
        self.token_patterns
            .names()
            .chain(self.phrase_patterns.names())
    }

    /// spaCy's `_split_label`: the label and ID of a rule name containing the separator.
    fn split<'s>(&self, name: &'s str) -> Option<(&'s str, &'s str)> {
        name.rsplit_once(self.options.id_separator.as_str())
    }

    /// spaCy's match key: a token pattern with any ID, or a phrase with a non-empty one, is stored
    /// under `label + separator + id`.
    fn rule_name(&self, pattern: &EntityPattern<'_>) -> String {
        match self.stored_id(pattern) {
            Some(id) => format!("{}{}{id}", pattern.label, self.options.id_separator),
            None => pattern.label.clone(),
        }
    }

    fn stored_id<'p>(&self, pattern: &'p EntityPattern<'_>) -> Option<&'p str> {
        let id = pattern.id.as_deref()?;
        match pattern.pattern {
            EntityPatternKind::Tokens(_) => Some(id),
            EntityPatternKind::Phrase(_) => (!id.is_empty()).then_some(id),
        }
    }
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
