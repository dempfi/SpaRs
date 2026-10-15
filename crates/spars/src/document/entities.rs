//! Checked entity annotation updates, following spaCy 3.8.14 `Doc.set_ents`.
use super::{Doc, Span, TokenIndex};
use crate::{Error, Result};
use serde::{Deserialize, Deserializer, Serialize};

/// A half-open token range without a label, for blocked, missing and outside annotation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenRange {
    pub start: TokenIndex,
    pub end: TokenIndex,
}

/// How [`Doc::set_entities`] annotates tokens outside every given span, like the `default`
/// of spaCy's `Doc.set_ents`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityDefault {
    /// Outside any entity (IOB `O`).
    #[default]
    Outside,
    /// Unknown, as before entity recognition.
    Missing,
    /// Never part of an entity (IOB `B` without a type).
    Blocked,
    /// Keep the current annotation.
    Unmodified,
}

/// A replacement of entity annotation, applied by [`Doc::set_entities`].
///
/// Spans in all four lists must not share tokens. Entities with an empty label are ignored. An
/// entity's `id` is written to its tokens; an entity without one, or with an empty one, keeps each
/// token's current ID, as in spaCy.
// TODO(entity-kb-ids): spaCy also copies each span's kb_id to its tokens; SpaRs stores no
// knowledge-base IDs. See docs/ENTITIES.md#limits.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EntityUpdate {
    /// Labeled entities.
    #[serde(deserialize_with = "labeled_spans")]
    pub entities: Vec<Span>,
    /// Tokens that can never be part of an entity.
    pub blocked: Vec<TokenRange>,
    /// Tokens whose entity annotation is unknown.
    pub missing: Vec<TokenRange>,
    /// Tokens outside any entity.
    pub outside: Vec<TokenRange>,
    pub default: EntityDefault,
}

/// Reject unknown entity fields, such as spaCy's `kb_id`, instead of dropping them.
fn labeled_spans<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Vec<Span>, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Entity {
        start: TokenIndex,
        end: TokenIndex,
        label: String,
        #[serde(default)]
        id: Option<String>,
    }
    let entities = Vec::<Entity>::deserialize(deserializer)?;
    Ok(entities
        .into_iter()
        .map(|e| Span {
            start: e.start,
            end: e.end,
            label: e.label,
            id: e.id,
        })
        .collect())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Iob {
    Missing,
    Inside,
    Outside,
    Begin,
}

/// A token's entity annotation as stored: IOB tag, type and ID.
struct TokenEntity {
    index: usize,
    iob: Option<String>,
    kind: Option<String>,
    id: Option<String>,
}

/// A checked entity update: the tokens whose stored values change, and the resulting entities.
struct EntityChanges {
    tokens: Vec<TokenEntity>,
    entities: Option<Vec<Span>>,
}

impl Doc {
    /// Replace entity annotation like spaCy's `Doc.set_ents`, keeping the token IOB tags,
    /// token entity types and [`Doc::entities`] consistent.
    ///
    /// Every check runs before anything changes. A span outside the document, a reversed span
    /// or two spans sharing a token return [`Error::Bounds`]. A current IOB tag other than `B`,
    /// `I`, `O` or missing, or an `I` left with no entity to continue, returns
    /// [`Error::Unsupported`]. [`Doc::from_json`] rejects unknown tags, so one can only come from
    /// direct changes to the public token fields; a stray `I` can also come from an edited
    /// snapshot. Afterwards an inside tag that follows a missing or outside token, or a token of
    /// another type, becomes a beginning tag. Tokens the update does not write keep their stored
    /// values. A non-empty document whose tokens are all missing has no entity annotation
    /// (`entities()` is `None`).
    pub fn set_entities(&mut self, update: &EntityUpdate) -> Result<()> {
        let changes = self.entity_changes(update)?;
        self.apply_entity_changes(changes);
        Ok(())
    }

    /// Return a copy with replaced entity annotation, leaving this document unchanged. The update
    /// is checked before anything is copied, so an invalid update costs no copy. Errors and the
    /// resulting annotation are those of [`Doc::set_entities`].
    pub fn with_entities(&self, update: &EntityUpdate) -> Result<Doc> {
        let changes = self.entity_changes(update)?;
        let mut doc = self.clone();
        doc.apply_entity_changes(changes);
        Ok(doc)
    }

    /// Write checked changes from [`Doc::entity_changes`] computed for this document's tokens.
    fn apply_entity_changes(&mut self, changes: EntityChanges) {
        for change in changes.tokens {
            let token = &mut self.tokens[change.index];
            token.entity_iob = change.iob;
            token.entity_type = change.kind;
            token.entity_id = change.id;
        }
        self.entities = changes.entities;
    }

    /// Check an update and compute the token values it changes, without changing anything.
    fn entity_changes(&self, update: &EntityUpdate) -> Result<EntityChanges> {
        let length = self.tokens.len();
        let ranges = update
            .entities
            .iter()
            .map(|span| (span.start, span.end))
            .chain(
                [&update.blocked, &update.missing, &update.outside]
                    .into_iter()
                    .flatten()
                    .map(|range| (range.start, range.end)),
            );
        for (start, end) in ranges {
            if start.0 > end.0 || end.0 > length {
                return Err(Error::Bounds);
            }
        }
        let entities: Vec<&Span> = update
            .entities
            .iter()
            .filter(|span| !span.label.is_empty())
            .collect();
        // Tokens the update writes; the rest keep their stored values, as in spaCy.
        let mut covered = vec![false; length];
        let unlabeled = [&update.blocked, &update.missing, &update.outside]
            .into_iter()
            .flatten()
            .map(|range| (range.start, range.end));
        for (start, end) in entities.iter().map(|s| (s.start, s.end)).chain(unlabeled) {
            for seen in &mut covered[start.0..end.0] {
                if std::mem::replace(seen, true) {
                    return Err(Error::Bounds);
                }
            }
        }
        let mut tags = self.entity_tags()?;
        // IDs change only where an entity carries a non-empty one; every other token keeps its ID.
        let mut ids: Vec<Option<&str>> =
            self.tokens.iter().map(|t| t.entity_id.as_deref()).collect();
        for span in &entities {
            let id = span.id.as_deref().filter(|id| !id.is_empty());
            for (i, tag) in tags[span.start.0..span.end.0].iter_mut().enumerate() {
                let iob = if i == 0 { Iob::Begin } else { Iob::Inside };
                *tag = (iob, span.label.as_str());
            }
            if id.is_some() {
                ids[span.start.0..span.end.0].fill(id);
            }
        }
        for (ranges, iob) in [
            (&update.blocked, Iob::Begin),
            (&update.missing, Iob::Missing),
            (&update.outside, Iob::Outside),
        ] {
            for range in ranges {
                tags[range.start.0..range.end.0].fill((iob, ""));
            }
        }
        let default = match update.default {
            EntityDefault::Outside => Some(Iob::Outside),
            EntityDefault::Missing => Some(Iob::Missing),
            EntityDefault::Blocked => Some(Iob::Begin),
            EntityDefault::Unmodified => None,
        };
        if let Some(iob) = default {
            for (tag, seen) in tags.iter_mut().zip(&mut covered) {
                if !*seen {
                    *tag = (iob, "");
                    *seen = true;
                }
            }
        }
        // spaCy's repair pass, in order, so each step sees the previous repair.
        for i in 1..length {
            let (previous, previous_type) = tags[i - 1];
            let (iob, kind) = tags[i];
            if iob == Iob::Inside
                && (matches!(previous, Iob::Missing | Iob::Outside)
                    || (previous_type != kind && matches!(previous, Iob::Begin | Iob::Inside)))
            {
                tags[i].0 = Iob::Begin;
                covered[i] = true;
            }
        }
        let spans = derive_entities(&tags, &ids)?;
        // spaCy's has_annotation: an empty document counts as annotated.
        let annotated = length == 0 || tags.iter().any(|(iob, _)| *iob != Iob::Missing);
        // Allocate only for written tokens whose stored values change; updates usually touch few.
        let tokens: Vec<TokenEntity> = tags
            .iter()
            .zip(&ids)
            .zip(&self.tokens)
            .enumerate()
            .filter(|&(i, _)| covered[i])
            .filter_map(|(index, ((&(iob, kind), &id), token))| {
                let (iob, kind) = match iob {
                    Iob::Missing => (None, None),
                    Iob::Inside => (Some("I"), Some(kind)),
                    Iob::Outside => (Some("O"), Some(kind)),
                    Iob::Begin => (Some("B"), Some(kind)),
                };
                let unchanged = token.entity_iob.as_deref() == iob
                    && token.entity_type.as_deref() == kind
                    && token.entity_id.as_deref() == id;
                (!unchanged).then(|| TokenEntity {
                    index,
                    iob: iob.map(str::to_owned),
                    kind: kind.map(str::to_owned),
                    id: id.map(str::to_owned),
                })
            })
            .collect();
        Ok(EntityChanges {
            tokens,
            entities: annotated.then_some(spans),
        })
    }

    /// The current tags. A missing tag is read without its type, which only an edited snapshot or
    /// a direct field change can store; `set_ents` clears the type whenever it writes a missing tag.
    fn entity_tags(&self) -> Result<Vec<(Iob, &str)>> {
        self.tokens
            .iter()
            .map(|token| {
                let iob = match token.entity_iob.as_deref() {
                    None | Some("") => return Ok((Iob::Missing, "")),
                    Some("B") => Iob::Begin,
                    Some("I") => Iob::Inside,
                    Some("O") => Iob::Outside,
                    Some(_) => {
                        return Err(Error::Unsupported(
                            "entity IOB tags must be B, I, O or missing".into(),
                        ))
                    }
                };
                Ok((iob, token.entity_type.as_deref().unwrap_or("")))
            })
            .collect()
    }
}

/// spaCy's `Doc.ents`: a beginning tag with a type starts an entity, inside tags continue it,
/// and a beginning tag without a type (a blocked token) starts none. An entity's ID is its first
/// token's.
fn derive_entities(tags: &[(Iob, &str)], ids: &[Option<&str>]) -> Result<Vec<Span>> {
    let mut spans = Vec::new();
    let mut open: Option<(usize, &str)> = None;
    for (i, &(iob, kind)) in tags.iter().enumerate() {
        match iob {
            Iob::Inside if open.is_none() => {
                return Err(Error::Unsupported(
                    "an entity inside tag must follow a beginning tag".into(),
                ))
            }
            Iob::Inside => continue,
            _ => {}
        }
        if let Some((start, label)) = open.take() {
            spans.push(span(start, i, label, ids[start]));
        }
        if iob == Iob::Begin && !kind.is_empty() {
            open = Some((i, kind));
        }
    }
    if let Some((start, label)) = open {
        spans.push(span(start, tags.len(), label, ids[start]));
    }
    Ok(spans)
}

fn span(start: usize, end: usize, label: &str, id: Option<&str>) -> Span {
    Span {
        start: TokenIndex(start),
        end: TokenIndex(end),
        label: label.into(),
        id: id.map(str::to_owned),
    }
}

#[cfg(test)]
mod tests;
