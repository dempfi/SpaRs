# Span groups

A span group is a named list of spans stored in a document, following spaCy 3.8.14's `Doc.spans` and `SpanGroup`. Unlike entities, the spans in a group may overlap, repeat, or be empty, so groups can hold candidate mentions, alternative analyses, or the output of rules that should not overwrite entities. The Rust [SpanRuler](SPAN_RULER.md) writes its matches to a span group. Span groups are independent of entities, sentences and noun chunks: setting a group does not change any other annotation. This guide uses the Rust API of the `spars-nlp` package, imported as `spars` (see the [README](../README.md)); Node exposes span groups through `NativeDocument.withSpanGroup`, `withoutSpanGroup` and `toObject().spanGroups`, as described in [the Node guide](NODE.md#store-span-groups).

## Store and read groups

`Doc::span_groups` returns the groups by key. `Doc::set_span_group` stores a group under a key, and `Doc::remove_span_group` removes one. A [`SpanGroup`](../crates/spars/src/document/span_groups.rs) has a `name`, JSON `attrs` and its `spans`. The example builds a three-token document from a native snapshot, so it needs no model: each token gives its byte offsets `start` and `end`, its code-point offset `idx`, whether a space follows it, and its norm, and `null` marks annotations that were not produced. [Entity editing](ENTITIES.md) describes snapshot validation.

```rust
use spars::{Doc, Error, Span, SpanGroup, TokenIndex};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let token = |start: usize, end: usize, space: bool| format!(
        r#"{{"start":{start},"end":{end},"idx":{start},"whitespace":{space},"norm":"x"}}"#);
    let mut doc = Doc::from_json(&format!(
        r#"{{"format_version":1,"document":{{"text":"New York City","tokens":[{},{},{}],
        "entities":null,"sentences":null,"noun_chunks":null}}}}"#,
        token(0, 3, true), token(4, 8, true), token(9, 13, false)))?;
    let span = |start, end, label: &str| Span::new(TokenIndex(start), TokenIndex(end), label);

    // Overlapping spans are allowed, and spans keep the order they were added in.
    let mut places = SpanGroup::new("places", vec![span(0, 3, "GPE"), span(0, 2, "GPE")]);
    places.attrs.insert("source", r#""gazetteer""#)?;
    doc.set_span_group("places", places)?;
    let group = doc.span_groups().get("places").unwrap();
    assert!(group.has_overlap());
    assert_eq!(group.attrs.get("source"), Some(r#""gazetteer""#));

    // To edit a group, copy it, change it and store it again. Replacing a key keeps its position.
    doc.set_span_group("other", SpanGroup::new("other", vec![]))?;
    let mut edited = doc.span_groups().get("places").unwrap().clone();
    edited.spans.push(span(2, 3, "CITY"));
    doc.set_span_group("places", edited)?;
    assert_eq!(doc.span_groups().keys().collect::<Vec<_>>(), ["places", "other"]);

    // Spans must lie within the document; a rejected group leaves the document unchanged.
    let outside = SpanGroup::new("bad", vec![span(2, 4, "X")]);
    assert!(matches!(doc.set_span_group("bad", outside), Err(Error::Bounds)));
    assert!(doc.span_groups().get("bad").is_none());

    // Snapshots keep every group, its attrs and the key order.
    let restored = Doc::from_json(&doc.to_json()?)?;
    assert_eq!(restored, doc);
    assert_eq!(doc.remove_span_group("other").unwrap().name, "other");
    Ok(())
}
```

## Behavior

These follow spaCy unless noted here or listed under [differences from spaCy](#differences-from-spacy).

- **Keys and names.** A group's `name` need not equal its key. spaCy names a group after its key when a plain list of spans is assigned (`doc.spans["k"] = [...]`); the equivalent here is `SpanGroup::new(key, spans)`. A new key goes after the existing keys, and replacing a key keeps its position, as in a Python dict.
- **Spans.** Each span covers tokens `start..end`, where `start <= end` and `end` is at most the number of tokens; `start == end` is an empty span. Labels may be empty. A span's `id` is spaCy's `Span.id_`; `Doc::set_span_group` stores an empty ID as `None`, since spaCy uses `""` for no ID. Bounds are checked when the group is stored: `Doc::set_span_group` returns `Error::Bounds` for any span with `start > end` or outside the document, where spaCy raises `IndexError` (E035) when the `Span` is constructed (slicing with `doc[a:b]` clamps instead).
- **Attributes.** `attrs` holds JSON values under string keys in insertion order, like spaCy's `SpanGroup.attrs` dict restricted to JSON values; spaCy also accepts other values its msgpack serializer can save, which SpaRs cannot store. `SpanGroupAttrs::insert` takes a value as JSON text, such as `"1"` or `r#""text""#`, and returns `Error::Json` for text that is not one JSON value. Values are stored as the text given, so the key order inside nested objects is kept, and two values are equal only when their texts are; `{"a": 1}` and `{"a":1}` differ.
- **Overlap.** `SpanGroup::has_overlap` reports whether two spans share a token, like `SpanGroup.has_overlap`. An empty span overlaps a span that starts before it and ends after it, but not one that only touches it.
- **Combining groups.** `SpanGroup::extend_group` appends another group's spans and adds the attrs whose keys the group lacks, like `SpanGroup.extend` with a group. `SpanGroup::concat` returns a new group with the first group's name, like `+`. Appending or extending with spans is ordinary `Vec` editing of `spans`.
- **Snapshots.** A document with span groups is saved as snapshot format version 4, which older SpaRs versions reject instead of dropping the groups. Documents without groups are saved in the oldest version that holds their data. `Doc::from_json` returns `Error::Bounds` for spans with `start > end` or outside the document, `Error::Model` for repeated group keys and empty span IDs, `Error::Json` for repeated attribute keys, attrs that are not a JSON object and unknown fields in a group entry, and `Error::Unsupported` for groups in a snapshot older than version 4.

## Differences from spaCy

These differences are [accepted](SPACY_DIFFERENCES.md#span-groups) as SpaRs's intended behavior; that page gives the reason and effect of each.

- **Removing a span by index.** spaCy 3.8.14's `del group[i]` removes the span before index `i` (an upstream bug). Here `group.spans.remove(i)` removes the span at index `i`.
- **Groups are values.** spaCy returns the stored group object, so editing it changes the document. Here callers edit a copy and store it with `Doc::set_span_group`, and each key holds its own group.
- **Copies keep empty spans.** spaCy's `Doc.copy()` raises E1052 for an empty span at the start or end of the document or after a token with trailing whitespace. `Doc::clone` copies them.
- **Snapshots keep key order.** spaCy's `Doc.to_bytes` can reorder keys whose groups have identical content. SpaRs snapshots keep the key order.
- **Attributes are JSON values compared as text.** spaCy accepts any value msgpack can save and compares attrs as dicts. SpaRs stores JSON values and compares their keys in order and their text.
- **Spans and groups from another document.** spaCy rejects them. SpaRs accepts a group whose spans fit the document.
- **No knowledge-base IDs.** spaCy group members carry a `kb_id`; SpaRs spans do not.
- **Extending a group with itself.** spaCy's `group.extend(group)` is undefined behavior in C++. `extend_group` with a copy appends its spans once.

## Verification

`fixtures/span-groups-v1.expected.json` freezes spaCy 3.8.14 span group states after assigning lists and groups, deleting keys, appending, extending with spans and groups, concatenating, replacing spans by index, and copying documents, including rejected edits. `crates/spars/tests/span_groups.rs` replays every step, compares the keys, names, attrs, spans and `has_overlap` of each group and the rejected edits, checks that a rejected edit changes nothing, and checks a snapshot round trip after every step. `bindings/node/tests/span-groups.test.mts` replays every step through the Node binding. `tools/test_span_group_reference.py` regenerates the fixture with spaCy, compares it with the frozen file, and replays every step with plain Python. Run them with:

```bash
cargo test --release --offline -p spars-nlp --test span_groups
cargo test --release --offline -p spars-nlp --lib span_groups
.venv/bin/python -m unittest discover -s tools -p test_span_group_reference.py
npm --prefix bindings/node test
```

The Python check needs the [reference environment](DEVELOPMENT.md) with `en_core_web_md` 3.8.0 installed. The Node suite needs the built binding, as described in [the Node guide](NODE.md); its span group tests need no model.
