# Match token phrases

`PhraseMatcher` compiles tokenized document patterns into a reusable prefix tree. It compares exact token text with `ORTH`/`TEXT`, pinned Unicode lowercase with `LOWER`, the token norm with `NORM`, the linguistic annotations `LEMMA`, `POS`, `TAG`, `DEP` and `MORPH`, a lexical flag such as `IS_ALPHA`, or the token `LENGTH`. Matching never downloads assets or runs inference; annotations come from documents you have already processed. Patterns and input may come from different models or validated native snapshots; values are compared as strings and do not depend on a shared vocabulary; lexical flags use the language rules in the model lexicon given to the matcher.

## Register and match

Complete the model setup in the [developer guide](DEVELOPMENT.md) to run this example. Tokenization is explicit here; applications with existing documents can register and search those directly.

```rust
use spars::{Model, PhraseAttribute, PhraseMatcher, PhraseRuleId, Stage, TokenIndex};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let model = Model::load("assets/en_core_web_md-3.8.0")?;
    let short = model.process_until("New", Stage::Tokenizer)?;
    let long = model.process_until("New York", Stage::Tokenizer)?;
    let input = model.process_until("New York and New York", Stage::Tokenizer)?;
    let mut matcher = PhraseMatcher::with_attribute(PhraseAttribute::Orth);
    matcher.add("place", &[&short, &long, &long])?;
    assert_eq!(matcher.len(), 1);
    assert_eq!(matcher.get("place").unwrap().len(), 2);
    let matches = matcher.find_matches(&input)?;
    assert_eq!(matches.len(), 4);
    assert_eq!(matches[0].rule, PhraseRuleId::from("place"));
    assert_eq!((matches[0].start, matches[0].end), (TokenIndex(0), TokenIndex(1)));
    assert_eq!(input.span_text(matches[1].start, matches[1].end)?, "New York");
    matcher.remove("place")?;
    assert!(matcher.find_matches(&input)?.is_empty());
    Ok(())
}
```

The result includes both “New” and “New York” at each occurrence. End indices are exclusive token indices, not byte or character offsets. Trailing spaces between tokens do not participate in comparisons; a whitespace token does. Sentence boundaries do not prevent a match.

## Match lowercase token text

`PhraseAttribute::Lower` applies the pinned Python Unicode 15.0.0 lowercase mapping to each complete token in both patterns and input. This includes context-sensitive Greek final sigma and multi-character mappings such as `İ` to `i` plus a combining dot. It does not perform case folding or Unicode normalization: `ß` and `ss`, and composed `é` and decomposed `e` plus an accent, remain different. Original document text and offsets remain unchanged.

```rust
use spars::{Model, PhraseAttribute, PhraseMatcher, Stage, TokenIndex};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let model = Model::load("assets/en_core_web_md-3.8.0")?;
    let pattern = model.process_until("the ritz", Stage::Tokenizer)?;
    let input = model.process_until("The Ritz", Stage::Tokenizer)?;
    let mut matcher = PhraseMatcher::with_attribute(PhraseAttribute::Lower);
    matcher.add("hotel", &[&pattern])?;
    let matches = matcher.find_matches(&input)?;
    assert_eq!(matches.len(), 1);
    assert_eq!((matches[0].start, matches[0].end), (TokenIndex(0), TokenIndex(2)));
    assert_eq!(input.span_text(matches[0].start, matches[0].end)?, "The Ritz");
    assert_eq!(matcher.get("hotel").unwrap()[0].tokens(), &["the", "ritz"]);
    Ok(())
}
```

`get` returns the stored lowercase token strings for a LOWER matcher; patterns with the same lowercase token sequence are deduplicated. The standalone resource is bundled with the Rust crate, so matching restored documents does not require loading a model. `crates/spars/tests/phrase_matcher_lower.rs` and the Node phrase suite compare 4 official cases, 24 lifecycle states and 351 ordered matches. Regenerate and compare both the resource and fixture with `.venv/bin/python tools/phrase_lower_reference.py --check`.

## Match token annotations

`PhraseAttribute::Lemma`, `Pos`, `Tag`, `Dep` and `Morph` compare each token's lemma, universal part-of-speech tag (such as `NOUN`), fine-grained tag (such as `NNS`), dependency label or morphological features instead of its text, like spaCy's attributes of the same names. `Norm` compares the token norm, which can differ from the text: the tokenizer splits `gonna` into `gon` and `na`, whose norms are `going` and `to`. Pattern documents must carry the annotation, so process them with the model rather than only tokenizing them. This example finds any form of “the dog runs”.

```rust
use spars::{Model, PhraseAttribute, PhraseMatcher, TokenIndex};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let model = Model::load("assets/en_core_web_md-3.8.0")?;
    let pattern = model.process("the dog runs")?;
    let input = model.process("The dogs ran home.")?;
    let mut matcher = PhraseMatcher::with_attribute(PhraseAttribute::Lemma);
    matcher.add("dog-runs", &[&pattern])?;
    assert_eq!(matcher.get("dog-runs").unwrap()[0].tokens(), &["the", "dog", "run"]);
    let matches = matcher.find_matches(&input)?;
    assert_eq!(matches.len(), 1);
    assert_eq!((matches[0].start, matches[0].end), (TokenIndex(0), TokenIndex(3)));
    assert_eq!(input.span_text(matches[0].start, matches[0].end)?, "The dogs ran");
    Ok(())
}
```

Each pattern token must match the corresponding input token's value exactly, so a `Tag` pattern for “the dog runs” (`DT NN VBZ`) does not match “The dogs ran” (`DT NNS VBD`). Because the model annotates a pattern on its own, without the sentence around it, its tags or dependency labels can differ from the same words inside a longer text. This matters most for `Dep`, because a short pattern is parsed as a sentence of its own. Morphology is compared as one string of features, such as `Number=Plur`; a token whose analysis has no features has the value `_`.

The rules for missing annotations follow spaCy, except that a rejected pattern leaves the matcher unchanged:

- A nonempty pattern document needs the annotation on at least one token. Otherwise `add` returns `Error::MissingAnnotation` and changes nothing; spaCy registers the rule name before raising this error. A document that was only tokenized, for example with `Stage::Tokenizer`, has no annotations. `Norm` never needs annotations.
- A token without the annotation has the empty value `""`. It matches only a pattern token that also lacks it. `get` shows those values as `""`.
- The input needs no annotations. A tokenized-only input is not an error; it matches only patterns whose tokens all lack the annotation, which is never the case for a pattern that was accepted, so it returns no matches.
- For `Lemma`, `Pos`, `Tag` and `Dep`, an empty string, for example a lemma of `""` in a snapshot, counts as missing, as in spaCy. An empty morphological analysis is not missing; it is `_`. `Norm` compares a snapshot's norm as stored, even when it is empty; spaCy would use the word's default norm instead.

Document morphology must be in spaCy's canonical order, as the model writes it; `Morph` returns `Error::Unsupported` for a pattern or input document with noncanonical morphology, like the other matchers. Snapshots store the empty analysis as `""`, so a literal `_` in a snapshot is rejected too. The [frozen reference corpus](../fixtures/README.md#phrase-matching) for these attributes covers 18 cases, 204 lifecycle states, 705 ordered matches and 45 rejected additions of pattern documents without the annotation; run `cargo test --release --offline --test phrase_matcher_annotations -- --nocapture`.

## Match lexical flags and length

The lexical flag attributes compare whether each token has a property, such as being alphabetic or looking like a number. All seventeen of spaCy's flags are available: `IsAlpha`, `IsAscii`, `IsDigit`, `IsLower`, `IsUpper`, `IsTitle`, `IsPunct`, `IsSpace`, `IsBracket`, `IsQuote`, `IsLeftPunct`, `IsRightPunct`, `IsCurrency`, `IsStop`, `LikeNum`, `LikeUrl` and `LikeEmail`, with the meanings described in the [token matcher guide](TOKEN_MATCHER.md). `PhraseAttribute::Length` compares the number of characters (Unicode code points) in each token. A pattern then matches any sequence of tokens with the same flags or lengths, whatever the words are. This example finds a number-like token followed by a token that is not number-like, then every three-character token.

```rust
use spars::{Model, PhraseAttribute, PhraseMatcher, Stage, TokenIndex};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let model = Model::load("assets/en_core_web_md-3.8.0")?;
    let pattern = model.process_until("3 apples", Stage::Tokenizer)?;
    let input = model.process_until("I ate 3 apples and ten pears.", Stage::Tokenizer)?;
    let mut matcher = PhraseMatcher::with_lexicon(PhraseAttribute::LikeNum, model.lexicon());
    matcher.add("count", &[&pattern])?;
    assert_eq!(matcher.get("count").unwrap()[0].tokens(), &["true", "false"]);
    let found: Vec<_> = matcher
        .find_matches(&input)?
        .iter()
        .map(|m| input.span_text(m.start, m.end))
        .collect::<Result<_, _>>()?;
    assert_eq!(found, ["3 apples", "ten pears"]);
    let mut length = PhraseMatcher::with_attribute(PhraseAttribute::Length);
    length.add("three-letters", &[&model.process_until("cat", Stage::Tokenizer)?])?;
    let matches = length.find_matches(&input)?;
    assert_eq!((matches[0].start, matches[0].end), (TokenIndex(1), TokenIndex(2)));
    assert_eq!(length.get("three-letters").unwrap()[0].tokens(), &["3"]);
    Ok(())
}
```

“ten” counts as a number because English `LIKE_NUM` accepts number words. The `Length` matcher matches every three-character token: “ate”, “and” and “ten”.

Flags depend on the language, for example English number words for `LikeNum`, so a flag matcher needs the model's lexicon: create it with `PhraseMatcher::with_lexicon`, as spaCy creates a matcher from the model's vocabulary. Adding a rule to a flag matcher created without one returns `Error::Pattern` and changes nothing. `Length` needs no lexicon. Flags and lengths come from the token text, so documents need no annotations; tokenizing is enough. `get` shows flag values as `true` and `false` and lengths as decimal numbers; spaCy stores them as the integers 1, 0 and the length.

spaCy 3.8.14 stores flag values and the length of a one-character token as the integers 0 and 1, which its internal hash table reserves. Removing the last rule that uses such a value leaves a pointer to freed memory, and can also drop other rules that share the start of the phrase; the next match or addition may then crash or return wrong results. SpaRs removes these rules normally. The [frozen reference corpus](../fixtures/README.md#phrase-matching) therefore adds rules without removing them; removal does not depend on the attribute and is covered by the other phrase suites. It covers all seventeen flags and `LENGTH` with 36 cases, 180 lifecycle states and 11,956 ordered matches; run `cargo test --release --offline --test phrase_matcher_lexical -- --include-ignored --nocapture` with the model assets.

## Ordering and rule lifecycle

`new()` selects `PhraseAttribute::Orth`; `Text` is the synonymous exact-text choice. Adding an existing rule accumulates patterns. Identical pattern/rule pairs emit only one occurrence; different rules may match the same span. Empty documents used as patterns are ignored, but an empty pattern collection still registers the rule. `contains`, `len`, `is_empty` and `remove` manage rules. Removing an unknown name returns an error. `get` is a Rust inspection API returning unique nonempty patterns in first-registration order; spaCy PhraseMatcher has no corresponding public `get` method.

Results follow spaCy 3.8.14: increasing start index, then increasing end index, then preshed 3.0.13 terminal-table order for rules on the same span. The final tie order depends on label hashes, table growth, deletions and re-registration. It is neither alphabetical nor insertion order. Empty and reserved-symbol rule names are supported. Matchers can be shared across concurrent read-only calls; every call owns its result vector.

The [frozen reference corpus](../fixtures/README.md#phrase-matching) covers 43 cases, 1,249 lifecycle states and 31,919 ordered outputs. Run `cargo test --release --offline --test phrase_matcher -- --nocapture` for explicit denominators. The acceptance script regenerates official expectations separately and runs this guide from its Markdown source.

## Rust boundaries and remaining scope

Patterns are borrowed, validated `Doc` objects; the matcher copies the selected value of each token, applying lowercase for LOWER. Python's raw integer arrays, invalid dynamic objects, callbacks and integer rule IDs are not accepted. Rust validates fallible inputs before changing registration state; spaCy may register a rule and earlier patterns before an invalid later item raises an error. Invalid native snapshots fail at `Doc::from_json`, before registration. Distinct rule names that collide in their resolved 64-bit identity return an error instead of sharing an upstream identity. Token text uses full string equality rather than accepting hash collisions or truncating a pattern when a token hash equals spaCy's reserved end-of-phrase marker. These are explicit safer Rust differences, not parity claims for those inputs.

spaCy's other PhraseMatcher attributes, `SHAPE`, the entity attributes (`ENT_TYPE` and others), `SENT_START` and `SPACY`, are not supported, and unsupported serialized attribute names are rejected. Names must be uppercase, while spaCy also accepts lowercase. Validation warnings, span input, labeled-span output, matcher serialization and callbacks are not exposed. The [Node binding](NODE.md) exposes the same attributes and rule lifecycle through immutable native documents. The remaining shared matcher options are planned in the [implementation sequence](PROGRESS.md#implementation-sequence), and the [EntityRuler](ENTITY_RULER.md) and [SpanRuler](SPAN_RULER.md) build on this matcher; `SHAPE`, the entity attributes, `SENT_START` and `SPACY` are not yet scheduled.

Compilation and retained storage grow with the total pattern text and shared-prefix tree. Search costs up to document length times the longest matching prefix, plus terminal-table scans and output size; dense overlaps may return many results. A terminal table retains its capacity while any label remains, so matching a heavily pruned dictionary can cost more than matching a freshly built equivalent. Removal prunes unused nodes and releases their contents, while the node arena retains reusable slots. See [performance measurements](PERFORMANCE.md) for reproducible measurement guidance.
