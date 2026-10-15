# Accepted differences from spaCy

SpaRs follows spaCy 3.8.14 by default, and a behavior that differs from it is normally a compatibility gap listed in [the compatibility inventory](COMPATIBILITY.md). This file lists the differences that are accepted as SpaRs's intended behavior instead. Each one exists because spaCy's behavior depends on Python internals that are impractical to emulate, is an upstream bug, leaves an object partly changed after an error, or fails later than the setting that causes the failure. The reference fixtures contain no case whose result these differences change; the unit tests named below check SpaRs's behavior.

Each entry gives spaCy's behavior, SpaRs's behavior, the reason, and the effect on callers.

## EntityRuler

These apply to the Rust [`EntityRuler`](ENTITY_RULER.md). A rule is the group of patterns with the same label and ID; spaCy names it `label + ent_id_sep + id`, or just the label without an ID, and SpaRs does the same. The pattern listing is spaCy's `patterns` property and SpaRs's `patterns()`. The unit tests are in `crates/spars/src/entity_ruler/tests.rs`; run them with `cargo test --release --offline -p spars-nlp --lib entity_ruler`.

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
