# SpaRs implementation status

## English inference

SpaRs implements native Rust inference for `en_core_web_sm`, `en_core_web_md`, and `en_core_web_lg` 3.8.0, using spaCy 3.8.14 and Thinc 8.3.13 as the reference. The supported pipeline includes tokenization, lexical features, neural encoding, tags, dependencies, attribute rules, lemmas, entities, sentences, noun chunks, static vectors for `md`/`lg`, and contextual vectors for `sm`. The [compatibility inventory](COMPATIBILITY.md) lists remaining library features.

## Current evidence

| Check | Verified scope |
|---|---|
| Original md full-pipeline suites | 44 documents / 1,035 tokens |
| Expanded domain and length coverage | 98 documents / 5,568 tokens, including documents of 896 and 3,584 tokens |
| Tokenizer boundary regressions | 24 documents / 120 tokens |
| Fresh post-fix comparison | 24 documents / 306 tokens; evaluated after the tokenizer correction |
| Combined full-pipeline agreement | 190 documents / 7,029 tokens; exact discrete outputs |
| Tokenizer cases | 4,057 cases / 14,737 tokens |
| Lexical properties | 4,374 cases |
| Regular-expression matching | 52,236 comparisons over 13,059 texts |
| Parser and entity action traces | 634 action choices, context IDs, and valid-action masks |
| New robustness checks | 9 malformed configurations, 4 damaged tensors, 27 malformed snapshots, 3 invalid UTF-8 offsets, and 28 processing results across repeated, batched, and concurrent calls |
| Dependency traversal | 98 documents / 5,568 tokens compared with official children, ancestors, subtree order, and sentence spans |
| Token Matcher | 85 reference cases / 5,960 rule registrations / 9,589 ordered matches against official spaCy, including Unicode `LOWER`, all lexical flags and `LENGTH` |
| Additional sm/lg full-pipeline suites | 98 documents / 5,568 tokens each; separate frozen official references, exact annotations and vector tolerances in `crates/spars/tests/model_parity.rs` |
| Model capability and order checks | Independent model identity, declared pipeline order, rejected dependencies, and independent contextual token/span vectors in `crates/spars/tests/model_loading.rs` and `crates/spars/tests/model_parity.rs` |
| Rust Markdown examples | README and guide examples execute through `tools/check_docs.py` |
| Node-API binding | 190 documents / 7,029 tokens, all mapped annotations and offset units; eight static-vector lookup cases; typed API, async lifecycle/error tests, spaCy processing defaults and lazy iteration in `bindings/node/tests/processing-parity.test.mts`, default bounded inference admission in `bindings/node/tests/queue.test.mts` and input limits in `bindings/node/tests/input-limits.test.mts` |
| TypeScript guide examples | Node guide examples type-check and execute through `tools/check_docs.py` |

The reference comparisons require exact token annotations and spans. Floating-point calculations use the limits in [validation](VALIDATION.md). Each run records the observed numerical differences in its generated reports. Eight static-vector lookup cases and six similarity pairs also pass.

The [acceptance script](../tools/verify.py) checks strict Python and TypeScript types, the typing policy, formatting, Clippy, Rust, Python and Node tests, source doc tests, Rust and TypeScript examples executed from Markdown, the separate native consumer, packaging, and a byte-identical model re-export. One source doc example is compile-only; it is not counted among the executed Markdown examples. The Rust consumer runs with no interpreters on its PATH. Run `.venv/bin/python tools/verify.py` after setup to generate evidence under ignored `target/reports/`. CI uploads these files as run artifacts; see the [quality process](QUALITY.md).

## Strong typing

Runtime model configuration uses typed Rust records and enums instead of generic JSON values. Python tools use strict Pyright checking, named records, and checked conversions at external-data boundaries. Regression tests reject wrong field types, unknown operators, invalid null constraints, booleans used as integers, and typing-policy bypasses. Legacy v1 assets and existing frozen reference expectations remain unchanged; additional models use v2.

## What the expanded evaluation found

The first 98-document run passed 97 documents. The remaining case contained `Wait—didn't`. SpaRs applied a contraction exception after splitting on the dash, while spaCy preserved the whole contraction there. The fixed input and expected output remain in the [evaluation fixtures](../fixtures/README.md), together with the boundary regressions that exercise the correction.

The correction follows spaCy's two-pass tokenizer. The second pass uses the same filtered rule set and matches the token sequence produced without exception handling. This also fixes related boundaries such as `He's-word`. Expected outputs stayed unchanged. All 98 cases and the 24 new boundary regressions now pass; the later 24-document comparison also passed without further runtime changes.

These are synthetic, project-authored examples. They broaden coverage but do not estimate accuracy on a random sample of real-world text. Once used, a comparison set becomes a regression suite rather than an untouched future test set.

## Model provenance

The official `en_core_web_md` 3.8.0 wheel has SHA-256 `5e6329fe3fecedb1d1a02c3ea2172ee0fede6cea6e4aefb6a02d832dba78a310`. Other supported releases are pinned in [the model catalog](../models/catalog.json). The medium-model exporter copies 69 F32 tensors, configuration, linguistic resources, and license notices. Source hashes verified against the official wheel are recorded in [source-lock.json](../reference/source-lock.json). For spaCy 3.8.14, wheel-shipped source was used because a source archive was unavailable at acquisition.

## Fixture generator limitation

The current lexical generator produces 5,367 cases and its regex generator produces 16,038 texts, while the frozen suites contain 4,374 lexical cases and 13,059 regex texts. This difference predates the typing changes: the previous and typed generators produce byte-identical outputs with the current exported resources. Existing fixture inputs and expectations remain frozen. Reconcile generator input provenance before claiming that every frozen suite can be regenerated from the current scripts.

## Performance and remaining work

See [performance measurements](PERFORMANCE.md) for commands to measure loading time, throughput, and peak process memory. The implementation uses native CPU matrix kernels and processes documents sequentially. Each measurement applies to its recorded machine, source version, and corpus; it does not promise a particular speed for other workloads.

Dependency traversal, sentence access, the [typed DependencyMatcher](DEPENDENCY_MATCHER.md), and the [Token Matcher](TOKEN_MATCHER.md) are implemented. Token Matcher supports shared text, annotation, lexical flag and length conditions with repetition and overlapping results. DependencyMatcher verification covers all 20 relationships and its declared token-condition subset, including ordered results and supplementary morphology regressions. Wider matcher compatibility remains partial. The next capabilities, in priority order, are:

1. Add EntityRuler and SpanRuler (deliveries A2 and A3), building on the implemented entity annotation updates (A1).
2. Store token string annotations once per document, as described in [interned token strings](#interned-token-strings).
3. Add the remaining matcher selection options, regex and fuzzy predicates (deliveries M4 and M5).
4. Add dependency-checked component selection, then measured neural batching.
5. Add independent sentence segmentation: Sentencizer, then trainable senter.
6. Resume multilingual model expansion after these four milestones. Catalog-selected installation, capability-declared loading, and configurable execution order are implemented for the English CNN family; additional component sets, languages and architectures remain planned.

The [implementation sequence](#implementation-sequence) defines dependencies and acceptance for these planned milestones. Token merge/split editing, broader spaCy serialization, custom-trained pipeline loading, and training remain later work. Checked annotation updates do not imply retokenization support. The all-official-pipelines target, including transformer architectures, remains unchanged; this sequence defers multilingual expansion without restricting shared component designs to English. Follow the [quality process](QUALITY.md): every feature needs its own reference cases, failure tests, performance review, and runnable example before it is marked verified.

The separate [Node binding](NODE.md) exposes loading, processing, batches, ordered stages, explicit native model downloads, offline model-store management, immutable native documents and snapshots, token/span views, dependency traversal, token/span/document vectors and similarity, lexical queries, and the three native matchers. Its document, traversal, lexical, vector and matcher suites reuse frozen official references; boundary tests cover invalid input and ownership. Per-platform package verification remains required. The [npm workflow](RELEASING.md#publish-the-node-package) assembles Linux x64/ARM64 glibc and musl, macOS x64/ARM64, and Windows x64/ARM64 packages with clean-install checks before optional publication; a completed workflow and registry checks are required release evidence. Browser WASM remains unimplemented. These bindings reuse the native library; they do not change the broader spaCy compatibility backlog.

### Interned token strings

Every string annotation (`norm`, `tag`, `pos`, `morphology`, `lemma`, `dep`, entity IOB tag, type and ID) is an owned `String` stored separately on each `Token`, so each annotation adds a pointer, length and capacity per token plus a heap allocation for each non-empty value, and copies, snapshots and Node conversion pay for every one. Adding entity IDs (A2) measurably slowed copying, snapshot loading and `toObject` on long documents, even when no token has an ID. spaCy instead stores each distinct string once in a shared `StringStore` and gives tokens integer hashes. The planned change stores each distinct string once per document (or shared vocabulary), keeps small integer symbols on tokens, and reads values through methods rather than public fields. Knowledge-base IDs would add another per-token string, and span groups (A3) would add per-span label and ID strings, which the same table can hold. `Token` and `Span` are `#[non_exhaustive]` and built with `Token::new` and `Span::new`, so new fields do not break callers; changing public fields to methods is still a breaking change. Acceptance: snapshot format, Node output and fixture results unchanged; copying, snapshot loading and saving, and `toObject` no slower than before entity IDs were added, beyond observed variation; no measured regression in inference, model loading, entity updates or the three matchers; documents with many distinct strings and with entity IDs included; alternated measurements on short and long documents with their spread; memory reported as inline token size, `estimated_heap_bytes` and process RSS. A table shared between documents also needs call-isolation tests and a concurrent benchmark.

## Implementation sequence

Deliveries M1, M2, M3 and A1 are implemented; A2, A3, M4, M5 and the later milestones are planned, not implemented or verified. Rulers depend only on the verified token and phrase pattern subset, so A1–A3 follow M3 and precede M4 and M5, which add selection options and predicate kinds that rulers do not require. Complete each as a bounded change; do not combine a new annotation contract, a neural algorithm change, and model conversion into one acceptance result. Preserve the existing English sm/md/lg fixtures, exact discrete outputs, numerical tolerances, offline inference, and immutable reusable models throughout.

### 1. Shared matcher conditions and options

The three native matchers and their reference suites are described in the [matching implementation plan](#matching-implementation-plan). Deliver the remaining matcher work in this order:

| Delivery | Scope | Acceptance |
|---|---|---|
| M1 (implemented) | Shared Unicode LOWER for TokenMatcher and DependencyMatcher, reusing the pinned lowercase resources where semantics agree | Ordered official outputs for Unicode expansions, combining characters, empty documents, repeated calls, and malformed conditions; existing PhraseMatcher LOWER results unchanged. Evidence: `token-match-lower-v1` and `dependency-match-lower-v1` fixtures through Rust and Node, with predicate unit tests for malformed conditions |
| M2 (implemented) | Inventory missing attributes and predicates across all three matchers; add typed lexical flags, starting with IS_ALPHA, IS_DIGIT, IS_SPACE, IS_PUNCT and LIKE_NUM; then LENGTH and numeric comparisons | Positive and negative cases for every added attribute/operator, including Unicode length semantics and invalid value types; no implicit string conversion for flags or numbers. Evidence: IS_ALPHA, IS_DIGIT, IS_SPACE, IS_PUNCT and LIKE_NUM through a model lexicon, with `token-match-flags-v1` and `dependency-match-flags-v1`; the twelve other lexical flags, with `token-match-more-flags-v1` and `dependency-match-more-flags-v1`; LENGTH with numeric comparisons and integer sets, with `token-match-length-v1` and `dependency-match-length-v1`; all in Rust and Node |
| M3 (implemented) | Set comparisons (`IS_SUBSET`, `IS_SUPERSET` and `INTERSECTS` on all string attributes and `LENGTH`; implemented), then PhraseMatcher attributes, one attribute family at a time: NORM and the LEMMA, POS, TAG, DEP and MORPH annotations, then the lexical flags and LENGTH (all implemented) | Missing annotations, invalid attribute choices, pattern/input consistency, and lifecycle ordering match the declared upstream contract. Evidence for set comparisons: `token-match-sets-v1` and `dependency-match-sets-v1` through Rust and Node, with an independent set calculation in the reference tool tests and predicate unit tests for invalid value types and attributes. Evidence for PhraseMatcher annotation attributes: `phrase-match-annotations-v1` through Rust and Node, with stored pattern values, missing-annotation errors and matches recomputed without spaCy in the reference tool tests. Evidence for PhraseMatcher lexical flags and LENGTH: `phrase-match-lexical-v1` through Rust and Node, with stored values and matches recomputed from spaCy lexeme values in the reference tool tests; flags use the model lexicon |
| M4 | TokenMatcher greedy FIRST/LONGEST, then alignments, checked span input and labeled-span results where the corresponding spaCy matcher supports them, including PhraseMatcher labeled-span output | Overlap and repetition tie-breaking, duplicate matches, alignment lengths, unavailable annotations and option combinations agree exactly; PhraseMatcher span labels, boundaries and result ordering match the pinned reference; span-relative and document-relative indices are tested separately for each API; existing default behavior and typed Rust callers remain compatible |
| M5 | Regex and fuzzy predicates as separate deliveries | Pinned reference semantics for search versus full match, Unicode, distance limits and supported operator combinations, checked before selecting an engine or algorithm; unsupported regex constructs reject explicitly |

Share attribute extraction and predicate logic where semantics agree, while retaining each matcher's validation rules. Use concrete types for text, flags, numbers and morphology rather than converting every value to a string. Every added condition needs official positive, negative, missing-annotation, malformed-input and Unicode cases in each affected matcher, and existing matcher outputs must remain unchanged.

Primary Rust paths are `crates/spars/src/dependency_matcher/predicates.rs`, `crates/spars/src/token_matcher/`, `crates/spars/src/phrase_matcher/`, and lexical resources. Inspect the pinned matcher source and tests before choosing each representation. Extend Node types and boundary tests for each exposed feature using the same frozen references. Callback invocation order and safe mutation build on the annotation-update contract from A1. Callbacks, custom extensions, spaCy pattern-JSON import and integer rule-ID interoperability remain outside this milestone and stay in the backlog; do not make them prerequisites for rulers.

### 2. Checked annotations and rule-based annotation

Annotation updates are a prerequisite for rulers and independent sentence segmentation. Inspect the pinned `Doc`, `Token`, `Span`, `SpanGroup`, EntityRuler and SpanRuler sources and tests before choosing the public editing API. Current Rust documents expose immutable borrowed views, while Node holds immutable native handles; preserve these ownership guarantees by defining whether each operation consumes a document, requires exclusive access, or returns a new document. Node must keep existing handles and views valid. Reject invalid edits before changing observable state, and document any difference from upstream failure behavior.

| Delivery | Scope | Acceptance |
|---|---|---|
| A1 (implemented) | Checked entity annotation replacement and the ownership contract for annotation updates | Entity spans, token entity types and IOB tags (inside, outside or beginning of an entity) remain consistent; distinguish missing, empty, blocked and outside states where supported; invalid bounds, overlaps and failed edits preserve the stated contract; text and offsets remain exact. Evidence: `entity-updates-v1` through Rust and Node, including one case of updates to `en_core_web_md` predictions, with every state recomputed without spaCy in the reference tool tests. Rust updates take exclusive access (`&mut Doc`); Node returns a new document and keeps existing handles valid. See the [entity editing guide](ENTITIES.md#limits) |
| A2 | EntityRuler applied after NER or on a document without entity predictions, using the verified token and phrase pattern subset | Official precedence, overlap filtering, overwrite policy, rule lifecycle and entity IDs; complete document comparisons and snapshot round-trips, including entity IDs. Implemented so far: entity ID storage (tokens and entities carry spaCy's IDs, `set_entities` and `withEntities` write them, and version-3 snapshots keep them), checked by `entity-ids-v1` through Rust and Node, with every state recomputed without spaCy |
| A3 | Named span groups, followed by SpanRuler | Overlapping spans, labels, IDs, group replacement/append behavior and filtering match the supported options; groups and annotations survive snapshots without losing metadata |
| A4 | EntityRuler before NER, with preset-entity support in the native recognizer | Reference action traces and final annotations establish that NER respects existing entities and blocking; unsupported preset states fail explicitly |

Primary paths are `crates/spars/src/document.rs`, document validation and traversal, `crates/spars/src/ner.rs`, and pipeline configuration. Put new ruler implementations in focused modules under `crates/spars/src/`. Version snapshots when adding fields requires a new storage contract, preserve existing snapshots, and test malformed new fields. Define cache invalidation for affected derived annotations; annotation edits must not silently leave sentence, entity, noun-chunk or traversal data inconsistent. Retokenization and arbitrary mutation callbacks are separate work.

### 3. Selective execution and neural batching

The current executor follows a declared component order, `process_until` selects an inclusive prefix, and Rust `pipe` processes one document at a time. Component selection and batching are separate deliveries with separate evidence.

| Delivery | Scope | Acceptance |
|---|---|---|
| P1 | Typed component selection with explicit dependency validation; preserve existing default and prefix APIs | NER-only, tokenizer-only and valid shared-encoder subsets match equivalent pinned spaCy configurations; missing dependencies, unknown components and unavailable output annotations fail clearly; default full outputs remain unchanged |
| P2 | Profile sequential processing and preserve an identifiable release-build baseline; batch one measured shared neural stage first | Independent numerical checks and exact final outputs across batch sizes, mixed document lengths, empty inputs, document boundaries and repeated/concurrent calls; no cross-document convolution context |
| P3 | Extend useful batching to other measured stages and expose bounded ordered streaming in Rust and Node | Input/output order, partial final batches, errors, cancellation where exposed, limits on accepted work and pausing input when processing falls behind have tested contracts; memory stays bounded by documented batch controls |

Primary paths are `crates/spars/src/pipeline.rs`, `crates/spars/src/config/`, `crates/spars/src/neural.rs`, the encoder implementation, and Node processing APIs. Inspect pinned `Language.pipe`, component `pipe` methods and Thinc's sequence batching before changing algorithms. Component selection must not silently enable omitted components or fabricate annotations; loading fewer tensors is a separate optimization from skipping execution. P1 must also support the ruler component order established in milestone 2 without introducing model-name branches.

Measure loading, inference throughput and process memory separately on the same models, corpus, build mode, thread limits and warmup. Include short/long and repeated/unfamiliar text, alternate before/after order, and inspect variation. A batch API alone does not establish a speedup. Preserve exact discrete outputs and existing floating-point limits; resolve measured regressions and batch-dependent prediction changes before completion. Keep measurements under ignored `target/reports/` and follow [performance guidance](PERFORMANCE.md).

### 4. Independent sentence segmentation

Reuse the annotation ownership contract from A1 and component selection from P1. Inspect pinned `sentencizer.pyx`, `senter.pyx`, their tests, and parser handling of preset sentence boundaries. Keep sentence segmentation available without dependency parsing; this does not make noun chunks or dependency traversal available.

| Delivery | Scope | Acceptance |
|---|---|---|
| S1 | Checked sentence-start updates and rule-based Sentencizer with configurable punctuation and overwrite behavior | Official boundaries for empty text, whitespace, punctuation runs, quotes, Unicode and existing boundaries; sentence spans and token flags agree, invalid updates preserve state, and snapshots retain annotations |
| S2 | Pipeline integration, including parser interaction with preset sentence boundaries | Tokenizer-plus-Sentencizer works without neural inference; unsupported component orders reject; parser constraints and final outputs agree with upstream when segmentation precedes parsing |
| S3 | Trainable senter with typed component/architecture configuration and native package conversion | Export the currently excluded component from a pinned official English package; compare converted parameters, neural outputs, sentence decisions, disabled/enabled defaults and malformed inputs; retain existing full-pipeline parity |

Primary paths are document annotations, pipeline configuration, `crates/spars/src/parser.rs`, new sentence component modules, `tools/export.py`, and `crates/spars-model/`. Sentence-start values must retain the distinction between unknown and explicitly false. Sentencizer does not require learned weights, but model-independent tokenizer acquisition/loading is a separate capability and must not be implied by this milestone. Measure both sentence-only paths against parser-based segmentation for time and memory; reference agreement is not a claim of better linguistic accuracy.

### Completion gates and subsequent scope

Before each runtime delivery, verify the relevant source against `reference/source-lock.json`, freeze new official reference cases separately from existing fixtures, and demonstrate the missing behavior with a failing test. For algorithm changes, establish the baseline and independent equivalence checks instead. Include malformed inputs, unavailable annotations, Unicode, call isolation, and snapshot or binding behavior where affected. Do not mark a milestone complete while any listed delivery remains unsupported or lacks evidence.

Run focused checks, executed Rust and TypeScript guide examples for changed public APIs, and `.venv/bin/python tools/verify.py` with required model assets and model-dependent tests included. Obtain independent reference, test, documentation and performance reviews under [QUALITY.md](QUALITY.md). Update relevant guides, README, model/snapshot format documentation and `COMPATIBILITY.md` only for behavior actually delivered. Record exact-commit CI evidence separately from local acceptance; no plan entry implies passing CI.

Multilingual expansion follows completion of milestones 1–4 using the [model extensibility plan](#model-extensibility-plan). Components added here must remain reusable across model identities and languages. Transformer support remains required by the broader model target; retokenization, DocBin interoperability, custom-trained pipeline loading and training retain separate contracts and acceptance work.

## Matching implementation plan

Token Matcher, DependencyMatcher and [PhraseMatcher](PHRASE_MATCHER.md) have native implementations and frozen reference suites. PhraseMatcher implements ORTH/TEXT/LOWER/NORM, the LEMMA/POS/TAG/DEP/MORPH annotations, the seventeen lexical flags and LENGTH, and Node exposes all three matchers. Steps 1–2 below retain the acceptance requirements for the implemented PhraseMatcher, step 3 records the implemented Node bindings, and step 4 describes the subsequent rule-based annotation milestone. Remaining matcher conditions and options are planned as deliveries M4 and M5 in the [implementation sequence](#1-shared-matcher-conditions-and-options); M1 (shared `LOWER`), M2 (lexical flags, `LENGTH` and numeric comparisons), and M3 (set comparisons and PhraseMatcher attributes) are implemented.

### 1. PhraseMatcher reference contract (implemented)

Inspect spaCy 3.8.14's `spacy/matcher/phrasematcher.pyx`, `matcher.pyx`, `dependencymatcher.pyx`, their attribute and hashing dependencies, and `spacy/tests/matcher/`. Verify the installed reference against `reference/source-lock.json`. Retain the [third-party notices](../THIRD_PARTY_NOTICES.md), including the FlashText adaptation notice and the preshed license.

Create a separate versioned phrase-matching corpus and official reference generator. Cover nested and overlapping phrases, repeated words, duplicate patterns and labels, repeated registration, removal with shared prefixes, empty patterns and documents, Unicode, whitespace, sentence boundaries, missing annotations, and invalid options. Probe ordering when several rules end at the same position; derive it from the pinned implementation rather than assuming insertion order. Include registration state after errors. Preserve all existing matcher fixtures.

Acceptance: reference generation is reproducible, case counts are explicit, and the first tests demonstrate the missing native behavior. Reserve an untouched comparison set until the initial implementation is ready; record when it becomes regression data.

### 2. Native PhraseMatcher (implemented for ORTH/TEXT/LOWER/NORM, annotations, lexical flags and LENGTH)

Begin with tokenized document patterns and exact token text (`ORTH`/`TEXT`), then add case-insensitive `LOWER` in a separate increment. Use a typed attribute choice and rule identifiers, compile reusable pattern storage, and keep search state local to each call. Resolve how attributes and string identities remain consistent across pattern and input documents, including restored documents. Use the pinned Unicode lowercase behavior for `LOWER`. Matching must not run inference or download assets implicitly.

Provide registration, lookup, removal, and checked document matching with typed token-index results. Evaluate a token trie that shares phrase prefixes against the upstream design; finalize the data structure after the ordering contract and baseline measurements are available. Preserve overlapping results, duplicate handling, and registration/removal behavior from the reference. Record any intentional safer Rust error-state behavior as a compatibility difference.

Acceptance: exact ordered results and rule lifecycle behavior match every declared reference case. Empty inputs, malformed patterns, repeated calls, and concurrent read-only calls pass. Add a Rust example to a PhraseMatcher guide and execute it through `tools/check_docs.py`. Keep unsupported attributes and options explicit.

Exact-text ORTH/TEXT matching is implemented in `crates/spars/src/phrase_matcher/`, with a safe prefix trie and pinned terminal-table ordering. `crates/spars/tests/phrase_matcher.rs` compares 43 official cases, 1,249 lifecycle states and 31,919 exact ordered matches, including a separately frozen comparison set. `tools/phrase_match_reference.py` regenerates references and verifies source provenance; the guide example executes through `tools/check_docs.py`. LOWER uses model-independent Unicode 15.0.0 resources; `crates/spars/tests/phrase_matcher_lower.rs` compares 4 official cases, 24 lifecycle states and 351 ordered matches. `tools/phrase_lower_reference.py --check` checks resource and fixture reproducibility. NORM and the LEMMA, POS, TAG, DEP and MORPH annotations compare spaCy's stored value strings, with `""` for a missing value and `_` for the empty morphological analysis; `crates/spars/tests/phrase_matcher_annotations.rs` compares 18 official cases, 204 lifecycle states, 705 ordered matches and 45 rejected additions of pattern documents without the annotation. The seventeen lexical flags and LENGTH compute values from token text, with flags using the model lexicon; `crates/spars/tests/phrase_matcher_lexical.rs` compares 36 official cases, 180 lifecycle states and 11,956 ordered matches. Rust accepts validated document patterns, rejects distinct rule-name hash collisions, and validates before mutation; see the guide for differences from Python error states and hash-only token identity.

### 3. Matching through Node (implemented for the declared subset)

The Node APIs expose reusable native PhraseMatcher, TokenMatcher and DependencyMatcher instances. Matching receives immutable `NativeDocument` handles, preserves rule identities and token-index results, and runs through the binding's bounded execution policy. Mutable plain JavaScript output objects are not accepted as native documents. See the [Node guide](NODE.md) for pattern types, rule operations and supported options.

Acceptance: run the same reference cases through Rust and Node, test ownership and repeated/concurrent use, and execute TypeScript guide examples from Markdown. Verify the packed Node package outside the repository.

### 4. Add rule-based annotation after document-editing contracts

Build EntityRuler and SpanRuler on the verified matchers once document annotation updates are supported. Deliveries A1–A4 in the [implementation sequence](#2-checked-annotations-and-rule-based-annotation) define the order. First define conflict resolution, overwrite policy, entity IOB updates, span groups, and token/span view validity against spaCy. Retokenization has separate merge/split and dependency-update requirements and is not a prerequisite for read-only matching.

Acceptance: official comparisons verify complete documents after rule application, including overlaps and existing annotations. Failed edits preserve the documented state contract, and native snapshots retain the resulting annotations.

### Quality and performance gates

For every increment, follow `docs/QUALITY.md` and obtain reference, test, documentation, and performance reviews. Run focused cases followed by `.venv/bin/python tools/verify.py`; CI must execute new parity tests with explicit denominators. Keep generated mismatch and timing reports under ignored `target/reports/` and update the compatibility inventory only for behavior actually verified.

Measure pattern compilation, removal, matching throughput, and memory separately. Cover small and large dictionaries, shared prefixes, short and long documents, Unicode, no-match inputs, and dense overlapping output. Record input size, pattern count/length, and output count; returning many matches has an unavoidable cost. Preserve baseline binaries for changes to existing matchers, compare under identical conditions without competing builds, and resolve measured regressions without weakening parity requirements. The next bounded deliveries are the ruler deliveries A2 and A3. Later matcher features retain separate acceptance.

## Model extensibility plan

The target is native loading and inference for all official spaCy pretrained pipelines across languages, sizes, and architectures. Official releases must be identified by version and source provenance; this target does not promise automatic compatibility with unknown future release formats. Custom-trained pipeline loading is future scope. Its model data and additional native components should fit the same contracts, without a second loader or a requirement that every model originate in the official catalog. Executing arbitrary Python components and training models are separate capabilities, not prerequisites for this target.

The [v2 loader](../crates/spars/src/model.rs) checks explicit capability declarations independently of model identity. [Pipeline execution](../crates/spars/src/pipeline.rs) follows declared component order and validates dependencies; shared and NER encoders allow static vectors to be absent. The [installer](../crates/spars-model/src/recipe.rs) selects conversion recipes by typed catalog identity and applies per-release limits. The three English sizes have separate reference coverage. Remaining constraints include a manifest that still requires all six English component configurations, English language behavior, and the implemented CNN and greedy-transition architectures. Lexical `LIKE_NUM`, used by `Model::lexeme` and matcher flags, implements spaCy's English rule and takes only a number-word list from the model; other languages need a typed per-language rule before their models can load. These must be extended before other pipeline families can execute.

### Separate acquisition, configuration, and execution

The architecture separates three responsibilities: decode model assets, check that the runtime implements the declared capabilities, and construct an executable pipeline. Track release-specific reference evidence separately from those capability checks. A catalog entry supplies identity, source locations, checksums, resource provenance, and archive limits; it must not define inference behavior through model-name branches. A new official model using implemented capabilities should need model data, acquisition metadata, and reference evidence without a runtime source change.

Use typed, versioned configuration for component factories (constructors for pipeline stages) and neural architectures, including tensor references and operation-specific validation. Resolve component instances from their declared order and dependencies on shared encoders, which are networks that produce token features for multiple components. Do not assume every model has a tagger, parser, lemmatizer, and NER in the English medium sequence. Language-specific tokenization, lexical rules, lemmatization, and noun chunks need their own resources or native implementations. Missing capabilities must identify the component, architecture, language behavior, or format that is unsupported; never silently substitute English behavior or omit a requested component.

Keep registration of native component implementations separate from the model catalog. Future custom-trained pipelines built from supported components should reuse the same loading and execution contracts. Additional custom behavior will require a registered native implementation and its validation contract. This extension point does not require a Python fallback, loading separately compiled native plugins, or a training API now. Loaded models remain immutable and reusable, with state owned by each processing call.

### Implementation order and acceptance

Steps 2 and 4 are implemented for the English CNN family. Step 3 is partial because the manifest still requires all six English component configurations. Steps 1 and 5 have not started. This plan resumes after milestones 1–4 of the [implementation sequence](#implementation-sequence). In the meantime, component selection (P1) and senter conversion (S3) must extend the same typed component configuration rather than adding model-specific paths.

1. Inventory the components, architectures, language resources, and serialization requirements of version-pinned official pipelines. Group work by shared capabilities and record gaps in the compatibility inventory. Include transformer and language-specific families; the English sizes are initial acceptance cases, not the complete scope.
2. Separate typed model identity and acquisition metadata from reusable conversion and runtime configuration. Introduce the required versioned format changes with an explicit path for existing v1 assets. Preserve checksum, tensor, resource, and unsupported-configuration checks. Test two distinct model identities without duplicating conversion or inference logic.
3. Construct pipeline execution from typed component configurations and dependencies. Add independent cases for shared encoders, missing or reordered components, unavailable annotations, and invalid dependencies. Preserve the existing English pipeline's reference outputs and performance while removing its role as the universal pipeline shape.
4. Validate `en_core_web_sm`, `en_core_web_md`, and `en_core_web_lg` as initial real model cases, implementing their actual declared requirements, including models without static vectors. Each needs its own official reference outputs. Keep the existing frozen `md` suite; measure installation, loading, peak memory, and inference separately for each size.
5. Implement the remaining shared components, neural architectures, and language behavior identified by the official-model inventory, including transformer pipelines. Extend native conversion, reference tests, and consumer examples for each family. The all-official-pipelines target remains incomplete until every release in the declared version-pinned inventory passes its required checks.

Use the [quality process](QUALITY.md) for each bounded change. Synthetic configuration tests establish validation and extension mechanics, not compatibility with a real model. New weights and languages require their own reference evidence even when they reuse existing runtime code. The following installation requirements also apply to additional models.

## Versioned model installation plan

The first target is the official `en_core_web_md` 3.8.0 package. A native installer will download and verify official assets, convert them into the supported native format, and return an installed model location. Installation must work without Python, Node, WASM, or subprocess-based conversion. Loading and processing remain offline. No project-hosted model service or release attachment is required. Python remains available to maintainers for generating reference data and checking conversion.

The [native installer](MODEL_INSTALLATION.md) implements catalog-selected package conversion, downloads, verification, listing, and repeat-safe installation. Its model-dependent test compares every manifest field and all tensor bytes with the Python export. The versioned identities and catalog establish the boundary for updates; sm/md/lg 3.8.0 have native conversion comparisons; automatic update discovery and removal commands remain unimplemented. The milestones below retain the acceptance requirements for extending this work.

### 1. Define model identity and account for every resource

Add typed installation records that distinguish the upstream model name/version, official archive checksum, conversion recipe revision, linguistic-resource revision, and native format version. A conversion recipe is the set of rules used to turn official package data into native assets. Changing a recipe must not overwrite an existing installation of the same upstream model. Record the supported spaCy/Thinc versions, architectures, source URLs, notices, and expected archive sizes or limits. The initial compatibility catalog ships with the installer and lists explicitly supported releases; it does not automatically trust or install the latest upstream version.

Trace every field written by `tools/export.py` and `tools/lexical.py` to its source. Classify it as stored package data, derived configuration, or pinned language data that must ship separately with the installer. Small language tables may be generated during development and distributed with their licenses and checksums. Do not assume the model archive contains Python-generated Unicode tables, normalization defaults, or symbol definitions.

Acceptance: one reviewed mapping covers every current manifest field and tensor; typed records reject malformed digests, invalid versions, and unsupported combinations. Identify every required binary format from pinned upstream readers before choosing decoding dependencies. This is the first implementation increment.

### 2. Convert a local official package entirely in Rust

Read an already-downloaded, checksum-verified package without executing its contents. Implement only the stored formats needed by the pinned pipeline. Recover weights, configuration, ordered actions and labels, tokenizer rules, lookup tables, and vector mappings. Combine them with the pinned language resources to produce the existing native manifest and SafeTensors assets. Preserve applicable notices and source provenance. Reject unsupported architectures and malformed arrays before creating a usable installation.

Acceptance: compare tensor names, shapes, values, configuration, and linguistic resources against the Python exporter, then run the existing stage and full-pipeline parity suites. Require exact data equality where the representation is unchanged; differences in JSON layout or provenance fields need explicit, tested rules. Preserve existing floating-point tolerances. A separate native program converts the local package and runs inference with interpreters absent and network access disabled.

### 3. Add explicit downloads and reliable installation

Keep networking and archive handling outside normal inference and consumer builds, in an optional installer target or a separate tooling crate if needed. Download from the catalog's official sources with bounded retries, timeouts, and streaming checksum verification. Support a local archive for disconnected installation. Validate archive entries and extraction limits, including path traversal, duplicate destinations, links, and oversized contents.

Build in a temporary directory on the destination filesystem. Validate the complete output and its inventory before committing it with an atomic rename, which makes the complete model visible in one filesystem operation. Use locking or an equivalent mechanism so concurrent installs cannot corrupt each other. An interrupted or failed installation must leave existing models usable; rerunning the same installation must be safe.

Acceptance: controlled download tests cover truncation, bad checksums, unavailable sources, interruption, malformed archives, concurrent attempts, and repeat installation. A pinned official download is an explicit end-to-end check; repeatable failure tests use local fixtures rather than public-network failures.

### 4. Support explicit updates and rollback

Store supported releases side by side, keyed by complete installation identity. Applications select an exact installed version or path. Installing a newer release does not switch an application's model. Provide listing, verification, and explicit removal; rollback means selecting the retained prior installation. Do not delete older models automatically or mutate a loaded model. A new supported release may initially require an update to the installer or library to obtain its reviewed catalog entry and conversion recipe.

Legacy v1 loading pins the original model identity. V2 loading uses typed capability contracts, while the installer catalog and frozen reference suites retain release-specific evidence. Preserve capability and reference-version validation; broad version ranges are not evidence of compatibility. New weights on a supported architecture still require reference evaluation. Different architectures or resource semantics may require reusable Rust implementations and a new native format version.

Acceptance: lifecycle tests install two distinct identities, select each deterministically, reject unsupported combinations, and preserve the old installation through failed updates. Synthetic records can test lifecycle mechanics but cannot establish compatibility with a real second model. Declare a second upstream release supported only after its own pinned reference and parity suite pass.

### 5. Complete the consumer workflow and quality gates

Add a plain-English installation guide with commands and a separate Rust consumer example that actually run. Extend CI to execute native local-package conversion and offline inference without silently skipping model tests. Keep Python reference/export jobs separate from the native installation acceptance job. Record checksums and fixtures in Git; generated mismatch and performance reports remain under ignored `target/reports/`.

Acceptance: formatting, Clippy, typing, unit tests, malformed-input tests, reference comparisons, documentation examples, and package dry runs pass. Review installation security boundaries, source fidelity, test quality, and documentation independently. Measure download, conversion, disk usage, peak memory, and subsequent loading separately; check inference against the unchanged baseline if loader/runtime code changes. Public documentation must still distinguish this pinned installer from support for arbitrary spaCy packages.

Name-based loading and explicit downloads share `SPARS_MODEL_DIR` and operating-system cache defaults across Rust, Node, and the `spars` CLI. `crates/spars/tests/model_store.rs`, `crates/spars-model/tests/project.rs`, and `bindings/node/tests/models.test.mts` cover malformed selection records, path precedence, environment discovery, native installation, missing-model errors, and an installed npm package. Downloading is separate from loading, and path overrides do not persist project configuration.
