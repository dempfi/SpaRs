//! Named span groups, following spaCy 3.8.14 `Doc.spans` and `SpanGroup`.
use super::{Doc, Span};
use crate::{Error, Result};
use serde::{de, ser::SerializeMap, Deserialize, Deserializer, Serialize, Serializer};
use serde_json::value::RawValue;
use std::collections::HashSet;
use std::fmt;

/// The attributes of a [`SpanGroup`], like spaCy's `SpanGroup.attrs`: JSON values under string
/// keys, kept in insertion order. Each value is kept as the JSON text it was given, without
/// surrounding whitespace, so key order inside nested objects survives a snapshot round trip.
/// Values are compared as text, unlike Python dicts: `{"a": 1}` and `{"a":1}`, or `1` and `1.0`,
/// differ, and so do attrs with the same keys in a different order.
#[derive(Debug, Clone, Default)]
pub struct SpanGroupAttrs {
    entries: Vec<(String, Box<RawValue>)>,
}

impl SpanGroupAttrs {
    /// No attrs, the same as `Default`.
    pub fn new() -> Self {
        Self::default()
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    /// The JSON text of `key`'s value, such as `"\"gazetteer\""` for a string, or `None` when
    /// the key is absent. Parse it with `serde_json::from_str` to get a typed value.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, value)| value.get())
    }
    pub fn contains_key(&self, key: &str) -> bool {
        self.get(key).is_some()
    }
    /// Set `key` to the JSON value written in `json`, such as `"1"`, `"\"text\""` or
    /// `r#"{"a": [1]}"#`, and return the previous value's JSON text. A new key goes last; an
    /// existing key keeps its position, as in a Python dict. Text that is not a single JSON value
    /// returns [`Error::Json`] and changes nothing. Whitespace around the value is dropped.
    ///
    /// ```
    /// let mut attrs = spars::SpanGroupAttrs::new();
    /// attrs.insert("source", r#" "gazetteer" "#)?;
    /// assert_eq!(attrs.get("source"), Some(r#""gazetteer""#));
    /// assert!(attrs.insert("bad", "'a'").is_err());
    /// # Ok::<(), spars::Error>(())
    /// ```
    pub fn insert(&mut self, key: impl Into<String>, json: &str) -> Result<Option<String>> {
        let value = RawValue::from_string(json.to_owned())?;
        let key = key.into();
        Ok(match self.entries.iter_mut().find(|(k, _)| *k == key) {
            Some((_, slot)) => Some(std::mem::replace(slot, value).get().to_owned()),
            None => {
                self.entries.push((key, value));
                None
            }
        })
    }
    /// Remove `key` and return its value's JSON text, keeping the order of the other keys.
    pub fn remove(&mut self, key: &str) -> Option<String> {
        let index = self.entries.iter().position(|(k, _)| k == key)?;
        Some(self.entries.remove(index).1.get().to_owned())
    }
    /// Keys and the JSON text of their values, in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v.get()))
    }
    /// Add the entries of `other` whose keys are missing here, as spaCy merges attrs when groups
    /// are combined.
    fn add_missing(&mut self, other: &Self) {
        for (key, value) in &other.entries {
            if !self.contains_key(key) {
                self.entries.push((key.clone(), value.clone()));
            }
        }
    }
    fn heap_bytes(&self) -> usize {
        self.entries.capacity() * std::mem::size_of::<(String, Box<RawValue>)>()
            + self
                .entries
                .iter()
                .map(|(k, v)| k.capacity() + v.get().len())
                .sum::<usize>()
    }
}

/// Equal when the keys are in the same order and each value has the same JSON text.
impl PartialEq for SpanGroupAttrs {
    fn eq(&self, other: &Self) -> bool {
        self.entries.len() == other.entries.len()
            && self
                .entries
                .iter()
                .zip(&other.entries)
                .all(|((ka, va), (kb, vb))| ka == kb && va.get() == vb.get())
    }
}

impl Serialize for SpanGroupAttrs {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.entries.len()))?;
        for (key, value) in &self.entries {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

/// Reads a JSON object in order and rejects repeated keys, which a Python dict cannot hold. Only
/// a JSON deserializer can produce the raw values.
impl<'de> Deserialize<'de> for SpanGroupAttrs {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = SpanGroupAttrs;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a JSON object of span group attributes")
            }
            fn visit_map<A: de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> std::result::Result<SpanGroupAttrs, A::Error> {
                let mut attrs = SpanGroupAttrs::new();
                // A set keeps the repeated-key check linear for snapshots with many attrs.
                let mut seen = HashSet::new();
                while let Some((key, value)) = map.next_entry::<String, Box<RawValue>>()? {
                    if !seen.insert(key.clone()) {
                        return Err(de::Error::custom(format!(
                            "repeated span group attribute {key:?}"
                        )));
                    }
                    attrs.entries.push((key, value));
                }
                Ok(attrs)
            }
        }
        deserializer.deserialize_map(Visitor)
    }
}

/// A named list of spans for a [`Doc`], like spaCy's `SpanGroup`. Spans keep the order they were
/// added in and may overlap, repeat, or be empty (`start == end`). A group is not tied to a
/// document, so nothing is checked until [`Doc::set_span_group`] stores it. To change a stored
/// group, clone it from [`Doc::span_groups`] (or take it with [`Doc::remove_span_group`]), edit
/// it, and store it again. Code outside this crate cannot build a `SpanGroup` with a struct
/// literal; use [`SpanGroup::new`] or `Default`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct SpanGroup {
    /// spaCy's `SpanGroup.name`. It need not equal the key the group is stored under: spaCy names
    /// a group after its key only when a plain list of spans is assigned.
    pub name: String,
    #[serde(default, skip_serializing_if = "SpanGroupAttrs::is_empty")]
    pub attrs: SpanGroupAttrs,
    /// Each span's `label` may be empty, and its `id` is spaCy's `Span.id_`, with `None` for no
    /// ID; [`Doc::set_span_group`] treats an empty ID as `None`.
    pub spans: Vec<Span>,
}

impl SpanGroup {
    /// A group named `name` with no attrs and `spans` in the given order. spaCy names a group
    /// after its key when a list of spans is assigned, as in `SpanGroup::new(key, spans)`.
    pub fn new(name: impl Into<String>, spans: Vec<Span>) -> Self {
        Self {
            name: name.into(),
            attrs: SpanGroupAttrs::new(),
            spans,
        }
    }
    /// Whether any two spans share a token, like spaCy's `SpanGroup.has_overlap`. Empty spans
    /// overlap nothing, except a span that starts before and ends after them.
    pub fn has_overlap(&self) -> bool {
        // spaCy sorts spans and compares each start with the previous span's end; spans with the
        // same bounds are interchangeable here, so sorting by bounds gives its result.
        let mut bounds: Vec<(usize, usize)> =
            self.spans.iter().map(|s| (s.start.0, s.end.0)).collect();
        bounds.sort_unstable();
        bounds.windows(2).any(|pair| pair[1].0 < pair[0].1)
    }
    /// Append `other`'s spans and the attrs whose keys this group lacks, like spaCy's
    /// `SpanGroup.extend` with a group or `+=`. Added attrs go last, in `other`'s order, and the
    /// name is unchanged. Unlike spaCy, a group from another document is accepted; bounds are
    /// checked when the result is stored with [`Doc::set_span_group`].
    pub fn extend_group(&mut self, other: &SpanGroup) {
        self.attrs.add_missing(&other.attrs);
        self.spans.extend_from_slice(&other.spans);
    }
    /// A new group with this group's name, its attrs plus those of `other` it lacks, and the spans
    /// of both, like spaCy's `+` on span groups.
    pub fn concat(&self, other: &SpanGroup) -> SpanGroup {
        let mut group = self.clone();
        group.extend_group(other);
        group
    }
    fn heap_bytes(&self) -> usize {
        self.name.capacity()
            + self.attrs.heap_bytes()
            + self.spans.capacity() * std::mem::size_of::<Span>()
            + self
                .spans
                .iter()
                .map(|s| s.label.capacity() + s.id.as_ref().map_or(0, String::capacity))
                .sum::<usize>()
    }
}

/// The span groups of a [`Doc`] by key, like spaCy's `Doc.spans`. Keys keep the order they were
/// added in: replacing a group keeps its key's position, and a key that is removed and set again
/// goes last. Read through [`Doc::span_groups`]; change with [`Doc::set_span_group`] and
/// [`Doc::remove_span_group`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpanGroups {
    entries: Vec<(String, SpanGroup)>,
}

impl SpanGroups {
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn get(&self, key: &str) -> Option<&SpanGroup> {
        self.entries
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, group)| group)
    }
    pub fn contains_key(&self, key: &str) -> bool {
        self.get(key).is_some()
    }
    /// Keys and their groups in the order the keys were added (see [`SpanGroups`]).
    pub fn iter(&self) -> impl Iterator<Item = (&str, &SpanGroup)> {
        self.entries.iter().map(|(k, g)| (k.as_str(), g))
    }
    /// Keys in the order they were added, the same order as [`SpanGroups::iter`].
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|(k, _)| k.as_str())
    }
    pub(crate) fn heap_bytes(&self) -> usize {
        self.entries.capacity() * std::mem::size_of::<(String, SpanGroup)>()
            + self
                .entries
                .iter()
                .map(|(k, g)| k.capacity() + g.heap_bytes())
                .sum::<usize>()
    }
    /// Check stored groups read from a snapshot: unique keys, spans within `length` tokens, and
    /// omitted rather than empty span IDs.
    pub(crate) fn check_snapshot(&self, length: usize) -> Result<()> {
        let mut seen = HashSet::new();
        for (key, group) in &self.entries {
            if !seen.insert(key.as_str()) {
                return Err(Error::Model(format!("repeated span group key {key:?}")));
            }
            check_spans(&group.spans, length)?;
            if group.spans.iter().any(|s| s.id.as_deref() == Some("")) {
                return Err(Error::Model(
                    "snapshot span IDs must be omitted rather than empty".into(),
                ));
            }
        }
        Ok(())
    }
}

fn check_spans(spans: &[Span], length: usize) -> Result<()> {
    if spans
        .iter()
        .any(|s| s.start.0 > s.end.0 || s.end.0 > length)
    {
        return Err(Error::Bounds);
    }
    Ok(())
}

/// Stored as a list of `{key, name, attrs?, spans}` objects, so snapshots keep the key order.
impl Serialize for SpanGroups {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Entry<'a> {
            key: &'a str,
            name: &'a str,
            #[serde(skip_serializing_if = "SpanGroupAttrs::is_empty")]
            attrs: &'a SpanGroupAttrs,
            spans: &'a [Span],
        }
        serializer.collect_seq(self.entries.iter().map(|(key, group)| Entry {
            key,
            name: &group.name,
            attrs: &group.attrs,
            spans: &group.spans,
        }))
    }
}

/// Reads the list written by `Serialize` without checking keys or spans; [`Doc::from_json`]
/// rejects repeated keys, spans outside the document and empty span IDs.
impl<'de> Deserialize<'de> for SpanGroups {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Entry {
            key: String,
            name: String,
            #[serde(default)]
            attrs: SpanGroupAttrs,
            spans: Vec<Span>,
        }
        let entries = Vec::<Entry>::deserialize(deserializer)?;
        Ok(Self {
            entries: entries
                .into_iter()
                .map(|e| {
                    (
                        e.key,
                        SpanGroup {
                            name: e.name,
                            attrs: e.attrs,
                            spans: e.spans,
                        },
                    )
                })
                .collect(),
        })
    }
}

impl Doc {
    /// The document's span groups, like spaCy's `Doc.spans`; empty unless set with
    /// [`Doc::set_span_group`] or restored by [`Doc::from_json`], since no pipeline stage adds
    /// groups. They are independent of entities, sentences and noun chunks. Any stored group, even
    /// one with no spans, makes [`Doc::to_json`] write snapshot format v4.
    pub fn span_groups(&self) -> &SpanGroups {
        &self.span_groups
    }
    /// Store `group` under `key`, like assigning to spaCy's `Doc.spans[key]`. A new key goes last;
    /// an existing key keeps its position and its group is replaced. Every span must satisfy
    /// `start <= end <= doc.tokens().len()`, or [`Error::Bounds`] is returned and the document is
    /// unchanged. Span labels are not checked, and a span ID of `Some("")` is stored as `None`.
    pub fn set_span_group(&mut self, key: impl Into<String>, mut group: SpanGroup) -> Result<()> {
        check_spans(&group.spans, self.tokens.len())?;
        for span in &mut group.spans {
            if span.id.as_deref() == Some("") {
                span.id = None;
            }
        }
        let key = key.into();
        match self.span_groups.entries.iter_mut().find(|(k, _)| *k == key) {
            Some((_, slot)) => *slot = group,
            None => self.span_groups.entries.push((key, group)),
        }
        Ok(())
    }
    /// Remove and return the group under `key`, keeping the order of the others; `None` when no
    /// group has that key.
    pub fn remove_span_group(&mut self, key: &str) -> Option<SpanGroup> {
        let index = self
            .span_groups
            .entries
            .iter()
            .position(|(k, _)| k == key)?;
        Some(self.span_groups.entries.remove(index).1)
    }
}

#[cfg(test)]
mod tests;
