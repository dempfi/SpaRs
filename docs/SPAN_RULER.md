# Add spans with rules

`SpanRuler` adds labeled spans from patterns you write, following spaCy 3.8.14's `SpanRuler` (the `span_ruler` pipeline component). It stores its matches as a [span group](SPAN_GROUPS.md), a named list of spans that may overlap, and can also write them as named entities. Use it to mark phrases for later processing, to collect candidate spans that overlap, or, with entity annotation turned on, as spaCy's rule-based alternative to the [EntityRuler](ENTITY_RULER.md). A pattern is either a [token pattern](TOKEN_MATCHER.md), which describes a sequence of tokens, or a phrase, which matches an exact sequence of token values like the [PhraseMatcher](PHRASE_MATCHER.md). Each pattern has a label and may have an ID, which the ruler writes to the spans and entities it adds. This guide uses the Rust API; Node provides the same ruler as [`SpanRuler`](NODE.md#add-spans-with-rules), with options named in camel case and filters given short forms of spaCy's registered names, such as `'first_longest'` for `spacy.first_longest_spans_filter.v1`.

The ruler annotates documents you have already processed. To write entities without the recognizer's predictions, process text with `Model::process_until` and a stage before `Stage::Ner`, such as `Stage::Tokenizer`. The ruler itself needs no model, except for lexical flag conditions and flag phrase attributes. The example below processes text first, so it needs the `en_core_web_md` export at `assets/en_core_web_md-3.8.0`, relative to the repository root, from the [development setup](DEVELOPMENT.md) or the [model installer](MODEL_INSTALLATION.md).

```rust
use spars::{Model, Predicate, Repetition, SpanFilter, SpanPattern, SpanPatternKind, SpanRuler,
    SpanRulerOptions, TokenAttribute, TokenConstraint, TokenPattern, TokenPatternItem};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let nlp = Model::load("assets/en_core_web_md-3.8.0")?;
    let doc = nlp.process("Tim Cook opened Apple Park in Cupertino.")?;

    // Phrase patterns are documents processed like the documents the ruler annotates.
    let campus = nlp.process("Apple Park")?;
    let park = nlp.process("Park")?;
    let city = TokenPattern { tokens: vec![TokenPatternItem {
        constraints: vec![TokenConstraint {
            attribute: TokenAttribute::Lower,
            predicate: Predicate::Equals { value: "cupertino".into() },
        }],
        repetition: Repetition::Once,
    }] };
    let patterns = [
        SpanPattern {
            label: "FAC".into(),
            id: Some("apple-park".into()),
            pattern: SpanPatternKind::Phrase(&campus),
        },
        SpanPattern { label: "PLACE".into(), id: None, pattern: SpanPatternKind::Phrase(&park) },
        SpanPattern { label: "CITY".into(), id: None, pattern: SpanPatternKind::Tokens(city) },
    ];

    // By default every match, overlapping or not, goes to the span group "ruler", and the
    // entities are left alone.
    let mut ruler = SpanRuler::new(SpanRulerOptions::default());
    ruler.add(&patterns)?;
    let annotated = ruler.annotated(&doc)?;
    let spans: Vec<(usize, usize, &str, Option<&str>)> = annotated.span_groups().get("ruler")
        .unwrap().spans.iter()
        .map(|s| (s.start.0, s.end.0, s.label.as_str(), s.id.as_deref())).collect();
    assert_eq!(spans, [
        (3, 5, "FAC", Some("apple-park")), (4, 5, "PLACE", None), (6, 7, "CITY", None),
    ]);
    assert_eq!(annotated.entities(), doc.entities());

    // Write entities instead of a span group. Keep the recognizer's entities (`overwrite` off) and
    // let matches replace those they overlap (`PrioritizeNew`); with the default first-longest
    // filter, the recognizer's FAC entity, which has no ID, would win the tie. A shorter match
    // inside a longer one is dropped.
    let mut options = SpanRulerOptions::default();
    options.spans_key = None;
    options.annotate_entities = true;
    options.entities_filter = SpanFilter::PrioritizeNew;
    options.overwrite = false;
    let mut ruler = SpanRuler::new(options);
    ruler.add(&patterns)?;
    let mut doc = doc;
    ruler.annotate(&mut doc)?;
    let entities: Vec<(&str, Option<&str>)> = doc.entities().unwrap().iter()
        .map(|e| (e.label.as_str(), e.id.as_deref())).collect();
    assert_eq!(entities, [("PERSON", None), ("FAC", Some("apple-park")), ("CITY", None)]);
    assert!(doc.span_groups().is_empty());
    Ok(())
}
```

`annotate` changes a document you own; `annotated` returns an annotated copy and leaves the original unchanged. `find_matches` returns the matches before any filtering.

## Options

`SpanRulerOptions` holds spaCy's `span_ruler` settings, with the factory defaults that `nlp.add_pipe("span_ruler")` uses:

| Option | spaCy setting | Default | Effect |
| --- | --- | --- | --- |
| `spans_key` | `spans_key` | `Some("ruler")` | The span group key the ruler writes. `None` or an empty key writes no group. |
| `spans_filter` | `spans_filter` | `None` | A [filter](#filters) for the span group; without one every match is added. |
| `annotate_entities` | `annotate_ents` | `false` | Whether matches are also written as entities. |
| `entities_filter` | `ents_filter` | `SpanFilter::FirstLongest` | The [filter](#filters) for entities, which must not overlap. |
| `phrase_attribute` | `phrase_matcher_attr` | `PhraseAttribute::Orth` | The token value phrases compare. |
| `overwrite` | `overwrite` | `true` | Whether existing spans under the key and existing entities are discarded first. |

Create the ruler with `SpanRuler::with_lexicon` and a model's [`Lexicon`](TOKEN_MATCHER.md#match-lexical-flags) to use lexical flag conditions or a lexical flag phrase attribute. Process phrase pattern text with the same model stages as the documents the ruler annotates, as spaCy runs phrase patterns through the components before the ruler; this matters for annotation attributes such as `LEMMA`. A phrase is compared only on `phrase_attribute`.

## How matches are found

`find_matches` collects every token-pattern and phrase match, drops empty matches and repeated matches of the same label and ID over the same tokens, and orders the rest by start token, then end token. Matches over exactly the same tokens are ordered by spaCy's string-store hashes (the 64-bit number spaCy assigns each string) of their labels, then of their IDs, as spaCy sorts its `Span` objects. Labels that are built-in spaCy symbols, such as `ORG` or `PERSON`, have small fixed numbers and come before other labels. This order decides which of two identical spans a filter keeps.

## Span groups

When `spans_key` is set and not empty, each annotation stores a group under that key:

- With `overwrite` (the default), or when the key has no group yet, the group is new: it is named after the key, has no attrs, and holds the matches or, with `spans_filter`, the filter's result for the matches alone, since there are no existing spans to compare. A replaced key keeps its position among the document's keys; a new key goes last.
- Without `overwrite`, an existing group keeps its name and attrs and is extended. Without a filter it gains every match. With a filter, the filter compares the existing spans with the matches, and spaCy appends the filter's whole result, which repeats the existing spans it keeps. SpaRs does the same. Applying the ruler again without `overwrite` therefore appends its matches again and, with a filter, also repeats the existing spans the filter keeps.

Span groups never change entities. A pattern with an empty label adds spans with an empty label.

## Entities

With `annotate_entities`, the ruler also writes entities with [`Doc::set_entities`](ENTITIES.md) and the default `EntityDefault::Outside`, as spaCy's `doc.ents` setter does. Without `overwrite`, the document's current entities compete with the matches in `entities_filter`; with `overwrite`, they are discarded first. Every token outside the resulting entities becomes outside (IOB `O`), including tokens whose entity annotation was missing or blocked, even when nothing matched. A match with an empty label takes part in filtering, so it can claim tokens and remove entities, but no entity is added for it. A match without an ID writes no ID, so its tokens keep any ID they already had, and the entity reports the ID of its first token.

## Filters

`SpanFilter` names spaCy's registered filters. Each orders candidates longest first, then by earliest start, keeping their input order when both are equal:

- `FirstLongest` (`spacy.first_longest_spans_filter.v1`) lets the existing spans and the matches compete together, existing spans first among equals. A candidate is dropped when its first or last token is already taken. The result is ordered by start. An empty existing span is kept unless the token at its position or the one before it is taken.
- `PrioritizeNew` (`spacy.prioritize_new_ents_filter.v1`) accepts a match when none of its tokens is taken by an accepted match, and removes every existing span that an accepted match overlaps. The result lists the remaining existing spans, then the accepted matches. This reproduces spaCy's EntityRuler with `overwrite_ents=True`; SpaRs's EntityRuler orders identical spans of different rules by addition instead (see [its differences](ENTITY_RULER.md#differences-from-spacy)).
- `PrioritizeExisting` (`spacy.prioritize_existing_ents_filter.v1`) is the same, except that the existing spans' tokens count as taken before any match is considered, so every existing span is kept and the result lists them, then the accepted matches. This reproduces spaCy's EntityRuler with `overwrite_ents=False`.

## Patterns, labels and IDs

Patterns with the same label and ID form one matcher rule; an ID of `None` and one of `Some("")` match the same way and give matches no ID. The other methods follow spaCy:

- `patterns()` lists every pattern in the order it was added, with its ID exactly as given; phrases are listed by their document text. `len()` counts every pattern, including duplicates and empty phrases.
- `labels()` lists the sorted distinct labels and `ids()` the sorted distinct IDs of the current patterns. A pattern added with `Some("")` contributes `""` to `ids()`; one added with `None` contributes nothing.
- `contains(label)` reports whether the label was ever added. As in spaCy, a label stays known after its patterns are removed or cleared.
- `remove(label)` removes every pattern with that label. It returns an error only for a label that was never added, so removing a label again, or after `clear()`, succeeds and changes nothing.
- `remove_by_id(id)` removes every pattern added with that ID and returns an error when no current pattern has it. As in spaCy, `remove_by_id("")` also stops every pattern added without an ID from matching, whatever its label, because both match under the empty ID, while `patterns()`, `len()` and `labels()` still list them; adding a pattern with the same label and no ID makes only the new one match.
- `clear()` removes every pattern and keeps the options, the lexicon and the known labels.

`add` checks every pattern before changing the ruler: an invalid token pattern, a lexical flag without a lexicon, or a phrase document without the annotation the phrase attribute compares returns an error and adds nothing.

## Differences from spaCy

[Accepted differences from spaCy](SPACY_DIFFERENCES.md#spanruler) gives spaCy's behavior, the reason and the tests for each difference.

- **Failed additions.** A failed `add` changes nothing, including the labels `contains` reports. spaCy can be left with some patterns added, some listed without matching, and labels recorded for patterns it did not add.
- **Not provided.** Custom filter functions (only spaCy's three registered filters), the `validate` setting (SpaRs always checks patterns), fuzzy matching (`matcher_fuzzy_compare`), `initialize`, scoring, loading or saving patterns (JSONL, `to_disk`, `to_bytes`), spaCy's pattern-dictionary format, knowledge-base IDs, the warning for a ruler without patterns, and running the ruler as a pipeline component. Callers apply the ruler to processed documents.

## Reuse and errors

Annotating does not change the ruler, so one ruler can annotate many documents. Adding, removing and clearing patterns need `&mut SpanRuler`. Errors from matching or from `set_entities`, such as a token IOB tag other than `B`, `I` or `O`, which only assigning the public field directly can produce, leave the document unchanged, including its span groups.

## Verification

`fixtures/span-ruler-v1.expected.json` records spaCy 3.8.14's matches, span groups, entity annotation, pattern listings, `contains` results and errors for 100 cases: 378 steps with 188 ruler applications, 799 matches and 15 rejected removals. 39 cases are authored, including cases adapted from spaCy's `test_span_ruler.py`; 5 start from `en_core_web_md` 3.8.0 output, including one without the recognizer and one comparing lemmas; 56 are random, with random options, existing entities, existing span groups and removals. Each document, phrase pattern document and result is recorded, so replaying the fixture needs no model. `crates/spars/tests/span_ruler.rs` replays every step and checks `find_matches`, `annotate`, `annotated`, `len`, `labels`, `ids`, `contains`, the pattern listings, errors and a snapshot round trip of every result; its model-dependent test checks lexical flags through `with_lexicon` and `clear`. `bindings/node/tests/span-ruler.test.mts` replays the same steps through Node, comparing `findMatches`, `annotate`, the option getters, the listings, `contains` and errors, and checks option validation, malformed patterns, pending-search rejection and lexical flags with a model. Unit tests in `crates/spars/src/span_ruler/tests.rs` cover the filters' edge cases, the difference above and rejected input. `tools/test_span_ruler_reference.py` regenerates the fixture with spaCy, compares it with the frozen file, and recomputes every span group and entity annotation from the recorded matches with plain Python; it needs the reference environment from the [development setup](DEVELOPMENT.md) with `en_core_web_md` 3.8.0. Run them with:

```sh
cargo test --release --offline -p spars-nlp --test span_ruler -- --include-ignored
cargo test --release --offline -p spars-nlp --lib span_ruler
.venv/bin/python -m unittest discover -s tools -p 'test_span_ruler_reference.py'
npm --prefix bindings/node run build
node --expose-gc --test bindings/node/tests/span-ruler.test.mts
```

The Node suite loads `en_core_web_md` for its lexical-flag and admission tests, so it fails without the model assets from the [development setup](DEVELOPMENT.md).

`crates/spars/examples/span_ruler_benchmark.rs` times the ruler against the two matchers it uses and, for entities, the EntityRuler, on generated documents and patterns; run it with `cargo run --release -p spars-nlp --example span_ruler_benchmark -- TOKENS PATTERNS REPEATS MODE`, where `MODE` is `spans`, `filtered` or `entities`.
