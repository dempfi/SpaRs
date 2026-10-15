# Use SpaRs from Node.js

The separate `spars-node` crate exposes native Rust inference to Node.js through Node-API. Load a model once, then process text or batches with a typed TypeScript API. The binding runs model loading and inference on Node's background worker threads. It uses the same Rust library and official model assets as Rust applications.

## Build and install a model

Use Node.js 24 or newer, npm, and Rust 1.88 or newer. Run these commands from the repository root:

```sh
npm --prefix bindings/node ci --ignore-scripts --no-audit --no-fund
cargo fetch --locked --manifest-path bindings/node/Cargo.toml
npm --prefix bindings/node run build
export SPARS_MODEL_DIR="./target/models"
node bindings/node/bin/spars.mjs download en_core_web_lg
```

Installation requires no Python. See [model installation](MODEL_INSTALLATION.md) for local archives and checksums. The Node package includes the `spars` CLI and an asynchronous `downloadModel` API backed by the native Rust installer. No separate installer executable is needed. `loadModel("en_core_web_lg")` resolves the installation under `SPARS_MODEL_DIR` and never downloads assets during loading or inference. Build the addon locally for your operating system and CPU. The [npm release workflow](RELEASING.md#publish-the-node-package) packages Linux x64/ARM64 glibc and musl, macOS x64/ARM64, and Windows x64/ARM64 builds and tests the resulting tarballs before optional publication. The workflow alone does not establish registry availability. Node dependencies are confined to `bindings/node/` and do not enter ordinary Rust consumer builds.

## Download and load by name

Set `SPARS_MODEL_DIR` in your shell, service, or application environment before downloading or loading. The library does not read `.env` files automatically or write project configuration. Without this variable, the shared default is `Library/Caches/spars/models` under the user home on macOS, `spars/models` under `LOCALAPPDATA` on Windows, and `spars/models` under `XDG_CACHE_HOME` (or `.cache` under the user home) on other systems. Relative values use the process working directory; an absolute path avoids differences between launch directories.

Once the Node package is installed locally, run `npx spars download en_core_web_lg`. From the source checkout, use `node bindings/node/bin/spars.mjs download en_core_web_lg`. Both accept `--path`, `--version`, and `--archive`. Omitting the version selects the catalog's single pinned release, never a remote latest release. `--path` affects that command only; set `SPARS_MODEL_DIR` or pass `{path: "models"}` to later `loadModel` calls to use the same custom directory.

The following executable example uses the official wheel acquired by the reference setup to run offline. Omit `archive` to download from the pinned official URL. Guide verification configures a temporary model store before running this example.

```typescript
import assert from 'node:assert/strict';
import { downloadModel, loadModel } from './index.js';

await downloadModel('en_core_web_sm', {
  archive: 'assets/en_core_web_sm-3.8.0-py3-none-any.whl',
});
const model = await loadModel('en_core_web_sm');
assert.equal((await model.process('Alice visits London.')).tokens[0]?.text, 'Alice');
```

`downloadModel(name, {path?, version?, archive?})` resolves to the installed directory. Existing installations are verified and reused. `loadModel(name, {path?})` loads by name; an explicit directory such as `./models/export` remains supported. Bare names select installed models even if a same-named directory exists in the working directory. Missing models reject with a download hint. Async calls capture environment and relative paths when called, before work is queued. Downloads use Node's worker pool and share the Rust installer's checksum verification, archive limits, staging, and locking; cancellation and progress callbacks are not implemented.

## Process text

Save this example as `bindings/node/analyze.mts`, then run `node bindings/node/analyze.mts` from the repository root. `SPARS_MODEL` optionally selects another model name or an explicit export directory. Reference verification supplies the medium-model export through this variable. Verification executes this code directly from this guide.

```typescript
import assert from 'node:assert/strict';
import { loadModel } from './index.js';
const model = await loadModel(process.env.SPARS_MODEL ?? 'en_core_web_lg');
const doc = await model.process('Alice works in London.');
assert.equal(doc.tokens[0]?.text, 'Alice');
assert.ok(doc.entities);
const places = doc.entities.filter(entity => entity.label === 'GPE').map(entity => {
  const first = doc.tokens[entity.start];
  const last = doc.tokens[entity.end - 1];
  assert.ok(first && last);
  return doc.text.slice(first.utf16Start, last.utf16End);
});
assert.deepEqual(places, ['London']);
console.log(places);
```

This prints `[ 'London' ]`. `GPE` labels a country, city, or similar political area. Entity, sentence, and noun-chunk spans use token indices: `start` is included and `end` is excluded. Tokens include text, trailing whitespace, normalization, grammatical tags, part-of-speech labels, morphology, base forms (lemmas), dependency heads and labels, sentence starts, and entity annotations. Returned objects own their data; editing them cannot change the model or later results.

Use `utf16Start` and `utf16End` with JavaScript's `slice`. `byteStart` and `byteEnd` count UTF-8 bytes; `codePointStart` and `codePointEnd` count Unicode code points. TypeScript treats these as distinct types to prevent accidental interchange. Offsets and token indices are checked unsigned 32-bit values. Text containing an unpaired UTF-16 surrogate is rejected because it cannot be preserved as valid Rust text.

## Types and autocomplete

The TypeScript declarations type every fixed vocabulary as a set of literal values, so editors offer them as completions and the compiler rejects typos. These include part-of-speech tags (`UniversalPos`), entity IOB tags (`EntityIob`: `B` begins an entity, `I` continues it, `O` is outside any entity), `PhraseMatcher` attributes (`PhraseAttribute`), token-condition attributes, predicate kinds and comparison operators (`TokenConstraint`), repetition kinds (`TokenRepetition`), DependencyMatcher relation operators such as `>` and `<<` (`DependencyRelation`), official model names (`OfficialModelName`) and error codes (`SparsErrorCode`). `TokenConstraint` also ties each attribute to the predicates it accepts: a lexical flag takes only `{ kind: 'flag', value }`, and `length` takes only numeric predicates. Token fields that are unavailable are `null`, while an empty string `''` is spaCy's unset or empty value; for example, tokens outside any entity have `entityType: ''`. Entity spans are `Span<EntityLabel>`, sentences `Span<'', ''>` and noun chunks `Span<'NP', ''>`; the second parameter types the span's `id`, which only entities can have.

Entity labels (`EntityLabel`), fine-grained tags (`FineGrainedTag`), dependency labels (`DependencyLabel`) and model names (`ModelName`) are open sets: they offer the labels of the supported official models for autocomplete, but also accept any other string, because custom models and edited documents can use other labels. Register your own labels by extending the matching interface (`EntityLabels`, `FineGrainedTags`, `DependencyLabels` or `ModelNames`); TypeScript's declaration merging adds the members of a redeclared interface to the original. In your own project, write the declaration in a file that your `tsconfig` includes and that has at least one `import` or `export`, and name the package: `declare module '@spars/node' { interface EntityLabels { PRODUCT_CODE: true } }`. Without an `import` or `export`, TypeScript treats the block as a replacement for the package's types rather than an extension. To accept only registered values in a set, declare `interface StrictLabelSets { entity: true }` the same way, using `entity`, `tag` or `dependency`; strict `tag` and `dependency` also narrow the values allowed in token conditions. Model names always accept any string, because `loadModel` also takes directory paths. Strict mode changes only compile-time types: `entityType`, span labels, `tag` and `dep` still hold whatever the loaded model or snapshot contains, so enable it only when your registered labels cover every model you load.

```typescript
import assert from 'node:assert/strict';
import { isSparsError, NativeDocument, PhraseMatcher } from './index.js';
import type { EntityLabel, TokenConstraint } from './index.js';

declare module './index.js' {
  interface EntityLabels { PRODUCT_CODE: true }
}

const label: EntityLabel = 'PRODUCT_CODE';
const number: TokenConstraint = { attribute: 'like_num', predicate: { kind: 'flag', value: true } };
assert.equal(new PhraseMatcher('LOWER').attribute, 'LOWER');
// @ts-expect-error 'LOWERR' is not a PhraseMatcher attribute
assert.throws(() => new PhraseMatcher('LOWERR'));
try {
  NativeDocument.fromSnapshot('{}');
  assert.fail('expected an error');
} catch (error) {
  assert.ok(isSparsError(error));
  console.log(label, number.attribute, error.code);
}
```

This prints `PRODUCT_CODE like_num SPARS_INVALID_MODEL`. `isSparsError(error)` narrows a caught value to `SparsError`, whose `code` is a `SparsErrorCode`, and `isOfficialModelName(name)` narrows a string to `OfficialModelName` before calling `downloadModel`. A POS condition such as `{ attribute: 'pos', predicate: { kind: 'equals', value: 'noun' } }` is a compile error: spaCy and SpaRs accept it at runtime, but POS values are compared exactly and every POS tag is uppercase (`NOUN`), so `'noun'` never equals a token's POS. In `in` it matches nothing, and in `not_in` it excludes nothing. These types add no runtime checks: JavaScript callers, and values cast past the types, still receive the same native errors. A matcher stores what it was given, so `get()` returns such a value unchanged even though its declared return type excludes it.

## Batches, stages, and vectors

This example can also be saved beside `index.js` and run from the repository root:

```typescript
import assert from 'node:assert/strict';
import { loadModel } from './index.js';

const model = await loadModel(process.env.SPARS_MODEL ?? 'en_core_web_lg');
assert.equal(model.maxLength, 1_000_000);
assert.equal(model.batchSize, 256);
model.maxLength = 100_000;
model.batchSize = 64;
const streamed: string[] = [];
for await (const doc of model.pipe(['😀 café', ''], { batchSize: 2, stage: 'Tokenizer' })) {
  streamed.push(doc.text);
}
assert.deepEqual(streamed, ['😀 café', '']);
const docs = await model.processBatch(streamed, 'Tokenizer');
assert.equal(docs[0]?.tokens[0]?.utf16End, 2);
assert.equal(docs[0]?.tokens[0]?.byteEnd, 4);
assert.equal(docs[0]?.entities, null);
assert.deepEqual((await model.process('')).entities, []);
const vector = model.vector('dog');
assert.ok(vector instanceof Float32Array);
assert.equal(vector.length, 300);
assert.equal(model.vector('spars_unknown_🙂_lexeme'), null);
```

`process(text, stage?)` and `processBatch(texts, stage?)` return promises. The default runs the full pipeline. A stage selects an ordered prefix: `Tokenizer`, `Tagger`, `Parser`, `AttributeRuler`, `Lemmatizer`, or `Ner`. Later annotations remain `null`; computed empty lists remain `[]`. The early-stage unavailable-value contract follows SpaRs and does not reproduce every default value in a partially processed Python Doc.

`model.maxLength` defaults to 1,000,000 Unicode code points, following spaCy 3.8.14's `Language.max_length`. An emoji such as `😀` counts as one code point; `e` followed by a combining accent counts as two. Equality is allowed. The native worker checks the limit before tokenization; exceeding it rejects with `SPARS_TEXT_TOO_LONG` and an `[E088]` message. Set `model.maxLength` to a nonnegative unsigned 32-bit integer; zero permits only empty text. This Node setting does not change Rust's direct `Model::process` API. Invalid UTF-16 remains unsupported.

`model.batchSize` reads the exported `[nlp] batch_size`: 256 for the supported official sm/md/lg releases. When that setting is absent, it uses spaCy's `Language` constructor default of 1,000. Set a positive unsigned 32-bit integer to override it per model. This is a processing chunk size, not a maximum number of input documents. Both model settings throw for invalid values or changes while any inference is pending in this package instance and JavaScript isolate. This stricter validation and idle-only mutation are Node contracts; Python permits direct attribute assignment.

`processBatch` snapshots the entire input array, processes consecutive chunks of at most `model.batchSize`, and resolves to all documents in input order. Each chunk uses a background task that performs sequential document inference. A failure rejects the whole call. The call retains a single admission slot across chunks, including result creation, and returns that slot on success or failure. All input strings and completed results remain in memory until the call finishes. Concurrent calls share immutable model weights and own their processing state.

`model.pipe(texts, {batchSize?, stage?})` returns an async generator over a synchronous or asynchronous iterable of strings. No input is consumed until iteration starts. It resolves the buffering size from its option or `model.batchSize` on the first iteration, processes one buffer, yields documents in order, then reads the next buffer. The final buffer may be smaller. Breaking iteration closes the source; a source or processing failure rejects iteration and stops it. Earlier buffers may already have been yielded. The initial buffering size stays fixed for that iterator; changes to model settings while idle apply when the next buffer is submitted. Avoid retaining yielded documents if bounded memory is required.

These buffers are not spaCy's component-level neural batching, and there is no `n_process` multiprocessing, `as_tuples`, per-component pipe configuration, or cancellation support. `pipe` accepts only `batchSize` and `stage` options; unknown options reject during iteration. Buffer failure timing need not match spaCy for every pipeline, especially tokenizer-only processing. Both input copying and result creation run on the JavaScript thread, so large documents or buffers can still delay the event loop. The default maximum length follows spaCy; it does not promise that all accepted text fits in memory or avoids native regex/inference limits.

## Upgrade from npm 0.2.0

The published `@spars/node@0.2.0` package has no package-level admission queue, configurable input caps, `maxLength`, or `pipe` wrapper. Its npm tarball is the upgrade baseline; the later source-tree defaults are not the defaults that npm consumers received. The published package can be inspected without running lifecycle scripts with `npm pack @spars/node@0.2.0 --ignore-scripts`. The changes below apply to an upgrade that includes the processing API described in this guide.

| Affected usage | Change and observable impact | Migration |
| --- | --- | --- |
| More than 34 unfinished inference calls across models in one package instance and JavaScript isolate | Two calls can be active and 32 wait; further calls reject with `SPARS_BUSY`. A synchronous burst of 35 calls previously reached the native pool without package-level rejection. | Bound submissions, consume `pipe` incrementally, or use `processBatch` when retaining all results is acceptable. Configure larger finite `maxActive`/`maxQueued` values during startup only when the service has capacity. |
| A service relying on more than two concurrent native inference tasks | Admission now limits active jobs to two, regardless of a larger libuv pool. Requests can wait longer and application deadlines may expire. | Measure the workload and tune `configureExecution`, keeping application backpressure. Increasing `UV_THREADPOOL_SIZE` alone does not change package admission. |
| Any document longer than 1,000,000 Unicode code points | The worker rejects with the new `SPARS_TEXT_TOO_LONG` code before tokenization, including within `processBatch`. The released package had no separate document-length policy. | Set `model.maxLength` to an intentional larger bound on each loaded model while inference is idle, or split input when losing cross-boundary NLP context is acceptable. Handle the new error; raising the limit does not guarantee enough memory or successful inference. |
| JavaScript callers matching native argument-error codes or messages | Wrapper validation of invalid `process`/`processBatch` arguments and unknown stages throws `TypeError`. Examples that previously reported native `InvalidArg` or `StringExpected` no longer carry those codes; a full queue may reject a batch with `SPARS_BUSY` before checking its elements. | Validate inputs and handle `TypeError` for argument mistakes. Catch documented `SPARS_*` processing errors separately; do not rely on native conversion messages. |
| Hand-edited or third-party model manifests with previously ignored `config` data | The loader now reads `[nlp] batch_size`; a null/non-string config, malformed or duplicate processing fields, or interpolation can reject loading with `SPARS_INVALID_MODEL`. This loader change also affects direct Rust consumers. | Use an unmodified official export or regenerate compatible model assets. For deliberately maintained manifests, supply the actual literal positive integer batch size in the preserved config; see [model format](MODEL_FORMAT.md). Normal supported official exports need no re-export. |
| Deployments with `NAPI_RS_NATIVE_LIBRARY_PATH` pointing to an older addon, or manually mixed wrapper and native versions | Package import throws `SPARS_NATIVE_INCOMPATIBLE` when required native settings accessors are absent. This check prevents silently returning empty batches with an older addon. | Remove the override to use the matching packaged addon, or update it to a compatible build. Reinstall the root and platform packages together; do not combine an old binary with a new wrapper. |

A rejected `Promise.all` does not cancel calls already admitted. Do not retry the entire original collection blindly: account for completed and still-running work. The limit is shared across models, so creating another model in the same isolate does not create a separate queue. `configureExecution(null)` restores two active jobs and 32 queued; it does not restore the published package's unbounded submission behavior.

For a collection that previously used `Promise.all(texts.map(text => model.process(text)))`, this example submits one buffer at a time and verifies that all 100 documents are consumed. It assumes no competing producers saturate the shared queue. Configure settings during startup before starting other inference:

```typescript
import assert from 'node:assert/strict';
import { loadModel } from './index.js';

const model = await loadModel(process.env.SPARS_MODEL ?? 'en_core_web_lg');
// Only raise this if the application intentionally accepts larger documents.
model.maxLength = 2_000_000;
const texts = Array.from({ length: 100 }, () => 'Alice visits London.');
let completed = 0;
for await (const doc of model.pipe(texts, { batchSize: 64 })) {
  assert.equal(doc.text, 'Alice visits London.');
  completed++;
}
assert.equal(completed, texts.length);
```

Valid `process`, `processBatch`, loading, vector lookup, result shapes, and within-batch ordering retain their APIs. Small sequential documents need no call-site rewrite. Batch chunking changes scheduling and result-conversion timing, not the ordered result contract: `processBatch` still collects all results and rejects the whole call on failure. `pipe` is optional; choosing it introduces incremental delivery, so earlier buffers may already have been consumed when a later error occurs.

The former 32,768-unit text, 128-document batch, and 65,536-unit aggregate caps existed in later unreleased source builds, not npm 0.2.0. Users of those builds who relied on the caps should enable `configureInputLimits` as shown below. Existing explicit input policies retain their UTF-16 units and values; a policy allowing text over 1,000,000 code points now also needs an appropriate per-model `maxLength`.

## Upgrade TypeScript code to the literal types

The declarations now use literal types for fixed vocabularies, as described in [types and autocomplete](#types-and-autocomplete). JavaScript behavior is unchanged except for snapshot validation, but TypeScript code that passed plain strings can stop compiling:

| Affected usage | Change and observable impact | Migration |
| --- | --- | --- |
| Passing a `string` variable to `downloadModel`, `new PhraseMatcher(...)`, a token condition's `attribute`, `kind` or `operator`, a repetition `kind` or a dependency `relation` | These parameters take literal types, so a value typed as `string` is a compile error. | Narrow external input first: use `isOfficialModelName` for model names, or check the value against your own list of the literal values. |
| Code importing the generated `TokenPredicate` interface or building conditions as `{ attribute: string; predicate: TokenPredicate }` | `TokenPredicate` is removed; `TokenConstraint` is a union whose predicate shape depends on the attribute. | Write conditions as `TokenConstraint` values, or use `Extract<TokenConstraint, { attribute: 'length' }>['predicate']` for one attribute's predicates. |
| Fields typed as `string` in your own code, such as `token.pos` or `span.label` | Output fields have literal or open label types. Assigning them to `string` still works, but comparing a closed field with an impossible value, such as `token.pos === 'noun'`, is a compile error. | Compare with the declared values; register custom labels as described above. |
| Hand-edited or third-party snapshots with an IOB tag outside `B`, `I`, `O` and empty, a POS tag outside the universal tags, an unlabeled entity, a labeled sentence, or a noun chunk not labeled `NP` | `NativeDocument.fromSnapshot` and Rust `Doc::from_json` now reject them with `SPARS_INVALID_MODEL` (`Error::Model` in Rust). Snapshots written by SpaRs are unaffected. | Correct the values, or regenerate the snapshot by processing the text again. |
| Code comparing whole token or span objects, or building `Span` objects | Every token now has `entityId` and every span has `id`, `''` when absent; sentences are `Span<'', ''>` and noun chunks `Span<'NP', ''>`. | Include the new fields in expected objects, or compare the fields you need. |

## Server resource limits

Inference admission defaults to two active jobs and 32 queued jobs per package instance and JavaScript isolate. Additional input limits are disabled by default. Users of unreleased source builds relying on earlier text and batch caps must configure `configureInputLimits` explicitly at startup. Published npm 0.2.0 consumers should follow the upgrade guidance above for the new queue protection. Services can customize either policy while inference is idle. These are Node server policies, not spaCy processing defaults:

```typescript
import assert from 'node:assert/strict';
import { configureExecution, configureInputLimits, loadModel } from './index.js';

const model = await loadModel(process.env.SPARS_MODEL ?? 'en_core_web_lg');
configureExecution({ maxActive: 2, maxQueued: 32 });
configureInputLimits({ maxTextLength: 32_768, maxBatchSize: 128, maxBatchTextLength: 65_536 });
// Keep pipe buffers within the explicit maxBatchSize policy.
model.batchSize = 128;
assert.equal((await model.process('Hello.')).text, 'Hello.');
// Demonstration only: restore default admission and remove additional input caps.
// Omit these resets in a server that should retain its custom policies.
configureExecution(null);
configureInputLimits(null);
```

Import `configureExecution` and `configureInputLimits` from the package entry point. `configureExecution({maxActive, maxQueued})` sets a maximum number of submitted inference jobs and a first-in, first-out waiting queue. `maxActive` must be a positive safe integer; `maxQueued` must be a nonnegative safe integer, and zero disables waiting. Excess calls reject with `SPARS_BUSY` without submitting native work. Calls rejected with `SPARS_BUSY` need application backpressure, such as rejecting an HTTP request or retrying later with a bounded policy. `configureExecution(null)` restores two active jobs and 32 queued jobs while idle; it does not cancel work or change the native pool's thread count.

`configureInputLimits({maxTextLength, maxBatchSize, maxBatchTextLength})` sets all three additional input limits. All must be positive safe integers. Text lengths count JavaScript UTF-16 units (`string.length`), so `😀` counts as two units for these server policies. `maxBatchSize` limits the whole array passed to `processBatch` or one `pipe` buffer, before any internal chunking. Empty text and empty batches remain supported. `configureInputLimits(null)` removes these extra limits while idle; the per-model `maxLength` still applies.

Both configuration functions copy their settings and throw synchronously for invalid values or changes while inference is pending. Policies are shared by models loaded through this package instance within one JavaScript isolate (the separate JavaScript environment of the main thread or a Node worker), not across Node workers or processes. They cover inference and matcher searches; model loading and downloads remain outside these policies. Load models and download assets during startup. Node's libuv worker pool is shared with other Node operations; these policies do not reserve threads or guarantee latency. Import the package entry point; importing the native `.node` file directly bypasses the JavaScript policies and streaming wrapper.

Oversized inputs reject with `SPARS_INPUT_LIMIT` before copying into Rust or submitting inference. Batch count is checked before accessing elements; text and aggregate length are checked while capturing the batch, and the first violation rejects the entire call. A `pipe` buffer is checked when submitted. Size rejection does not consume an inference slot. A full queue can return `SPARS_BUSY` before inspecting batch elements. These policies do not limit model loading, downloads, `vector` lookups, other isolates, or memory already allocated by the caller. Large allowed results still require synchronous JavaScript object creation. Increasing limits increases potential CPU, memory, and event-loop cost.

`vector(word)` is a synchronous, case-sensitive static-vector lookup. It returns a copied `Float32Array`, or `null` when the lexical key has no row. Modifying the array is safe. Native documents support the additional vector, traversal, and matcher APIs below.

## Handle processing errors

Processing failures reach a caller in two ways. Invalid configuration values and invalid JavaScript arguments throw synchronously when the function is called, before any promise exists: `configureExecution` and `configureInputLimits` throw `RangeError` or `TypeError` for invalid values, and a plain `Error` without a `code` when called while inference is pending, so configure them during startup. `process` and `processBatch` throw `TypeError` for non-string text or an unknown stage when the call is admitted (see below). Admission and processing failures reject the returned promise with an `Error` whose `code` is one of the `SPARS_*` values listed under [errors and verification](#errors-and-verification). The example below does not depend on a web framework and exercises each path deterministically:

```typescript
import assert from 'node:assert/strict';
import { configureExecution, configureInputLimits, loadModel } from './index.js';
import type { Document, InputLimits, Model } from './index.js';

// Caught values are unknown
function errorCode(error: unknown): string | undefined {
  return error instanceof Error && 'code' in error && typeof error.code === 'string' ? error.code : undefined;
}

async function processWithRetry(model: Model, text: string, attempts = 3): Promise<Document> {
  for (let attempt = 1; ; attempt++) {
    try {
      return await model.process(text);
    } catch (error) {
      if (errorCode(error) !== 'SPARS_BUSY' || attempt >= attempts) throw error;
      await new Promise(resolve => setTimeout(resolve, 50 * attempt));
    }
  }
}

function planBatches(texts: readonly string[], limits: InputLimits): string[][] {
  const batches: string[][] = [];
  let current: string[] = [];
  let length = 0;
  for (const text of texts) {
    if (text.length > Math.min(limits.maxTextLength, limits.maxBatchTextLength)) throw new RangeError('Document exceeds the input policy');
    if (current.length > 0 && (current.length === limits.maxBatchSize || length + text.length > limits.maxBatchTextLength)) {
      batches.push(current);
      current = [];
      length = 0;
    }
    current.push(text);
    length += text.length;
  }
  if (current.length > 0) batches.push(current);
  return batches;
}

const model = await loadModel(process.env.SPARS_MODEL ?? 'en_core_web_lg');

assert.throws(() => configureExecution({ maxActive: 0, maxQueued: 0 }), RangeError);
assert.throws(() => model.processBatch(['ok', 42 as unknown as string]), TypeError);

configureExecution({ maxActive: 1, maxQueued: 0 });
const running = model.process('Alice visits London.', 'Tokenizer');
await assert.rejects(model.process('Overflow.'), { code: 'SPARS_BUSY' });
await assert.rejects(model.processBatch(['ok', 42 as unknown as string]), { code: 'SPARS_BUSY' });
await running;
configureExecution(null);
assert.equal((await processWithRetry(model, 'Recovered.')).text, 'Recovered.');

// Oversize rejection the big batch
const limits: InputLimits = { maxTextLength: 20, maxBatchSize: 2, maxBatchTextLength: 30 };
configureInputLimits(limits);
await assert.rejects(model.processBatch(['Short.', 'x'.repeat(21)]), { code: 'SPARS_INPUT_LIMIT' });
assert.throws(() => planBatches(['x'.repeat(21)], limits), RangeError);
const texts = ['Dogs bark.', 'Cats sleep.', 'Birds fly.'];
const docs: Document[] = [];
for (const batch of planBatches(texts, limits)) docs.push(...await model.processBatch(batch));
assert.deepEqual(docs.map(doc => doc.text), texts);
configureInputLimits(null);

await assert.rejects(processWithRetry(model, '\ud800'), { code: 'SPARS_INVALID_TEXT' });
assert.equal((await model.process('Still works.')).text, 'Still works.');
```

For `process`, `processBatch`, `pipe`, and `findMatches`, `SPARS_BUSY` means every active slot is in use and the waiting queue is full; it does not indicate a problem with the input. (Matcher `add` and `remove` also throw `SPARS_BUSY` synchronously while a search on that matcher is pending; see [token matching](#token-matching).) Retrying immediately and without a limit adds calls to a queue that is already full and prolongs the overload. Use a bounded policy instead: a few delayed attempts, as `processWithRetry` shows, or reject the request (for example with HTTP 503) and let the client retry later. The same call can fail differently depending on load. While the queue has room, `processBatch(['ok', 42])` throws `TypeError`; while the queue is full, it rejects with `SPARS_BUSY`, because admission is checked before batch elements are inspected. A retried call that is admitted can therefore still throw `TypeError`, so validate arguments before retrying.

`SPARS_INPUT_LIMIT` means an input exceeded the policy set with `configureInputLimits`. A batch is rejected as a whole when its element count, total length, or any single element exceeds the policy; no documents from that call are returned. Either reject the input, or group separate, independent documents into smaller batches, as `planBatches` does. Do not cut one document into arbitrary pieces to fit a limit: each piece is processed without the surrounding text, so tokens, sentences, entities, and dependencies near a cut can change, and offsets start again at zero in each piece. The per-model `model.maxLength` limit is separate: it counts Unicode code points and rejects with `SPARS_TEXT_TOO_LONG` after admission, also failing the whole batch. With `pipe`, a rejection stops iteration after earlier buffers may already have been yielded, so account for consumed documents instead of restarting the source.

Handle only the codes that the application's policy understands, and rethrow the rest, as `processWithRetry` does with `SPARS_INVALID_TEXT`. A rejected call does not leave the package unusable: later valid calls succeed after busy, input-limit, and processing rejections. The defaults remain two active jobs, 32 queued jobs, and no additional input caps. Both policies apply per package instance and JavaScript isolate. They bound admission and input size, not the JavaScript-thread cost of converting large accepted results into objects.

## Retain and restore native documents

`model.processDocument(text, stage?)` runs the same pipeline as `process` and retains an immutable native document. It shares the inference queue, optional text input limit, and per-model `maxLength`. Existing `process` and `processBatch` results remain plain JavaScript objects. Call `toObject()` when you need that output shape; each call returns an independent copy.

```typescript
import assert from 'node:assert/strict';
import { loadModel, NativeDocument } from './index.js';

const model = await loadModel(process.env.SPARS_MODEL ?? 'en_core_web_lg');
const doc = await model.processDocument('Alice visits London.');
const restored = NativeDocument.fromSnapshot(doc.toSnapshot());
assert.deepEqual(restored.toObject(), doc.toObject());
const output = restored.toObject();
output.tokens.length = 0;
assert.ok(restored.toObject().tokens.length > 0);
```

`toSnapshot()` preserves native annotations, contextual vectors and entity IDs when available. A snapshot with entity IDs uses format version 3, which older SpaRs versions reject instead of dropping the IDs. Entity IDs must be omitted rather than empty, may appear only on tokens and entities, must match each entity's first token, and require format version 3; otherwise import throws `SPARS_INVALID_MODEL`, or `SPARS_UNSUPPORTED` for IDs in an earlier version. `NativeDocument.fromSnapshot(json)` validates the complete snapshot, including text boundaries, annotation indices, tensor shapes and closed values: entity IOB tags must be `B`, `I`, `O` or empty, POS tags must be spaCy universal POS tags or empty, entities need a label, sentences have none and noun chunks are `NP`. Malformed JSON, a missing field, an invalid tensor or a value outside these closed sets throws `SPARS_INVALID_MODEL`; offsets, token indices or spans that do not fit the text throw `SPARS_BOUNDS`; an unknown format version throws `SPARS_UNSUPPORTED`; and input with an unpaired UTF-16 surrogate throws `SPARS_INVALID_TEXT`. Import does not check that entity spans agree with the tokens' IOB tags and entity types. This versioned native format is separate from spaCy JSON and DocBin. Native documents remain usable after their originating model is released. Snapshot import/export and plain-object conversion run synchronously on the JavaScript thread and allocate memory proportional to the document, including any contextual tensor; inference admission limits do not bound these operations.

### Native memory and garbage collection

A native document's text, tokens, spans and contextual tensor live outside the JavaScript heap. Each `NativeDocument` reports an estimate of that memory to V8, Node's JavaScript engine and garbage collector, when it is created, and withdraws it when the object is collected, so V8 can collect unused documents sooner when many large ones are created. `reportedNativeMemory()` returns the total external memory reported through Node-API to the current thread's V8 isolate (one independent JavaScript heap; each worker thread has its own), in bytes. The total also includes memory reported by other native code, so compare values taken before and after an operation rather than reading it as SpaRs' own use. On Node 24.17, the version tested, `process.memoryUsage().external` does not include it. This example builds a document from a small snapshot, so it needs only the built addon, not a model.

```typescript
import assert from 'node:assert/strict';
import { NativeDocument, reportedNativeMemory } from './index.js';

const before = reportedNativeMemory();
const doc = NativeDocument.fromSnapshot(JSON.stringify({ format_version: 1, document: { text: 'Hello world', tokens: [
  { start: 0, end: 5, idx: 0, whitespace: true, norm: 'hello' },
  { start: 6, end: 11, idx: 6, whitespace: false, norm: 'world' },
] } }));
assert.ok(reportedNativeMemory() > before);
assert.equal(doc.utf16Length, 11);
```

`utf16Length` is the text length in UTF-16 code units, the same as JavaScript's `string.length`. For documents from `processDocument` and `NativeDocument.fromSnapshot`, the estimate counts allocated capacity and is a lower bound: it leaves out the allocator's per-allocation overhead and the dependency index that the first tree traversal builds and caches. A document from `withEntities` reports its source's estimate without recounting; the copy usually owns somewhat less, because copying drops unused capacity, but slightly more when the update adds entity tags to a document that had none. Token and span views share their document's memory without reporting it, so a view that outlives its document object keeps unreported memory alive until the view is collected. Node frees a document's native memory in a finalizer, a cleanup callback that runs on a later event-loop turn after garbage collection finds the object unused, so a synchronous loop that creates many documents without yielding cannot free any of them until it ends. When creating many large documents in one task, yield between documents (or every few documents), for example with `await setImmediate()` from `node:timers/promises`. `bindings/node/tests/memory.test.mts` checks that reports are made, released on collection, reused by copies and skipped for rejected updates; `npm --prefix bindings/node test` runs it.

## Lexical attributes

`model.lexeme(text)` returns an owned record of the existing native lexical attributes, including normalization, shape, prefix/suffix, lexical flags, and static-vector availability. It runs synchronously and does not change the vocabulary.

```typescript
import assert from 'node:assert/strict';
import { loadModel } from './index.js';

const model = await loadModel(process.env.SPARS_MODEL ?? 'en_core_web_lg');
const lexeme = model.lexeme('London');
assert.equal(lexeme.isAlpha, true);
assert.equal(lexeme.suffix, 'don');
assert.equal(typeof lexeme.orth, 'bigint');
```

`orth` is a JavaScript `bigint` so the unsigned 64-bit string identity stays exact. Convert it explicitly to a decimal string when serializing a lexical record to JSON; JavaScript's default JSON serializer rejects bigints.

## Model-store management

`ModelStore` exposes offline selection of installed model directories. Construction and `discover()` capture the directory at call time; `resolve` and `register` perform filesystem work asynchronously. Registration selects an existing direct child directory and does not inspect model weights. Loading by name still validates the selected model identity and contents.

```typescript
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { ModelStore } from './index.js';

const directory = mkdtempSync(join(tmpdir(), 'spars-store-'));
try {
  const store = new ModelStore(directory);
  const installed = join(directory, 'example-installation');
  mkdirSync(installed);
  await store.register('en_core_web_sm', installed);
  assert.equal(await store.resolve('en_core_web_sm'), realpathSync(installed));
} finally { rmSync(directory, { recursive: true, force: true }); }
```

Use `ModelStore.discover()` for `SPARS_MODEL_DIR` or the platform cache default. `store.root` supplies the `{path}` option for `loadModel(name, {path: store.root})`. Store operations neither acquire models nor implement update/removal commands.

## Token and span views

Native documents expose checked token and span views that retain the document. Indices must be nonnegative integers within the document; spans use an exclusive end. `doc.tokens()` and `span.tokens()` return arrays of token views. Traversal returns arrays in the same order as the Rust API; unavailable dependency annotations raise an error, while unavailable sentence lists return `null`.

```typescript
import assert from 'node:assert/strict';
import { loadModel } from './index.js';

const model = await loadModel(process.env.SPARS_MODEL ?? 'en_core_web_lg');
const doc = await model.processDocument('Alice runs.');
const alice = doc.token(0);
assert.equal(alice.text, 'Alice');
assert.equal(alice.head()?.text, 'runs');
assert.deepEqual(alice.ancestors().map(token => token.text), ['runs']);
assert.equal(alice.sentence().text, 'Alice runs.');
assert.equal(doc.span(0, 2).text, 'Alice runs');
assert.equal(doc.sentenceViews()?.length, 1);
```

`token.annotations()` returns a copied record of the native annotations and byte/code-point start offsets. `token.children()`, `ancestors()`, `subtree()`, `head()`, `span()`, and `sentence()` provide graph and sentence access. Use `doc.toObject()` for the full JavaScript output with UTF-16 offsets. Views are immutable; retaining even one view retains its full document and contextual tensor. Traversal and view creation run synchronously and allocate in proportion to the returned results.

## Edit entity annotation

`doc.withEntities(update)` replaces entity annotation like spaCy's `Doc.set_ents` and returns a new native document. The original document and every token and span view of it keep their earlier annotation, so code holding them is unaffected. Intervals use token indices with an exclusive end. `entities` lists labeled intervals, each with an optional `id`; `outside`, `blocked` and `missing` list unlabeled intervals for tokens outside any entity, tokens that can never be part of one, and tokens whose annotation is unknown. `default` sets every other token: `'outside'` (the default), `'missing'`, `'blocked'` or `'unmodified'`. An entity's `id` is written to its tokens, which report it as `entityId`, and each entity in `toObject().entities` reports its first token's ID as `id`. In output, `''` means no ID; in an update, an `id` of `''` has the same effect as leaving it out. As in spaCy, an entity without an `id` keeps the IDs its tokens already have, and tokens that stop being part of an entity keep theirs; the [entity editing guide](ENTITIES.md#entity-ids) explains these rules.

```typescript
import assert from 'node:assert/strict';
import { loadModel } from './index.js';

const model = await loadModel(process.env.SPARS_MODEL ?? 'en_core_web_lg');
const doc = await model.processDocument('Tim Cook visited London.');
const edited = doc.withEntities({ entities: [{ start: 3, end: 4, label: 'CITY' }], default: 'unmodified' });
assert.deepEqual(edited.toObject().entities?.map(entity => entity.label), ['PERSON', 'CITY']);
const named = doc.withEntities({ entities: [{ start: 0, end: 2, label: 'PERSON', id: 'tim-cook' }], default: 'unmodified' });
assert.equal(named.token(1).annotations().entityId, 'tim-cook');
assert.equal(named.toObject().entities?.[0]?.id, 'tim-cook');
assert.equal(doc.token(3).annotations().entityType, 'GPE');
assert.throws(() => doc.withEntities({ entities: [{ start: 3, end: 9, label: 'CITY' }] }), { code: 'SPARS_BOUNDS' });
```

The original `doc` still reports London as `GPE`. An interval outside the document, a reversed interval, a non-integer or negative index, or two intervals sharing a token (entities with an empty label are ignored) throws `SPARS_BOUNDS`. A label with an unpaired surrogate throws `SPARS_INVALID_TEXT`, and an `I` tag on the first token that the update leaves in place, which only an edited snapshot can contain, throws `SPARS_UNSUPPORTED`. An object of the wrong shape, an unknown `default`, or an undeclared property such as a misspelled `ents` or an unsupported `kbId` throws a native argument error (`InvalidArg`, `NumberExpected` or `StringExpected`), so a typo cannot silently become the default update. The update runs synchronously on the JavaScript thread, outside inference admission and input limits. It is checked before anything is copied, so an invalid update throws without copying; a valid one copies the whole document, including any contextual tensor. Token states, the repair of inside tags and the differences from spaCy are described in the [entity editing guide](ENTITIES.md).

## Vectors and similarity

```typescript
import assert from 'node:assert/strict';
import { loadModel } from './index.js';

const model = await loadModel(process.env.SPARS_MODEL ?? 'en_core_web_lg');
const doc = await model.processDocument('dog cat');
assert.ok(model.documentVector(doc) instanceof Float32Array);
assert.ok(model.tokenVector(doc, 0) instanceof Float32Array);
assert.ok(model.spanVector(doc, 0, 1) instanceof Float32Array);
assert.equal(model.similarity(doc, doc), 1);
assert.equal(model.spanSimilarity(doc, 0, 1, doc, 0, 1), 1);
```

The receiving model selects vector behavior: md/lg use their static vocabulary, while sm uses contextual rows retained in the supplied document. Documents do not record model identity; when comparing contextual documents, callers should use the same model and processing stage. Token vectors return `null` when unavailable; unavailable document/span vectors are empty arrays. All returned vectors are independent copies. Similarity preserves identical-token and zero-vector shortcuts, then rejects incompatible nonzero vector dimensions with `SPARS_UNSUPPORTED`; this check is stricter than the current Rust similarity helpers, which truncate unequal dimensions during their dot product.

These methods run synchronously. Mean vectors and similarities scan the selected tokens and vector dimensions; they do not run inference. Large repeated comparisons can delay the JavaScript thread.

## Token matching

`TokenMatcher` compiles the existing native token patterns and returns ordered, overlapping matches over a `NativeDocument`. Patterns use the typed SpaRs schema, not spaCy's Python dictionary syntax. Registration validates the complete addition before changing rules; unsupported fields and predicate/operator values reject.

```typescript
import assert from 'node:assert/strict';
import { loadModel, TokenMatcher } from './index.js';

const model = await loadModel(process.env.SPARS_MODEL ?? 'en_core_web_lg');
const doc = await model.processDocument('Alice visits London.', 'Tokenizer');
const matcher = new TokenMatcher();
matcher.add('place', [{ tokens: [{
  constraints: [{ attribute: 'text', predicate: { kind: 'equals', value: 'London' } }],
  repetition: { kind: 'once' },
}] }]);
assert.deepEqual(await matcher.findMatches(doc), [{ rule: 'place', start: 2, end: 3 }]);
```

Supported token attributes are `text`, `lower`, `norm`, `lemma`, `pos`, `tag`, `dep`, `morphology`, the lexical flags `is_alpha`, `is_digit`, `is_space`, `is_punct`, `like_num`, `is_lower`, `is_upper`, `is_title`, `is_ascii`, `is_currency`, `is_stop`, `is_bracket`, `is_quote`, `is_left_punct`, `is_right_punct`, `like_url` and `like_email`, and `length`. `lower` compares the token's pinned Unicode 15.0.0 lowercase text, as in spaCy's `LOWER`, and needs no model annotations; pattern values are compared as written, so write them in lowercase. Predicates are `equals` with a string `value`, `flag` with a boolean `value`, `compare` with an `operator` (`==`, `!=`, `>=`, `<=`, `>` or `<`) and a finite number `value`, `in`, `not_in`, `is_subset`, `is_superset`, `intersects`, `morph_superset`, and `morph_intersects` with string `values`, or `in_integers`, `not_in_integers`, `is_subset_integers`, `is_superset_integers` and `intersects_integers` with integer `values`. `is_subset`, `is_superset` and `intersects` follow spaCy's set operators on any string attribute (see [Compare with a set of values](TOKEN_MATCHER.md#compare-with-a-set-of-values)); `morph_superset` and `morph_intersects` require morphology; `flag` is the only predicate for lexical flags; `length` accepts only `compare` and the integer predicates and counts Unicode code points. Integer `values` must be safe JavaScript integers (whole numbers up to 2^53−1 in magnitude); other numbers are rejected rather than rounded. A `compare` value may be any finite number, including a fraction such as `2.5`; NaN and infinities are rejected. For example, `{ attribute: 'length', predicate: { kind: 'compare', operator: '>=', value: 7 } }` matches tokens of at least seven code points. Repetition kinds are `once`, `optional`, `zero_or_more`, `one_or_more`, `negated`, and `range`; a range requires `min` and accepts an optional inclusive `max`. Missing required document annotations fail explicitly. See the [token matcher guide](TOKEN_MATCHER.md) for native semantics and unsupported options.

Lexical flag conditions use the model's language rules, so create the matcher with a model: `new TokenMatcher(model)` or `new DependencyMatcher(model)`. A matcher created without a model rejects flag conditions with `SPARS_INVALID_PATTERN`. This example finds a number followed by a word:

```typescript
import assert from 'node:assert/strict';
import { loadModel, TokenMatcher } from './index.js';
import type { FlagAttribute, TokenPatternItem } from './index.js';

const model = await loadModel(process.env.SPARS_MODEL ?? 'en_core_web_lg');
const matcher = new TokenMatcher(model);
const flag = (attribute: FlagAttribute): TokenPatternItem => ({
  constraints: [{ attribute, predicate: { kind: 'flag', value: true } }],
  repetition: { kind: 'once' },
});
matcher.add('quantity', [{ tokens: [flag('like_num'), flag('is_alpha')] }]);
const doc = await model.processDocument('Ten people paid 1,000 dollars.', 'Tokenizer');
assert.deepEqual(await matcher.findMatches(doc), [
  { rule: 'quantity', start: 0, end: 2 },
  { rule: 'quantity', start: 3, end: 5 },
]);
```

Matcher instances support `size`, `contains(rule)`, `get(rule)`, `add(rule, patterns)`, and `remove(rule)`. Repeated additions append to an existing rule; `get` returns independent pattern copies or `null`. Matching runs on the worker pool using the retained native document, without rerunning inference or copying compiled rules. Searches share the inference admission queue and optional `maxTextLength` policy. Rule mutation is rejected with `SPARS_BUSY` while any search on that matcher is queued or active; after the returned promise settles, mutation is available again. Registration and inspection are synchronous and are not limited by inference input policies. Large match arrays still require JavaScript-thread allocation.

## Dependency matching

`DependencyMatcher` uses the same token constraints and matcher lifecycle as `TokenMatcher`. Each node after the first links to an earlier named node. Matches contain token indices in pattern-node order. All 20 native relationship operators are supported; see the [dependency matcher guide](DEPENDENCY_MATCHER.md) for their meanings and remaining scope.

```typescript
import assert from 'node:assert/strict';
import { loadModel, DependencyMatcher } from './index.js';

const model = await loadModel(process.env.SPARS_MODEL ?? 'en_core_web_lg');
const matcher = new DependencyMatcher();
matcher.add('subject', [{ nodes: [
  { id: 'verb', constraints: [] },
  { id: 'subject', constraints: [{ attribute: 'dep', predicate: { kind: 'equals', value: 'nsubj' } }], link: { left: 'verb', relation: '>' } },
] }]);
const doc = await model.processDocument('Alice runs.');
assert.deepEqual(await matcher.findMatches(doc), [{ rule: 'subject', tokens: [1, 0] }]);
```

## Phrase matching on text, lowercase, annotations, lexical flags or length

`PhraseMatcher` accepts native documents as patterns. Choose `LOWER` for Python-compatible lowercase matching, `ORTH`/`TEXT` for exact token text, `NORM` for token norms, `LEMMA`, `POS`, `TAG`, `DEP` or `MORPH` for linguistic annotations, a lexical flag such as `IS_ALPHA` or `LIKE_NUM`, or `LENGTH`. Lexical flags need the model as a second argument, `new PhraseMatcher('LIKE_NUM', model)`, for its language rules; adding a rule to a flag matcher without one throws `SPARS_INVALID_PATTERN`. `LENGTH` needs no model, and a model passed for any other attribute has no effect. It follows the same asynchronous matching, rule mutation, and admission contracts as the other matchers.

```typescript
import assert from 'node:assert/strict';
import { loadModel, PhraseMatcher } from './index.js';

const model = await loadModel(process.env.SPARS_MODEL ?? 'en_core_web_lg');
const matcher = new PhraseMatcher('LOWER');
matcher.add('hotel', [await model.processDocument('the ritz', 'Tokenizer')]);
const doc = await model.processDocument('The Ritz', 'Tokenizer');
assert.deepEqual(await matcher.findMatches(doc), [{ rule: 'hotel', start: 0, end: 2 }]);
assert.deepEqual(matcher.get('hotel'), [['the', 'ritz']]);
```

Lowercase mappings are pinned to Unicode 15.0.0 and include contextual Greek sigma. This is lowercase matching, not case folding or Unicode normalization: `ß` does not become `ss`, and composed/decomposed accents remain distinct. Phrase boundaries still follow tokenization. `get(rule)` returns copies of the unique compared token values in first-registration order: lowercase text for `LOWER`, the annotation values, such as `[['the', 'dog', 'run']]`, for `LEMMA`, `'true'` and `'false'` for flags, and decimal numbers such as `'3'` for `LENGTH`. Matching preserves overlaps and native result order. Attribute names must be uppercase. spaCy also accepts lowercase names, but here lowercase names such as `'lemma'` and the unsupported attributes `SHAPE`, the entity attributes, `SENT_START` and `SPACY` throw `SPARS_UNSUPPORTED`. Span input, callbacks, and Python pattern JSON are unsupported; see the [phrase matcher guide](PHRASE_MATCHER.md).

Annotation attributes need pattern documents processed by the model rather than only tokenized:

```typescript
import assert from 'node:assert/strict';
import { loadModel, PhraseMatcher } from './index.js';

const model = await loadModel(process.env.SPARS_MODEL ?? 'en_core_web_lg');
const matcher = new PhraseMatcher('LEMMA');
matcher.add('dog-runs', [await model.processDocument('the dog runs')]);
const doc = await model.processDocument('The dogs ran home.');
assert.deepEqual(await matcher.findMatches(doc), [{ rule: 'dog-runs', start: 0, end: 3 }]);
const tokensOnly = await model.processDocument('the dog', 'Tokenizer');
assert.throws(() => matcher.add('tokens-only', [tokensOnly]), { code: 'SPARS_INVALID_PATTERN' });
assert.equal(matcher.contains('tokens-only'), false);
```

A nonempty pattern document with no token carrying the selected annotation throws `SPARS_INVALID_PATTERN`, and the matcher is unchanged. Input documents need no annotations: a token without the annotation has the value `""` and matches only a pattern token that also lacks it. An empty morphological analysis has the value `_`. Morphology must be in spaCy's canonical order, as the model writes it; other morphology makes `MORPH` matching reject with `SPARS_UNSUPPORTED`.

Lexical flags compare whether each token has a property, so a pattern matches any tokens with the same flags. This matcher finds a number-like token followed by one that is not:

```typescript
import assert from 'node:assert/strict';
import { loadModel, PhraseMatcher } from './index.js';

const model = await loadModel(process.env.SPARS_MODEL ?? 'en_core_web_lg');
const matcher = new PhraseMatcher('LIKE_NUM', model);
matcher.add('count', [await model.processDocument('3 apples', 'Tokenizer')]);
assert.deepEqual(matcher.get('count'), [['true', 'false']]);
const doc = await model.processDocument('I ate 3 apples and ten pears.', 'Tokenizer');
assert.deepEqual(await matcher.findMatches(doc), [{ rule: 'count', start: 2, end: 4 }, { rule: 'count', start: 5, end: 7 }]);
```

## Errors and verification

An incompatible native addon can throw `SPARS_NATIVE_INCOMPATIBLE` during package import; remove stale native-library overrides or install matching package versions. Invalid matcher registration throws synchronously. Model and processing failures reject their promises with an `Error` containing a `code` and message. Codes are `SPARS_IO`, `SPARS_INVALID_MODEL`, `SPARS_UNSUPPORTED`, `SPARS_INVALID_TEXT`, `SPARS_BOUNDS`, `SPARS_INFERENCE`, `SPARS_INVALID_PATTERN`, `SPARS_BUSY`, `SPARS_INPUT_LIMIT`, and `SPARS_TEXT_TOO_LONG`. For `pipe`, errors reject iteration; `process` and `processBatch` throw immediately for invalid JavaScript argument types or unknown stages; when the queue is full, `SPARS_BUSY` takes precedence over checking individual batch elements. `vector` also throws immediately for malformed text. Use `loadModel`; constructing `Model` directly is unsupported and its TypeScript constructor is private.

After the [reference setup](DEVELOPMENT.md#set-up-the-project), run:

```sh
npm --prefix bindings/node run build
npm --prefix bindings/node run typecheck
npm --prefix bindings/node test
.venv/bin/python tools/check_docs.py
```

Tests execute against the required model assets; missing assets fail. The binding compares all discrete outputs on the frozen 190-document / 7,029-token suite, checks Unicode offsets, eight static-vector lookup cases, stage availability, error paths, mutation isolation, batches, concurrent jobs, and model lifetime during garbage collection. `tests/processing-parity.test.mts` covers model defaults, code-point length boundaries, opt-in policy reset, lazy iteration, ordering, source cleanup, and failure propagation. Type checking checks generated declarations and distinct offset types. The full [acceptance command](QUALITY.md#run-the-acceptance-checks) includes these checks. Generated mismatch reports belong under `target/reports/` and CI retains them as artifacts.

Processing defaults follow the pinned [spaCy 3.8.14 Language implementation](https://github.com/explosion/spaCy/blob/v3.8.14/spacy/language.py) and the official model configs preserved in exported manifests. The implementation follows [napi-rs worker tasks](https://napi.rs/docs/concepts/async-task) and [object conversion](https://napi.rs/docs/concepts/object). Rust and npm lockfiles pin the binding dependencies. The build corrects napi-rs's missing private-constructor declaration and fails if that generated class changes unexpectedly.

For repeatable local measurements, this command processes the short-document portion of the frozen evaluation corpus with one warmup pass and three measured passes:

```sh
UV_THREADPOOL_SIZE=1 node bindings/node/scripts/measure.mts assets/en_core_web_md-3.8.0 fixtures/evaluation-v1.json short single 3 1 96 Ner
```

Use `long` for the two long documents, `batch` or `concurrent` instead of `single`, and `Tokenizer` instead of `Ner` to measure tokenization alone. The argument `96` limits each batch to 96 documents; a smaller corpus produces a smaller group. For `concurrent`, replace `96` with `8` to measure groups of eight concurrent calls; configure different admission limits when measuring another server policy. Output separates model loading, awaited processing, peak process memory, submission time, and maximum timer gap. Awaited processing includes scheduling and result conversion; it is not a measurement of Rust inference alone. Repeat runs without competing workloads and record hardware, build mode, thread limits, and corpus identity as described in [performance measurements](PERFORMANCE.md).
