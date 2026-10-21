# SpaRs for Node.js

Native Rust NLP using official spaCy pretrained weights. Node.js 22 or newer is required. Release packages target Linux x64/ARM64 with glibc or musl, macOS x64/ARM64, and Windows x64/ARM64; other platforms require a source build and separate verification. See the [Node guide](https://github.com/tareksanger/SpaRs/blob/main/docs/NODE.md) for the typed API and limitations.

Model loading and NLP inference (`loadModel`, `process`, `processBatch`, and `pipe`) run asynchronously on background worker threads, keeping CPU-heavy inference off the event loop. Input copying and result creation still use the JavaScript thread. Each model defaults to spaCy's 1,000,000-code-point document limit (`model.maxLength`) and its exported batch size (`model.batchSize`, 256 for the supported releases). `pipe` lazily yields documents from an iterable. Inference admission defaults to two active jobs and 32 queued jobs per package instance and JavaScript isolate; excess calls reject with `SPARS_BUSY`. Customize these limits with `configureExecution`; additional input caps are opt-in through `configureInputLimits`; see the Node guide for configuration, errors, and worker-pool limits. The [error-handling example](https://github.com/tareksanger/SpaRs/blob/main/docs/NODE.md#handle-processing-errors) shows a bounded response to `SPARS_BUSY`, batching independent documents within `SPARS_INPUT_LIMIT` policies, and synchronous argument errors. Admission and input policies bound queued work and input size; they do not reduce the JavaScript-thread cost of converting large accepted results.

Upgrading from npm 0.2.0 introduces bounded inference admission and a document-length limit. Applications submitting many concurrent calls or very long documents should review the [migration cases and fixes](https://github.com/tareksanger/SpaRs/blob/main/docs/NODE.md#upgrade-from-npm-020) before upgrading.

After installing a published version:

```sh
npm install @spars/node
npx spars download en_core_web_sm
```


Use the executable examples in the [Node guide](https://github.com/tareksanger/SpaRs/blob/main/docs/NODE.md) to load a model and inspect token annotations.

Model acquisition is explicit. Loading and inference run offline. Set `SPARS_MODEL_DIR` to share an installation directory across applications. The verified model releases are `en_core_web_sm`, `en_core_web_md`, and `en_core_web_lg` 3.8.0; this does not establish full spaCy compatibility. Model assets retain their own licenses and are acquired separately.

The package includes the project MIT license and upstream notices. Platform packages also include Cargo dependency license texts. SpaRs is independent of Explosion.
