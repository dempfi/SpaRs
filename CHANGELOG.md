# Changelog

## [0.4.0](https://github.com/tareksanger/SpaRs/compare/v0.3.0...v0.4.0) (2026-10-16)


### ⚠ BREAKING CHANGES

* store spaCy entity IDs on tokens and entities ([#96](https://github.com/tareksanger/SpaRs/issues/96))
* **node:** type fixed vocabularies as literal unions ([#88](https://github.com/tareksanger/SpaRs/issues/88))

### Features

* add a native EntityRuler ([#97](https://github.com/tareksanger/SpaRs/issues/97)) ([71fdfa4](https://github.com/tareksanger/SpaRs/commit/71fdfa489e9d6ecabede42b012141ba72769671f))
* add checked entity annotation updates ([#87](https://github.com/tareksanger/SpaRs/issues/87)) ([1dafc82](https://github.com/tareksanger/SpaRs/commit/1dafc82566da99c7d537fe812ff936c735cfcb25))
* add IS_SUBSET, IS_SUPERSET and INTERSECTS set predicates to matchers ([#81](https://github.com/tareksanger/SpaRs/issues/81)) ([8d904cd](https://github.com/tareksanger/SpaRs/commit/8d904cd15a11777f87cdd77945319c759bd1882f))
* add LENGTH conditions with numeric comparisons to matchers ([#79](https://github.com/tareksanger/SpaRs/issues/79)) ([f8a1458](https://github.com/tareksanger/SpaRs/commit/f8a1458c58a883a25f873b9042199c0cdb944320))
* add lexical flag and LENGTH attributes to PhraseMatcher ([#85](https://github.com/tareksanger/SpaRs/issues/85)) ([1f9270b](https://github.com/tareksanger/SpaRs/commit/1f9270ba0c95814adb32b00bb11e3c71260a616f))
* add lexical flag conditions to token and dependency matchers ([#76](https://github.com/tareksanger/SpaRs/issues/76)) ([597ddb4](https://github.com/tareksanger/SpaRs/commit/597ddb407de2e9e33da94776167fafa135601fd9))
* add NORM, LEMMA, POS, TAG, DEP and MORPH attributes to PhraseMatcher ([#84](https://github.com/tareksanger/SpaRs/issues/84)) ([8f4a961](https://github.com/tareksanger/SpaRs/commit/8f4a9612c310023e6073a10e9fa0b8432cefded6))
* add remaining lexical flag conditions to matchers ([#80](https://github.com/tareksanger/SpaRs/issues/80)) ([ad9266e](https://github.com/tareksanger/SpaRs/commit/ad9266e97a4125e2a0e146893c45de67426dce00))
* add span groups to Rust documents ([#111](https://github.com/tareksanger/SpaRs/issues/111)) ([aaab96c](https://github.com/tareksanger/SpaRs/commit/aaab96cccbce4b2c193b15ce3b921b60b60478af))
* add the SpanRuler to Rust documents ([#114](https://github.com/tareksanger/SpaRs/issues/114)) ([5065c1d](https://github.com/tareksanger/SpaRs/commit/5065c1dd78e051b598a82e355e53531fd0740dbd))
* add Unicode LOWER conditions to token and dependency matchers ([#74](https://github.com/tareksanger/SpaRs/issues/74)) ([af3dd1b](https://github.com/tareksanger/SpaRs/commit/af3dd1b8dcdc1a9e3dd91ddde3919587633d8a0c))
* **node:** add EntityRuler ([#109](https://github.com/tareksanger/SpaRs/issues/109)) ([74267d9](https://github.com/tareksanger/SpaRs/commit/74267d97fd78c12904220cced0245dd440d8a5fb))
* **node:** add span groups to native documents ([#113](https://github.com/tareksanger/SpaRs/issues/113)) ([48683da](https://github.com/tareksanger/SpaRs/commit/48683daa71f17689172be93ddc30bcd50ee64cc9))
* **node:** add the SpanRuler to native documents ([#123](https://github.com/tareksanger/SpaRs/issues/123)) ([ec1b72d](https://github.com/tareksanger/SpaRs/commit/ec1b72dbdc16ec74b7a02d5b5e4ddbf5024a8ae9))
* **node:** type fixed vocabularies as literal unions ([#88](https://github.com/tareksanger/SpaRs/issues/88)) ([525734e](https://github.com/tareksanger/SpaRs/commit/525734edefdbe5c3cba271865f347be00e39254b))
* store spaCy entity IDs on tokens and entities ([#96](https://github.com/tareksanger/SpaRs/issues/96)) ([20e113c](https://github.com/tareksanger/SpaRs/commit/20e113ceaeee7ebf0789bf79b762318152ae4f7b))


### Bug Fixes

* match spaCy morphology field order and POS alias folding ([#82](https://github.com/tareksanger/SpaRs/issues/82)) ([ab5f186](https://github.com/tareksanger/SpaRs/commit/ab5f18638180da2a4c561ad7a33f6d057e0ad98c))
* **matcher:** require annotations only where spaCy's E155 does ([#121](https://github.com/tareksanger/SpaRs/issues/121)) ([d96508f](https://github.com/tareksanger/SpaRs/commit/d96508f580c3923e179f89573271a708814ad198))
* reject snapshots whose entities spaCy could not read from the tags ([#122](https://github.com/tareksanger/SpaRs/issues/122)) ([1efe719](https://github.com/tareksanger/SpaRs/commit/1efe7193e2d4ba2fb226a8467f6a89f36ad09cea))


### Performance Improvements

* index token patterns by their first token ([#112](https://github.com/tareksanger/SpaRs/issues/112)) ([da09a96](https://github.com/tareksanger/SpaRs/commit/da09a960b5ff0bd3c3ef38bc667c99a5962bcb00))

## [0.3.0](https://github.com/tareksanger/SpaRs/compare/v0.2.0...v0.3.0) (2026-09-28)


### ⚠ BREAKING CHANGES

* **node:** align processing defaults with spaCy ([#58](https://github.com/tareksanger/SpaRs/issues/58))

### Features

* add native exact-text PhraseMatcher with spaCy parity ([#59](https://github.com/tareksanger/SpaRs/issues/59)) ([843dca6](https://github.com/tareksanger/SpaRs/commit/843dca6f654bd45ba5eaf5f9001a618fe4a90735))
* **node:** add Intel macOS release packages ([#53](https://github.com/tareksanger/SpaRs/issues/53)) ([f044210](https://github.com/tareksanger/SpaRs/commit/f044210ef41c2f8e9b9d9d86004cb0d92aaf613f))
* **node:** add Linux ARM64 release packages ([#42](https://github.com/tareksanger/SpaRs/issues/42)) ([919d1e3](https://github.com/tareksanger/SpaRs/commit/919d1e35cf410437a90001b7e6bc642dcd5c1af3))
* **node:** add native Alpine musl release packages ([#56](https://github.com/tareksanger/SpaRs/issues/56)) ([c9c1a28](https://github.com/tareksanger/SpaRs/commit/c9c1a28657908ee95973bf082151403206ede45d))
* **node:** add Windows ARM64 release packages ([#55](https://github.com/tareksanger/SpaRs/issues/55)) ([28029ce](https://github.com/tareksanger/SpaRs/commit/28029ce1e808f6dc117c1480659f93d5301b6b84))
* **node:** add Windows x64 release packages ([#46](https://github.com/tareksanger/SpaRs/issues/46)) ([362d6b6](https://github.com/tareksanger/SpaRs/commit/362d6b6a08c2a5fa5b68fbe281a0bd17fcd884bd))
* **node:** align processing defaults with spaCy ([#58](https://github.com/tareksanger/SpaRs/issues/58)) ([9cdfb74](https://github.com/tareksanger/SpaRs/commit/9cdfb74c2cc0e04b8509eb64440bce58b6e4b4cd))
* **node:** bound active and queued inference jobs ([#43](https://github.com/tareksanger/SpaRs/issues/43)) ([ccfe95e](https://github.com/tareksanger/SpaRs/commit/ccfe95e5dd0cf96c3a19cb6e13e00dee2a1bf3e5))
* **node:** expose dependency matching ([#67](https://github.com/tareksanger/SpaRs/issues/67)) ([eac0fc4](https://github.com/tareksanger/SpaRs/commit/eac0fc490053359f090c07cc6c62c1a51d59e99c))
* **node:** expose document vectors and checked similarity ([#65](https://github.com/tareksanger/SpaRs/issues/65)) ([69ce530](https://github.com/tareksanger/SpaRs/commit/69ce5303e0306e9e11e4b75a78739693cddcf7ee))
* **node:** expose lexical attributes with exact identities ([#62](https://github.com/tareksanger/SpaRs/issues/62)) ([2308545](https://github.com/tareksanger/SpaRs/commit/23085459bd488745e870b6cadb9b4fa77c35e223))
* **node:** expose offline model-store management ([#63](https://github.com/tareksanger/SpaRs/issues/63)) ([5fd07dd](https://github.com/tareksanger/SpaRs/commit/5fd07dd3853145cd40fde3ad98b40e04eeaa22d6))
* **node:** expose phrase matching with Unicode LOWER ([#68](https://github.com/tareksanger/SpaRs/issues/68)) ([b189086](https://github.com/tareksanger/SpaRs/commit/b18908665590d7a7377fcee160b614d7c38430e3))
* **node:** expose reusable token matching ([#66](https://github.com/tareksanger/SpaRs/issues/66)) ([e3dff3e](https://github.com/tareksanger/SpaRs/commit/e3dff3eadd70491084bc95e6c03d4857d20b5f89))
* **node:** expose token and span views with traversal ([#64](https://github.com/tareksanger/SpaRs/issues/64)) ([446823f](https://github.com/tareksanger/SpaRs/commit/446823f0a3d4f7b9345f3cda1192ada693d983e6))
* **node:** reject oversized inference inputs before native copies ([#44](https://github.com/tareksanger/SpaRs/issues/44)) ([12570e7](https://github.com/tareksanger/SpaRs/commit/12570e71d0bd1e6724ce9941a861f06d7dd38bf7))
* **node:** retain native documents and validated snapshots ([#61](https://github.com/tareksanger/SpaRs/issues/61)) ([97b44a5](https://github.com/tareksanger/SpaRs/commit/97b44a5edb16046a8770647a94a465e1cac3c97a))
* **node:** retain release assets and add Cargo recovery commands ([#57](https://github.com/tareksanger/SpaRs/issues/57)) ([28aed63](https://github.com/tareksanger/SpaRs/commit/28aed63264e9534a8953c9edde57a96ac6560d20))
* **release:** add cargo publish-npm command ([ce8009e](https://github.com/tareksanger/SpaRs/commit/ce8009e2d9548c188ba5025bd517dbe44409c093))
* **release:** add verified native npm publication workflow ([acb9c9e](https://github.com/tareksanger/SpaRs/commit/acb9c9e9b6fa297f7ec12c5e7e01ee75f7c107fc))


### Bug Fixes

* **ci:** canonicalize Windows release smoke paths ([#71](https://github.com/tareksanger/SpaRs/issues/71)) ([8f03055](https://github.com/tareksanger/SpaRs/commit/8f030551034d5245497bb5c46d47bf417c97a20c))
* repair release tooling and isolate verification model stores ([02909c2](https://github.com/tareksanger/SpaRs/commit/02909c21c4ffd659aa17bc3885629cd48f7f6fa7))

## [0.2.0](https://github.com/tareksanger/SpaRs/compare/v0.1.0...v0.2.0) (2026-09-25)


### Features

* add cargo release command for workflow preparation ([118f806](https://github.com/tareksanger/SpaRs/commit/118f8068bb8c58e7cf1102347d64c4afdc403db5))
* add native model downloads and name-based loading ([#29](https://github.com/tareksanger/SpaRs/issues/29)) ([ed1b516](https://github.com/tareksanger/SpaRs/commit/ed1b516ce6997d5137b05b0068ca982f1d8223f2))


### Bug Fixes

* follow the default branch throughout releases ([ff03243](https://github.com/tareksanger/SpaRs/commit/ff0324369b9a7bf4534bfe87c20bc753c5a708b7))
* stale lock file ([366e938](https://github.com/tareksanger/SpaRs/commit/366e93854bcb0b491f37c91476b54a5a5dc43c9a))

## 0.1.0 (2026-09-25)


### Features

* **ci:** automate reviewed source releases ([1e064e1](https://github.com/tareksanger/SpaRs/commit/1e064e140f96522fdad5f04d6a8731b9ef59f78c))

## Changelog

Release entries are prepared in release pull requests and published after acceptance CI passes for the merged commit.

## Initial release overview

The first release PR must include this overview in its versioned entry before merging:

- Native Rust tokenization and inference for the official English `sm`, `md`, and `lg` 3.8.0 models, referenced against spaCy 3.8.14 and Thinc 8.3.13.
- Explicit native model installation and separate Node.js bindings built from source.
- Model assets acquired separately; no model weights bundled in the Rust crate.
- Experimental API; inference supports only the three English pipelines listed above. See [compatibility](docs/COMPATIBILITY.md) for supported behavior and remaining limitations.
