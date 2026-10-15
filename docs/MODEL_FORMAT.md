# Native model formats

## Capability-declared format v2

Format v2 retains the v1 resource and tensor layout below and adds a required `capabilities` record. Its typed identifiers declare the language, tok2vec architecture, embedding architecture, encoder, tagger, transition model, and lemmatizer semantics. Model name and version describe provenance rather than select inference code. Currently implemented identifiers are English, `spacy.Tok2Vec.v2`, `spacy.MultiHashEmbed.v2`, `spacy.MaxoutWindowEncoder.v2`, `spacy.Tagger.v2`, `spacy.TransitionBasedParser.v2`, and `en-rule-v1`. Unknown or missing capabilities fail loading. See [model-v2.schema.json](model-v2.schema.json).

The ordered `pipeline` may select and reorder the built-in components subject to checked dependencies: tagger/parser require a preceding shared tok2vec, the English attribute ruler requires tagger/parser, and the English lemmatizer requires the attribute ruler. NER has its own encoder. Duplicate components and missing dependencies fail loading. A requested stage absent from the pipeline returns an error. The v2 manifest still requires the six component configurations; arbitrary component sets and other language/architecture implementations remain future work.

Both encoder `static` fields may be null. Without a projection, the embedding concatenation omits the static-vector block. The only permitted empty tensor shape is `vectors: [0, 0]`, with an empty key map and no static projection. All other tensors require positive dimensions. The small English model has 67 tensors; medium and large have 69. Models without static vectors retain shared contextual rows in processed documents for token/span/document vector access. Such documents serialize as native snapshot version 2, which validates row counts, rectangular dimensions, and finite values on restoration. Documents without contextual rows retain snapshot version 1. Documents with entity IDs serialize as version 3, which also carries contextual rows when present; see [entity IDs](ENTITIES.md#entity-ids).

## Legacy format v1

The manifest is a JSON file that describes the model. Tensors are numbered arrays containing its learned weights. This format lets a Rust application load the exported model without Python.

A model directory contains `manifest.json`, `weights.safetensors`, model LICENSE, LICENSES_SOURCES, upstream source/license notices and source-lock.json. All numeric parameters and static vectors are F32, row-major, in SafeTensors. The manifest's SHA256 authenticates the weights against that manifest (it is not a digital signature for an untrusted manifest). Acquisition separately pins the official wheel SHA256.

Required top-level fields:

- format_version=1; model=en_core_web_md; model_version=3.8.0; versions with spaCy 3.8.14 and Thinc 8.3.13; official source URL, wheel digest, metadata/license, original config text and ordered pipeline.
- tensors: map of tensor keys to {shape: positive dimension array, dtype: F32}. SafeTensors validates offsets/lengths; the loader checks actual shapes, dtypes, finite values and operation-specific cross-dimension constraints.
- tokenizer: prefix/suffix/infix/url/token_match; rules map source strings to ordered ORTH/NORM entries. python_patterns retain original Python regexes. faster_heuristics is true for the supported model; older v1 exports without this field use true. Other values are unsupported. The second exception pass matches token sequences produced without special-case rules. regex_dialect identifies explicit Python-Unicode shorthand translation.
- lexical: pinned Unicode version, sorted inclusive property ranges, lowercase mapping, stop words, number words, punctuation sets, TLDs, email regex.
- symbols: reserved spaCy IDs. norms: hashed lexical keys to string replacements, with model lookups overriding BASE_NORMS. IDs use JSON integer values or decimal string map keys; they must not be rounded through JavaScript Number.
- tok2vec: width, ordered attrs, hash table references and seeds, static projection, mix maxout/normalization parameters, windows, encoder layers and padding count.
- tagger: ordered labels and affine parameter references.
- parser/ner: ordered action names, reduction/lower/upper tensor references; NER additionally contains its independent tok2vec. Format v1 specifies greedy maxout transition inference with no unseen-class mask.
- attribute_rules: ordered patterns/attributes/index; only the pinned model's finite TAG/DEP/LOWER/IS_SPACE predicates with equality/IN/NOT_IN/REGEX are accepted. An assigned `POS` must be a spaCy universal POS tag or empty, because spaCy cannot store any other value.
- lemmas: lemma_index, lemma_exc, lemma_rules per POS.
- vector_keys: lexical hash to row; vectors tensor provides row width/count.

The loader reads a literal positive unsigned 32-bit `batch_size` from the preserved config's `[nlp]` section. `Model::default_batch_size()` exposes this processing metadata; the Node binding uses it as the initial batch size. The supported official releases declare 256. An absent config or absent field uses the spaCy `Language` constructor default of 1,000. Duplicate `[nlp]` sections or `batch_size` fields, zero, malformed values, and interpolation in `batch_size` fail loading. This narrow reader does not implement spaCy's complete Confection configuration system or infer new runtime capabilities from config text. Existing model exports require no format change.

Dimensions and label counts are loaded, from the validated official model configuration. The pinned architecture semantics define concatenation, zero padding, learned parser padding, residual order, layer normalization epsilon 1e-8, and first-index tie breaking. Lower transition weights are [feature, output, piece, input]; pad is [1, feature, output, piece]. Unnormalized tagger scores are used.

Unsupported versions, components, feature types or operators fail explicitly. Changing this representation or numerical semantics requires a new format version and fresh compatibility evidence. Model acquisition is never performed by loading or processing. The manifest uses ordinary JSON; [model-v1.schema.json](model-v1.schema.json) specifies its structural contract. The loader enforces structural and tensor constraints in [validation.rs](../crates/spars/src/validation.rs) and the `validate` function in [neural.rs](../crates/spars/src/neural.rs).

The [native installer](MODEL_INSTALLATION.md) writes v1 for `md` and v2 for `sm`/`lg`, and adds `installation.json`, which records the exact installation identity, conversion-recipe digest, and file inventory. This receipt belongs to the installation tool; the inference library continues to load the model directory without networking or installer dependencies.
