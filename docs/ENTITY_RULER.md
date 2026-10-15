# Add entities with rules

`EntityRuler` adds named entities from patterns you write, following spaCy 3.8.14's `EntityRuler` (the `entity_ruler` pipeline component). Use it to add entities the statistical recognizer misses, to correct its labels, to give entities stable IDs, or to recognize entities with rules alone. A pattern is either a [token pattern](TOKEN_MATCHER.md), which describes a sequence of tokens, or a phrase, which matches an exact sequence of token values like the [PhraseMatcher](PHRASE_MATCHER.md). Each pattern has a label and may have an ID, which the ruler writes to the entities it adds as described in [entity IDs](ENTITIES.md#entity-ids).

The ruler annotates documents you have already processed, like spaCy's ruler placed after the recognizer in a pipeline. To use it without the recognizer, process text with `Model::process_until` and a stage before `Stage::Ner`, such as `Stage::Tokenizer`. The ruler itself needs no model. The example below processes text first, so it needs the `en_core_web_md` export at `assets/en_core_web_md-3.8.0`, relative to the repository root, from the [development setup](DEVELOPMENT.md) or the [model installer](MODEL_INSTALLATION.md).

```rust
use spars::{EntityPattern, EntityPatternKind, EntityRuler, EntityRulerOptions, Model, Predicate,
    Repetition, TokenAttribute, TokenConstraint, TokenPattern, TokenPatternItem};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let nlp = Model::load("assets/en_core_web_md-3.8.0")?;
    let doc = nlp.process("Tim Cook opened Apple Park in Cupertino.")?;
    let entities = |doc: &spars::Doc| -> Vec<(String, Option<String>)> {
        doc.entities().unwrap().iter().map(|e| (e.label.clone(), e.id.clone())).collect()
    };

    // Phrase patterns are documents processed like the documents the ruler annotates.
    let campus = nlp.process("Apple Park")?;
    let city = TokenPattern { tokens: vec![TokenPatternItem {
        constraints: vec![TokenConstraint {
            attribute: TokenAttribute::Lower,
            predicate: Predicate::Equals { value: "cupertino".into() },
        }],
        repetition: Repetition::Once,
    }] };
    let patterns = [
        EntityPattern {
            label: "FAC".into(),
            id: Some("apple-park".into()),
            pattern: EntityPatternKind::Phrase(&campus),
        },
        EntityPattern { label: "CITY".into(), id: None, pattern: EntityPatternKind::Tokens(city) },
    ];

    // The recognizer already labeled Apple Park FAC and Cupertino GPE. By default the ruler skips
    // matches on tokens that belong to an entity, so both matches are skipped.
    let mut ruler = EntityRuler::new(EntityRulerOptions::default())?;
    ruler.add(&patterns)?;
    assert_eq!(entities(&ruler.annotated(&doc)?), [
        ("PERSON".to_owned(), None), ("FAC".to_owned(), None), ("GPE".to_owned(), None),
    ]);

    // With overwriting, matches replace the entities they overlap.
    let mut options = EntityRulerOptions::default();
    options.overwrite_entities = true;
    let mut ruler = EntityRuler::new(options)?;
    ruler.add(&patterns)?;
    let mut doc = doc;
    ruler.annotate(&mut doc)?;
    assert_eq!(entities(&doc), [
        ("PERSON".to_owned(), None),
        ("FAC".to_owned(), Some("apple-park".to_owned())),
        ("CITY".to_owned(), None),
    ]);
    // Token 3, "Apple", carries the ID of its entity.
    assert_eq!(doc.tokens()[3].entity_id.as_deref(), Some("apple-park"));
    Ok(())
}
```

`annotate` changes a document you own; `annotated` returns an annotated copy and leaves the original unchanged. `find_matches` returns the candidate matches before any filtering.

## How matches become entities

The ruler finds every token-pattern and phrase match, then:

1. Drops empty matches and repeated matches of the same rule over the same tokens.
2. Orders the rest longest first, then by earliest start.
3. Takes each match in that order. Without `overwrite_entities`, a match is skipped if any of its tokens already has an entity type. A match that overlaps a match the ruler already accepted is dropped. Otherwise it is accepted, and with `overwrite_entities` every existing entity it overlaps is removed whole, even the tokens outside the match.
4. Writes the remaining existing entities and the accepted matches with [`Doc::set_entities`](ENTITIES.md) and the default `EntityDefault::Outside`.

The last step means every token outside the resulting entities becomes outside (IOB `O`, the outside tag; see [token states](ENTITIES.md#token-states)), even when nothing matched. Tokens whose entity annotation was missing, such as those of a document that never ran the recognizer, and blocked tokens, which the recognizer must not use, both become outside. Blocked tokens have no entity type, so a match on them is accepted. A match from a pattern with an empty label is accepted like any other, so it claims its tokens and can remove overwritten entities, but `set_entities` adds no entity for it, as in spaCy.

A pattern's ID is written to the tokens of its entity. An entity from a pattern without an ID writes no ID, so its tokens keep any ID they already had, and the entity reports the ID of its first token.

## Patterns, labels and IDs

`EntityRulerOptions` holds spaCy's `entity_ruler` settings: `overwrite_entities` (`overwrite_ents`, default `false`), `phrase_attribute` (`phrase_matcher_attr`, the token value phrases compare, default `ORTH`) and `id_separator` (`ent_id_sep`, default `||`). Create the ruler with `EntityRuler::with_lexicon` and a model's [`Lexicon`](TOKEN_MATCHER.md#match-lexical-flags) to use lexical flag conditions or a lexical flag phrase attribute.

Process phrase pattern text with the same model stages as the documents the ruler annotates, as spaCy runs phrase patterns through the components before the ruler. This matters for annotation attributes such as `LEMMA`, which need the pattern document's lemmas. A phrase is compared only on `phrase_attribute`.

Like spaCy, the ruler stores a pattern with an ID under the rule name `label + id_separator + id`, and reads labels and IDs back from those names:

- `labels()` lists the sorted distinct labels and `ids()` the sorted distinct IDs. A rule name is split at its last separator, so a label added without an ID that contains the separator, such as `A||B`, is listed as label `A` with ID `B`. Its matches are still labeled `A||B` without an ID, unless a pattern with label `A` and ID `B` is also added.
- Patterns whose rule names coincide share one rule. A phrase labeled `A||B` without an ID and a phrase labeled `A` with ID `B` both produce entities labeled `A` with ID `B`. When one addition gives the same rule name two different label and ID pairs, the pair of its last phrase wins, or of its last token pattern if it has no phrase.
- A phrase with an empty ID has no ID. A token pattern with an empty ID is stored under `label + id_separator`, so `ids()` lists `""` and `remove("")` removes it; its matches still have no ID.
- `patterns()` lists token patterns, then phrases, each grouped by rule name in the order each name first received a token pattern (or a phrase). Phrases are listed by their document text. `len()` counts every added pattern, including duplicates and empty phrases.

`remove(id)` removes every pattern with that ID. It returns an error when the ID was never added. Removed rule names stay known, as in spaCy: removing the same ID again fails unless a pattern has since been added under one of its rule names, a later label containing the separator can inherit a removed rule's label and ID, and after an ID is re-added under a different label, removing it fails because the old rule name has no patterns. `clear()` removes every pattern, forgets removed rule names, and keeps the options and lexicon. `add` checks every pattern before changing the ruler: an invalid token pattern, a lexical flag without a lexicon, or a phrase document without the annotation the phrase attribute compares returns an error and adds nothing.

## Differences from spaCy

These differences are SpaRs's intended behavior; [accepted differences from spaCy](SPACY_DIFFERENCES.md#entityruler) gives spaCy's behavior, the reason and the tests for each.

- **Ties between identical spans.** When patterns with different labels or IDs match exactly the same tokens, SpaRs keeps the rule that received its first pattern earliest; a rule removed and added again counts from its new addition. spaCy's choice depends on hashes of the rule names.
- **Removing a rule with both kinds of pattern.** `remove` removes an ID's token patterns and phrases at once, so a second `remove` of that ID fails. spaCy 3.8.14 removes only the phrases from matching, so the token patterns keep producing entities until a second `remove`.
- **Failed changes.** A failed `add` or `remove` changes nothing, where spaCy can be left partly changed.
- **Empty separator.** An empty ID separator is rejected when the ruler is created, where spaCy fails later when labels, IDs or patterns are read once it has patterns.
- **Not provided.** The `__contains__` check, the `validate` setting (SpaRs always checks patterns), fuzzy matching (`matcher_fuzzy_compare`), `initialize`, scoring, loading or saving patterns (JSONL, `to_disk`, `to_bytes`), spaCy's pattern-dictionary format, knowledge-base IDs, the warning for a ruler without patterns, and running the ruler as a pipeline component or before the recognizer. Callers apply the ruler to processed documents.

## Reuse and errors

Annotating does not change the ruler, so one ruler can annotate many documents. Adding, removing and clearing patterns need `&mut EntityRuler`. Errors from matching or from `set_entities`, such as a token IOB tag other than `B`, `I` or `O`, which only assigning the public field directly can produce, leave the document unchanged.

## Verification

`fixtures/entity-ruler-v1.expected.json` records spaCy 3.8.14's matches, entity annotation, IDs, pattern listings and errors for 61 cases: 161 steps with 75 ruler applications, 176 matches and 5 rejected removals. 31 cases are authored, including cases adapted from spaCy's `test_entity_ruler.py`; 4 start from `en_core_web_md` 3.8.0 output, including one without the recognizer and one comparing lemmas; 26 are random. Each document, phrase pattern document and result is recorded, so replaying the fixture needs no model. `crates/spars/tests/entity_ruler.rs` replays every step and checks `find_matches`, `annotate`, `annotated`, `len`, `labels`, `ids`, the pattern listings, errors and a snapshot round trip of every result; its model-dependent test checks lexical flags through `with_lexicon` and `clear`. Unit tests in `crates/spars/src/entity_ruler/tests.rs` cover the differences above and rejected input. `tools/test_entity_ruler_reference.py` regenerates the fixture with spaCy, compares it with the frozen file, and recomputes every annotation from the recorded matches with plain Python; it needs the reference environment from the [development setup](DEVELOPMENT.md) with `en_core_web_md` 3.8.0. Run them with:

```sh
cargo test --release --offline -p spars-nlp --test entity_ruler -- --include-ignored
cargo test --release --offline -p spars-nlp --lib entity_ruler
.venv/bin/python -m unittest discover -s tools -p 'test_entity_ruler_reference.py'
```

`crates/spars/examples/entity_ruler_benchmark.rs` times the ruler against the two matchers it uses, on generated documents and patterns; run it with `cargo run --release -p spars-nlp --example entity_ruler_benchmark -- TOKENS PATTERNS REPEATS EXISTING`.
