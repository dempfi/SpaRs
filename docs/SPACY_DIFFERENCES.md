# Accepted differences from spaCy

SpaRs follows spaCy 3.8.14 by default, and a behavior that differs from it is normally a compatibility gap listed in [the compatibility inventory](COMPATIBILITY.md). This file lists the differences that are accepted as SpaRs's intended behavior instead. Each one exists because spaCy's behavior depends on Python internals that are impractical to emulate, is an upstream bug, leaves an object partly changed after an error, fails later than the setting that causes the failure, depends on internal state left behind by rules that were removed, relies on Python object identity or values that native snapshots cannot store, is a limitation of spaCy's offset-based copying or byte format, or concerns data that SpaRs does not store. The reference fixtures contain no case whose result these differences change; the tests named below check SpaRs's behavior.

Each entry gives spaCy's behavior, SpaRs's behavior, the reason, and the effect on callers.

## EntityRuler

These apply to the Rust [`EntityRuler`](ENTITY_RULER.md) and the Node [`EntityRuler`](NODE.md#add-entities-with-rules), which uses it. A rule is the group of patterns with the same label and ID; spaCy names it `label + ent_id_sep + id`, or just the label without an ID, and SpaRs does the same. The pattern listing is spaCy's `patterns` property and SpaRs's `patterns()`. The unit tests are in `crates/spars/src/entity_ruler/tests.rs`; run them with `cargo test --release --offline -p spars-nlp --lib entity_ruler`.

### Ties between identical spans

- **spaCy:** when patterns with different labels or IDs match exactly the same tokens, the ruler keeps the match that comes first when Python iterates over a set of `(rule hash, start, end)` tuples. That order is deterministic for a given CPython version, but it depends on the tuple hashes (the 64-bit hash of the rule name, built from the label and ID, combined with the start and end), the size of the set and hash collisions, not on the order in which patterns were added.
- **SpaRs:** keeps the match of the rule that received its first pattern earliest, in list order within one `add` call. Adding more patterns to an existing rule keeps its place; a rule that is removed and added again counts from its new addition.
- **Reason:** emulating CPython's set layout is impractical, and a fixed order lets callers decide the winner by adding patterns in priority order.
- **Effect:** results differ from spaCy only for identical spans of different rules, and only when spaCy's set order disagrees with addition order. The reference generator (`tools/entity_ruler_reference.py`) rejects such ties: it redraws random cases that contain one, and an authored case with one fails generation.
- **Tests:** `identical_spans_take_the_first_added_rule`, `a_rule_added_again_ranks_after_older_rules`.

### Removing an ID that has both token patterns and phrases

- **spaCy:** `EntityRuler.remove(id)` removes the rule's patterns from the pattern listing, then removes the rule from the phrase matcher if it has phrases and from the token matcher only otherwise. A rule with both kinds of pattern therefore stops being listed, but its token patterns keep producing entities. Because spaCy keeps removed IDs in an internal map, a second `remove` of the same ID still finds it and removes them; a third raises error E175. This is a bug in spaCy 3.8.14, and the code is unchanged on spaCy's `master` branch at commit `c2dabfce56ad` (2026-09-30); spaCy's `SpanRuler.remove_by_id` and `SpanRuler.remove` check both matchers. The [reproduction](#reproducing-the-remove-bug) below shows it.
- **SpaRs:** removes the token patterns and phrases at once, so the rule stops matching and a second `remove` of the ID returns an error.
- **Reason:** the documented purpose of `remove` is to remove the ID's patterns, and the pattern listing should describe what matches.
- **Effect:** after such a removal, spaCy still adds entities from the token patterns and SpaRs does not.
- **Tests:** `remove_drops_token_and_phrase_patterns_of_a_rule`.

### Failed additions and removals change nothing

- **spaCy:** `add_patterns` adds every token pattern, then every phrase, one at a time, and stores each pattern in its listing before passing it to the matcher. An invalid token pattern therefore raises an error after the token patterns before it have been added, before any phrase is added, and while the rejected pattern itself stays listed and counted by `len` without matching. `remove` raises error E175 when it reaches a rule name that an earlier `remove` already deleted and that was not added again, as when an ID is removed twice, or re-added under only some of its earlier labels and removed again. By then every rule of the ID has been removed from the listing; the rules handled before the failure stop matching and the rest keep matching.
- **SpaRs:** `add` and `remove` check every pattern or rule first and return an error without changing the ruler.
- **Reason:** a caller that handles the error can keep using the ruler without inspecting what was partly changed. The PhraseMatcher follows the same rule for rejected additions; see [its Rust boundaries](PHRASE_MATCHER.md#rust-boundaries-and-remaining-scope).
- **Effect:** after a failed call, SpaRs matches what it matched before the call; spaCy may match a subset or superset of it.
- **Tests:** `invalid_additions_change_nothing`, `phrase_attributes_validate_pattern_documents`, `removing_an_id_with_removed_and_live_rules_changes_nothing`.

### Empty ID separator

- **spaCy:** accepts an empty `ent_id_sep`. Matching still works, but once any pattern is stored, reading labels, IDs or the pattern listing, and saving the ruler, raise a `ValueError`, because splitting a rule name on an empty separator fails.
- **SpaRs:** `EntityRuler::new` and `EntityRuler::with_lexicon` return an error for an empty `id_separator`; both use the same check.
- **Reason:** a ruler whose patterns cannot be listed or saved is not fully usable, so the error is reported where the setting is chosen.
- **Effect:** configurations that spaCy can annotate with, but whose patterns it cannot list, are rejected when the ruler is created.
- **Tests:** `empty_id_separator_is_rejected` (through `EntityRuler::new`).

## Token matchers

These apply to the Rust and Node [Token Matcher](TOKEN_MATCHER.md), [DependencyMatcher](DEPENDENCY_MATCHER.md) and the token patterns of the [`EntityRuler`](ENTITY_RULER.md), which share one annotation check, described in [missing annotations](TOKEN_MATCHER.md#missing-annotations).

### Annotations required by removed rules

- **spaCy:** `Matcher.add` records every attribute that a pattern compares with a direct value, such as `{"LEMMA": "x"}`, in an internal set that `Matcher.remove` never clears. Before matching, spaCy raises error E155 if any recorded tag, POS, morphology, lemma or dependency attribute is missing from every token of the document. After a rule is removed, its attributes therefore still require annotations, even when no remaining rule compares them, and even after every rule is removed. The DependencyMatcher checks its conditions through an internal `Matcher` with the same set. The EntityRuler's `remove` keeps it; its `clear` creates a new `Matcher` and so resets it. The [reproduction](#reproducing-the-removed-rule-check) below shows it.
- **SpaRs:** checks only the conditions of the rules registered when matching runs, so removing a rule also removes the annotations it required.
- **Reason:** the recorded set is stale state, not part of the documented matching contract: it makes a matcher's errors depend on rules it no longer has, and a removed rule cannot affect any match.
- **Effect:** after `remove`, if a document lacks an annotation that only removed rules compared, SpaRs matches it against the current rules, while spaCy raises E155. Matches on documents that spaCy accepts are the same.
- **Tests:** `removed_rules_do_not_require_annotations` in `crates/spars/src/token_matcher/tests.rs` and `crates/spars/src/dependency_matcher/tests.rs`, and `removed_token_patterns_do_not_require_annotations` in `crates/spars/src/entity_ruler/tests.rs`. Run them with `cargo test --offline -p spars-nlp --lib -- removed_rules_do_not_require_annotations removed_token_patterns_do_not_require_annotations`.

## Reproducing the remove bug

With spaCy 3.8.14, using a blank English pipeline:

```python
import spacy

nlp = spacy.blank("en")
ruler = nlp.add_pipe("entity_ruler")
ruler.add_patterns([
    {"label": "ORG", "pattern": "Acme", "id": "acme"},
    {"label": "ORG", "pattern": [{"LOWER": "acme"}, {"LOWER": "corp"}], "id": "acme"},
])
ruler.remove("acme")
print(ruler.patterns)  # []
print([(e.text, e.label_, e.ent_id_) for e in nlp("Acme Corp").ents])  # [('Acme Corp', 'ORG', 'acme')], expected []
ruler.remove("acme")  # succeeds, and only now removes the token pattern
print([(e.text, e.label_, e.ent_id_) for e in nlp("Acme Corp").ents])  # []
```

The cause is the final loop of `EntityRuler.remove` in `spacy/pipeline/entityruler.py`, which uses `if label in self.phrase_matcher: ... else: self.matcher.remove(label)`. Removing the label from each matcher that contains it, as `SpanRuler.remove_by_id` does, fixes it.

## Reproducing the removed-rule check

With spaCy 3.8.14, using a blank English pipeline, whose documents have no lemmas:

```python
import spacy
from spacy.matcher import Matcher

nlp = spacy.blank("en")
matcher = Matcher(nlp.vocab)
matcher.add("LEMMA", [[{"LEMMA": "x"}]])
matcher.add("TEXT", [[{"ORTH": "a"}]])
matcher.remove("LEMMA")
matcher(nlp("a b"))  # raises ValueError [E155], although no rule compares LEMMA
```

The recorded set is `Matcher._seen_attrs` in `spacy/matcher/matcher.pyx`: `add` inserts into it and `remove` does not change it.

## Span groups

These apply to Rust [span groups](SPAN_GROUPS.md). Unit tests are in `crates/spars/src/document/span_groups/tests.rs` and replay and snapshot tests in `crates/spars/tests/span_groups.rs`; run them with `cargo test --release --offline -p spars-nlp --lib span_groups` and `cargo test --release --offline -p spars-nlp --test span_groups`.

### Deleting a span by index

- **spaCy:** `del group[i]` erases the span at position `i - 1` after normalizing a negative index, so `del group[1]` removes the first span, `del group[-1]` removes the second-to-last, and `del group[0]` erases the position before the first span, which is undefined behavior in C++. This is a bug in spaCy 3.8.14 (`SpanGroup.__delitem__` in `spacy/tokens/span_group.pyx`), and the code is unchanged on spaCy's `master` branch at commit `c2dabfce56ad` (2026-09-30). spaCy's own `test_span_doc_delitem` passes despite it, because it checks only the length and the spans from index `i` onward, which are the same whichever of the two spans was removed. The [reproduction](#reproducing-the-span-deletion-bug) below shows it.
- **SpaRs:** a group's spans are a `Vec`, so `group.spans.remove(i)` removes the span at index `i` and panics for an index out of range, as Rust's `Vec::remove` does.
- **Reason:** the documented purpose of `del group[i]` is to delete the span at index `i`, and the upstream behavior cannot be reproduced safely for index 0.
- **Effect:** code that deletes spans by index removes a different span than spaCy 3.8.14 does. The fixture contains no deletion by index.
- **Tests:** none; Rust's `Vec::remove` defines the behavior.

### Groups are values

- **spaCy:** `doc.spans[key]` returns the stored `SpanGroup` object, so appending to it or replacing its spans changes the document directly, and assigning one group object to two keys shares it between them.
- **SpaRs:** `Doc::span_groups` returns groups by shared reference (`&SpanGroups`), so they cannot be edited in place. To change one, callers clone it, edit the copy and store it with `Doc::set_span_group`, and each key holds its own group.
- **Reason:** storing through one checked method lets the document validate every span when it is stored, so the document's groups are always within bounds, and fits Rust ownership without shared mutable state.
- **Effect:** an edit made to a copy has no effect until it is stored, and storing one group under two keys stores two independent groups.
- **Tests:** `stored_groups_are_independent_values`; the compiler prevents editing a group through `Doc::span_groups`.

### Copies keep empty spans

- **spaCy:** `Doc.copy()` rebuilds group spans from character offsets and raises `ValueError` E1052 for an empty span (`start == end`) at the start or end of the document, or one that follows a token with trailing whitespace. An empty span after a token without trailing whitespace is copied.
- **SpaRs:** `Doc::clone` copies every group, including empty spans.
- **Reason:** spaCy allows empty spans in groups, so a copy that cannot hold them is a limitation of its offset-based copy rather than a rule about groups.
- **Effect:** some documents whose groups contain empty spans can be copied in SpaRs but not in spaCy.
- **Tests:** `copies_and_snapshots_keep_empty_spans`.

### Snapshots keep key order

- **spaCy:** `Doc.to_bytes` writes groups with identical content (name, attrs and spans) once, listing their keys, so loading moves a later key next to the first one with the same content.
- **SpaRs:** native snapshots store every group under its own key, in key order.
- **Reason:** saving and loading should not change a document, and native snapshots are a separate format from spaCy's bytes.
- **Effect:** after a round trip, key order in SpaRs matches the saved document; in spaCy it can change.
- **Tests:** the snapshot round trip after every step of `official_span_group_edits`.

### Attributes are JSON values compared as text

- **spaCy:** `SpanGroup.attrs` is a Python dict that can hold any value, and `Doc.to_bytes` saves values msgpack can encode, such as bytes. Dicts compare regardless of key order, and `1 == 1.0`.
- **SpaRs:** attrs hold JSON values, given and returned as JSON text, and compare equal only when their keys are in the same order and their values have the same text. `{"a": 1}` and `{"a":1}` differ.
- **Reason:** keeping each value as the text given preserves key order inside nested objects without an extra dependency, and JSON is what native snapshots can store.
- **Effect:** non-JSON values cannot be stored, and attrs that are equal as Python dicts can compare unequal in SpaRs. Because SpaRs compares `SpanGroup` and `Doc` by value, this also affects their equality; spaCy compares those objects by identity.
- **Tests:** `attrs_compare_as_json_text`, `attrs_compare_order_and_text`, `attrs_keep_insertion_order_and_replace_in_place`, `attrs_reject_repeated_keys_and_non_objects`.

### Spans and groups from another document

- **spaCy:** spans and groups belong to a document; building a group from another document's spans, or adding them to a group, raises `ValueError` E855. Assigning another document's `SpanGroup` to `doc.spans[key]` fails an `assert` instead (`AssertionError`, skipped under `python -O`).
- **SpaRs:** spans are token ranges with no document, so a group built for another document is accepted when its spans fit; `Doc::set_span_group` returns `Error::Bounds` for spans outside the document.
- **Reason:** SpaRs spans carry no document reference, and bounds are the property the document can check.
- **Effect:** code that would fail in spaCy for mixing documents succeeds in SpaRs when the spans fit, and the spans then refer to this document's tokens.
- **Tests:** `groups_built_for_another_document_are_checked_when_stored`.

### No knowledge-base IDs

- **spaCy:** span group members carry a `kb_id`.
- **SpaRs:** `Span` has no knowledge-base ID, in groups or elsewhere.
- **Reason:** SpaRs stores no knowledge-base IDs anywhere ([entity editing limits](ENTITIES.md#limits)); adding them would extend `Span` for every use at once.
- **Effect:** `kb_id` values cannot be stored in span groups.
- **Tests:** none; `Span` has no such field.

### Extending a group with itself

- **spaCy:** `group.extend(group)` and `group += group` insert a vector's range into the same vector, which is undefined behavior in C++.
- **SpaRs:** Rust's borrow rules reject `group.extend_group(&group)`; extending with `group.clone()` appends the spans once.
- **Reason:** undefined behavior cannot be reproduced, and appending a copy matches extending with any other group.
- **Effect:** code that extends a group with a copy of itself gets each span repeated once.
- **Tests:** `extend_group_adds_spans_and_missing_attrs`.

## Reproducing the span deletion bug

With spaCy 3.8.14, using a blank English pipeline:

```python
import spacy

nlp = spacy.blank("en")
doc = nlp("a b c d")
doc.spans["g"] = [doc[0:1], doc[1:2], doc[2:3]]
del doc.spans["g"][1]
print([s.text for s in doc.spans["g"]])  # ['b', 'c'], expected ['a', 'c']
doc.spans["g"] = [doc[0:1], doc[1:2], doc[2:3]]
del doc.spans["g"][-1]
print([s.text for s in doc.spans["g"]])  # ['a', 'c'], expected ['a', 'b']
```

The cause is `self.c.erase(self.c.begin() + i - 1)` in `SpanGroup.__delitem__`; erasing `self.c.begin() + i` fixes it.

## SpanRuler

These apply to the Rust [`SpanRuler`](SPAN_RULER.md). Its other behavior follows spaCy, including the ordering of identical spans by string-store hashes and the repeated spans a filter adds to an existing group, so neither is listed here. The unit tests are in `crates/spars/src/span_ruler/tests.rs`; run them with `cargo test --release --offline -p spars-nlp --lib span_ruler`.

### Failed additions change nothing

- **spaCy:** `SpanRuler.add_patterns` walks the patterns once: it records each pattern's label and ID, adds a token pattern to the matcher, then lists the pattern; phrases are added to the phrase matcher after the walk. An invalid token pattern raises at that pattern: earlier token patterns match, earlier phrases are listed and counted by `len` but never match, the rejected and later patterns are not listed, and the labels up to and including the rejected pattern are recorded, so `label in ruler` reports the rejected label. A phrase rejected for missing annotation raises after every pattern of the call is listed and every label recorded, with every token pattern and the earlier phrases added to their matchers.
- **SpaRs:** `add` checks every pattern first and returns an error without changing the ruler, including the labels `contains` reports.
- **Reason:** a caller that handles the error can keep using the ruler without inspecting what was partly changed, as with the [EntityRuler](#failed-additions-and-removals-change-nothing). `remove` and `remove_by_id` already check before changing anything in spaCy, so they need no difference.
- **Effect:** after a failed `add`, SpaRs matches and lists what it did before the call; spaCy may match more, list patterns that never match, and report labels of patterns it did not add.
- **Tests:** `invalid_additions_change_nothing`, `phrase_attributes_validate_pattern_documents`.
