# Contributing agent guidance

## Repository layout

Before creating or moving a project, read and follow [repository layout rules](docs/REPOSITORY_LAYOUT.md). Keep the root Cargo manifest a virtual workspace, place Rust libraries and command-line tools under `crates/`, and place language bindings under `bindings/`. The layout rules define the independent-consumer exception and the checks required when paths change.

## Project scope

SpaRs (Cargo package `spars-nlp`, Rust import `spars`) is a standalone native Rust NLP library. The current supported models are the official `en_core_web_sm`, `en_core_web_md`, and `en_core_web_lg` 3.8.0 exports, using spaCy 3.8.14 and Thinc 8.3.13 as the reference. Keep broader compatibility status in `docs/COMPATIBILITY.md`; one English pipeline does not establish full spaCy parity.

Production tokenization and inference must run natively in Rust. Python belongs only in development, export, and reference tooling. Keep acquisition explicit and processing offline. Use official spaCy, Thinc, and model sources identified by `reference/source-lock.json` and model metadata. Keep third-party executable demos and their assets out of the project. Record licenses and provenance when adding resources. Keep downstream application features in separate projects.

## spaCy behavioral parity

Use the pinned official spaCy implementation as the default design and behavior reference for every ported feature. Before implementing or extending a feature, inspect its upstream source and tests, then capture the relevant behavior in reference comparisons. Preserve supported inputs, defaults, output values and ordering, edge cases, and validation behavior wherever compatible with native Rust, memory safety, and the project's explicit contracts. Use idiomatic Rust types and ownership while keeping the observable behavior equivalent; API spelling and internal code structure may differ.

For matching, check attribute and operator semantics, repetition, overlapping matches, and result ordering. For document manipulation, check preservation of text and whitespace, token and span boundaries, annotation updates, and what happens to existing token/span views after edits. Define these contracts from upstream behavior before choosing data structures or optimization strategies.

When a behavioral difference is necessary, document the upstream behavior, the reason for the difference, and its effect on callers in the relevant guide and `docs/COMPATIBILITY.md`, with tests for the chosen behavior. Keep unsupported and unverified cases explicit. Preserve existing exact-output requirements and numerical tolerances; a documented mismatch remains a compatibility gap unless the user explicitly approves a changed acceptance criterion.

## Model extensibility

The compatibility target is native loading and inference for all official spaCy pretrained pipelines, across languages, model sizes, and architectures. The verified English models are an implementation milestone, not the design boundary. Loading custom-trained pipelines is future work; preserve extension points for their model data and native component implementations. Training models is a separate capability.

Separate model identity and acquisition records from runtime capabilities. Model names, release URLs, checksums, archive limits, and provenance belong in typed catalog data. Runtime compatibility must ultimately depend on versioned component factories, neural architectures, language resources, and their validated configuration, rather than an enumeration of model names. A new model using supported capabilities should not require changes to inference code. A new capability should have one reusable implementation, not a model-specific branch.

Build pipeline execution from declared component order and dependencies. Keep language behavior and resources separate from shared neural operations. Preserve strong typing and explicit validation at each boundary; extensibility does not justify generic unvalidated configuration, silent component omission, or Python execution in production. Future custom pipelines should use the same configuration and component contracts, with explicit registration for additional native implementations.

Follow the staged architecture plan in `docs/PROGRESS.md`. Preserve existing compatibility checks until their replacements have reference and malformed-input coverage. Distinguish the ability to decode a package, the capabilities needed to execute it, and release-specific parity evidence. Do not claim that removing a model-name restriction establishes model support. Keep new model reference fixtures separate and retain existing model coverage.

## Before changing code

Read the working-tree status and the relevant source, tests, and documentation before editing. Use `docs/COMPATIBILITY.md` and `docs/PROGRESS.md` to locate supported behavior and remaining work, then verify claims against the implementation. Historical reports, earlier messages, and successful runs from another revision are context, not proof that the current checkout passes.

Choose one bounded change with an observable acceptance criterion. When a prerequisite is blocked, record the exact blocker in the task response and continue independent work. Update the durable compatibility inventory only when supported behavior or an enduring limitation changes.

## Documentation

Keep each prose paragraph on one source line; do not hard-wrap Markdown to a fixed column width. Keep structural line breaks for headings, list items, tables, and code blocks.

Write API doc comments for callers, following the API doc comment standard in `docs/QUALITY.md`: each comment must tell a developer something they need at the point of use that the name and type do not, such as valid values, units, absence, errors, or relationships to other fields. Delete comments that only restate the name or type.

Commit documentation for lasting supported behavior, repeatable workflows, settled design decisions, and enduring limitations. Keep active-task notes, exploratory profiling, interim benchmark runs, and temporary findings in ignored `target/` files or temporary storage. Update public guides after the result is verified; do not commit a new document or report for each work step. Keep reusable diagnostic tools separate from their temporary output. Write generated test, verification, and benchmark reports under ignored `target/reports/`; CI retains them as artifacts. Keep tests, fixtures, expected results, checksums, and source/model provenance in Git.

Write repository documentation for users and contributors who have not read any development conversation. Describe the software, supported behavior, limitations, design rationale, setup, verification, and contribution requirements.

- Exclude conversation summaries, agent handoffs, user requests, local workspace arrangements, commit organization, and repository-setup narration.
- Describe the resulting behavior rather than recounting how a task was assigned or completed. For example, document the supported model and parity suite instead of saying that an agent finished the requested milestone.
- Keep `docs/PROGRESS.md` as a durable implementation inventory with evidence, unresolved technical blockers, and concrete next work, not a session diary.
- Preserve technically relevant history: source provenance, fixture influence, resolved compatibility differences, and reasons for implementation constraints.
- Scope claims to their evidence. Distinguish implemented, verified, partial, and unsupported behavior. Link validation claims to the relevant tests and reproducible commands; explain how to obtain current CI artifacts instead of linking to ignored local reports; do not infer linguistic accuracy from reference agreement.
- Avoid transient publication or CI-status claims in prose. Document how to install, verify, or locate CI results instead. Generated report metadata must describe only what that generator actually checks.
- Save only project-relative filesystem paths in fixtures, reports, and documentation. Remove external host paths from captured diagnostics before saving them. Never record personal home directories or temporary-directory locations. Keep upstream source URLs intact.
- Use repository-relative links and portable commands. Do not assume readers know a URL, path, or artifact mentioned in a conversation.
- When changing behavior, reconcile the README, compatibility inventory, model format, and validation documentation where relevant. Do not rewrite license notices or remove provenance to simplify prose.

## Quality reviews

Follow `docs/QUALITY.md` for every behavior change. For a bug fix or new behavior, first demonstrate the failure with a reference or invariant test. For behavior-preserving refactors and optimizations, establish the baseline and use independent equivalence checks where the algorithm changes; existing behavior tests may already pass. Run focused checks and the acceptance checks appropriate to the change. Add an executed developer example when public behavior changes; otherwise rerun affected examples. Keep fixture inputs and expected outputs frozen; review checksum changes explicitly.

Delegate independent review of substantial behavior changes to the project agents `reference_review`, `test_review`, and `docs_review`, using `.codex/agents/*.toml`. Give each reviewer the changed files and a bounded question. Reviewers report findings; the main agent owns edits and test execution. If custom roles are unavailable, pass the matching instructions to a standard subagent. Wait for findings and resolve concrete issues before claiming verification. Changes that add or modify public Rust items or Node binding declarations need `api_docs_review`, which applies the API doc comment standard in `docs/QUALITY.md`. Documentation-only changes need documentation review. Tooling and CI changes need test review, including failure paths when reporting or acceptance logic changes. Runtime changes also require the performance review below. Reviewers supplement executed tests; their approval alone does not establish correctness.

Give implementation agents separate file ownership and bounded tasks. The coordinating agent owns integration and schedules builds, tests, and benchmarks so they do not compete. Reviewers inspect independently and return findings; they do not edit the files they are reviewing.

Write plain English and explain necessary technical terms. Guide examples must run from their Markdown source during `tools/check_docs.py`; compiling without running does not count as a tested example.

## Strong typing

Typing is a required part of every change, not a cleanup step. Use concrete types throughout Rust, Python and TypeScript, including tools and tests. Represent known records with structs, enums, dataclasses, or TypedDicts rather than generic dictionaries. Annotate Python function parameters and return values. Validate external JSON and upstream library values at the boundary, then pass typed records into computation. Generic JSON is appropriate for decoding, serialization, and deliberately malformed test inputs, not as a substitute for model or domain types.

Run strict Pyright and `tools/check_typing_policy.py` through the acceptance script. Fix errors rather than adding `Any`, unchecked casts, ignored diagnostics, or weaker checker settings. Keep local stubs narrow and accurate to the pinned upstream interfaces; exercise those interfaces against the real reference environment. Type annotations do not replace shape, bounds, or semantic validation.

Public binding types must be as precise as the values they carry, so callers get autocomplete and compile-time errors instead of runtime failures. Type closed value sets as literal unions or enums, never as plain `string` or `number`: IOB tags, Universal POS tags, matcher attribute names, predicate kinds and operators, defaults, stages and similar fixed vocabularies. For open sets with known members, such as entity labels, fine-grained tags, dependency labels and model names, offer the known values for autocomplete while still accepting other strings, and let callers register their own values (for example through an interface they can augment) so custom labels are typed too. Use branded unit types for indices and offsets. When the generator cannot express a precise type, add an explicit override or declaration post-processing rather than accepting the weaker type, and test that every literal list matches the values the native code accepts and produces. Avoid `any`, and avoid `unknown` or untyped values in public return types and fields; `unknown` is appropriate for parameters that are narrowed, such as type guards.

## Performance changes

Speed is a required design constraint for every runtime change. Delegate an independent review to `performance_review` for changes to runtime algorithms, model loading, data structures, batching, or runtime dependencies, including new features. Give it the affected paths and available before/after measurements. If the custom role is unavailable, use a standard subagent with its instructions. Resolve measured regressions before completion; report missing measurements as unverified performance.

Profile the actual workload before choosing an optimization. Measure the final implementation against the same baseline, model, corpus, build mode, thread limits, and warmup procedure. Keep loading, inference, and process memory separate. Run measurements without competing test or benchmark processes. Preserve an identifiable baseline binary or checkout before editing, and record the measured source revision and any working-tree changes. Repeat measurements, alternate before/after order where practical, and inspect variation before claiming a change. Cover short and long inputs, repeated and unfamiliar text, and input-size scaling relevant to the algorithm. Report throughput, loading, and memory tradeoffs. Treat small differences within observed variation as inconclusive unless stronger statistical evidence supports them. Differences between cumulative pipeline timings are estimates, not exact component costs.

Optimize measured hot paths with reusable buffers, precompiled immutable resources, cached per-call features, and matrix operations across token rows where appropriate. Preserve model reuse and call isolation. Keep numerical tolerances and discrete-output expectations unchanged; an optimization that fails parity is unfinished.

Keep unsafe numerical code inside a small checked wrapper. Validate dimensions, strides, bounds, and nonaliasing assumptions, explain the safety argument, and test rectangular, empty, and malformed inputs against an independent calculation. Changes to such wrappers need independent review.

## Implementation and verification

Keep changes small and focused. Prefer pure computation, immutable loaded models, and per-call state. Preserve exact text, whitespace, and distinct byte offsets, code-point offsets, and token indices. Unsupported configurations and unavailable annotations must remain explicit rather than being omitted or fabricated.

Keep source files focused, unit tests in separate `tests.rs` modules, integration tests in each crate's `tests/` directory, and runnable examples in its `examples/` directory. The library lives in `crates/spars/`. Export tooling must stay outside normal consumer Cargo builds.

Do not change frozen evaluation inputs or expected outputs to make an implementation pass. Add regressions separately and record which evaluation data influenced the implementation. Expected results must come from the pinned official reference or an independently justified invariant. Preserve declared numerical tolerances and exact discrete-output requirements; record unresolved mismatches.

For runtime or model changes, follow the setup in `README.md` and run `.venv/bin/python tools/verify.py`. Ordinary `cargo test` skips model-dependent tests; the acceptance run requires `--include-ignored` and available model assets. For documentation-only changes, check factual claims, links, and command consistency; a full model export and parity run is unnecessary unless affected. For tooling and CI changes, exercise affected success and failure paths, including missing output directories. Keep report generation independent of tracked files; a clean checkout must create its own ignored output directories.

Preserve unrelated working-tree changes, including generated verification reports. Keep commits small and scoped, and exclude generated reports and transient notes. At handoff, state what changed, which checks actually ran, unresolved limits, and whether CI is pending or completed for the exact commit. Do not describe local success as CI success. Repository pushes, releases, and registry publication require user authorization; authorization to push does not imply authorization to publish a package or release.
