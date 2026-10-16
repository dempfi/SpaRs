# Measure performance

Measure the checked-out implementation before making a speed claim. Loading time, inference time, and process memory answer different questions and must be reported separately. Keep generated measurements in ignored `target/reports/`; CI saves its measurements as downloadable run artifacts. Benchmark results are not committed to Git.

## Compare with official spaCy

After the model setup and dependency fetching in the [README](../README.md), run from the repository root:

```sh
.venv/bin/python tools/compare_python.py --output target/reports/python-comparison.json
```

The tool compares SpaRs and official spaCy using the same `en_core_web_md` 3.8.0 model and fixed `fixtures/evaluation-v1.json` corpus. Rust uses a release build. Processing is sequential, with Python numerical-library thread limits set to one. Each loaded model receives one warmup pass and three measured passes by default. This compares individual calls, not batching.

The short group contains 96 documents; three passes measure 288 document calls and 3,264 tokens. The long group contains two documents; three passes measure 6 calls and 13,440 tokens. Imports, model loading, corpus reading, document destruction, and JSON output are outside the inference timer. Python also materializes sentence, noun-chunk, and entity spans, which SpaRs computes during processing.

The generated report includes raw timings, workload counts, hardware, versions, source hashes, and measurement limits. Each profile runs SpaRs before spaCy in separate processes. Operating-system caches are not cleared, and the synthetic long documents repeat one sentence. One run does not establish performance on other hardware, text, or batch sizes. Repeat comparisons to check whether an apparent improvement exceeds normal timing variation.

## Measure native loading and inference

Run from the repository root on macOS or Linux:

```sh
.venv/bin/python tools/benchmark.py --output target/reports/benchmark.json
```

The script builds the release measurement worker and runs load-only, short-document, and long-document groups in separate processes. Python launches the native worker and collects system information; inference runs in Rust. The process needs permission to read CPU and RAM information.

The report records loading time, document and token throughput, per-document durations, and peak process memory. The median is the middle duration, or the average of the two middle durations when the sample count is even. The 95th percentile is a duration at or above 95% of the samples. With only six long-document measurements, that percentile is the slowest sample and does not reliably estimate production tail latency.

Peak memory is the process's highest resident memory usage, measured by the operating system. It includes loading and model storage, even for the inference groups. Python comparison peaks also include the interpreter. These values are not inference-only allocation counts or proof that no memory leak exists. Do not subtract peaks from separate processes to estimate processing memory.

## Profile pipeline stages

Measure successive pipeline prefixes to locate expensive stages:

```sh
cargo run --release --offline --example profile_stages -- assets/en_core_web_md-3.8.0 fixtures/evaluation-v1.json long 3
```

Use `short` in place of `long` for the short-document group. Each total includes all stages before the named stage. Subtracting adjacent totals estimates a component's cost, with timing noise; the tagger prefix includes the shared encoder, and the attribute prefix includes noun chunks. One warmup pass precedes three measured passes. Model loading and result destruction are excluded, and stage order rotates between passes. These measurements help locate expensive work; they do not predict the speedup of a proposed fix.

## Evaluate an optimization

Run the full acceptance suite to establish unchanged annotations before measuring speed. Compare the old and new implementations on the same machine, with the same model, corpus, release build settings, thread limits, warmup, and number of passes. Avoid competing test or benchmark processes. Record hardware, source and model versions, workload counts, loading time, inference throughput, and memory separately.

Keep exact annotation agreement and the existing numerical tolerances. Measure both short and long inputs, and add a relevant workload when the changed algorithm has different cost as input size grows. Report timing variation and memory tradeoffs alongside speed improvements. There is no automatic speed threshold yet; baseline variation needs to be measured before a trustworthy threshold can be chosen.

## Phrase dictionaries

Build `cargo build --release --offline --example phrase_benchmark`, then run `target/release/examples/phrase_benchmark phrase shared 1000 4096 16`. The arguments select engine (`phrase`, the existing `token` matcher with text conditions, `lower` for Token Matcher lowercase conditions, or `lower-upper` for lowercase conditions on uppercase input that must be lowercased per token), scenario (`independent`, `shared`, `miss`, `dense`, `pruned`), pattern count, document token count and phrase length. Dense patterns use repeated Unicode tokens. The pruned case registers many labels on one phrase and removes all but one before searching; it reports pruning separately. Setup creates validated documents before timing; reports separate pattern registration, cold symbol initialization, the first matching call, five matching calls after two warmups, removal, and output count. Matching timings include result destruction. Wrap each process with the platform's peak-memory tool (for example `command time -l` on macOS). Process memory includes input preparation, matcher storage and outputs; it is not isolated matcher heap usage.

Compare the same workload and release binary, repeat processes, alternate engines and avoid concurrent builds. Token Matcher is a functional baseline for exact phrase patterns, not the same result-order contract. For algorithm changes preserve the previous PhraseMatcher binary too. Large output counts and long shared prefixes require separate workloads; a short no-match case cannot establish overall matching performance.

## Token Matcher rules

Build `cargo build --release --offline --example token_matcher_benchmark`, then run `target/release/examples/token_matcher_benchmark 10000 1000 indexed 5`. The arguments are the document token count, the number of rules (one pattern each), the pattern mix, the number of timed matching calls, and an optional vocabulary size (default 200). Patterns and documents draw words from the vocabulary; documents use at most its first 9,973 words. The `indexed` mix uses patterns whose first item requires an exact `Text` or `Lower` value (one-token, two-token and `In` with `OneOrMore` patterns); `unindexed` uses patterns that start with `NotIn`, `Optional`, `Negated` or `ZeroOrMore` items, which are tried at every token; `mixed` alternates the two. The program prints one JSON object: `add_ms` is the registration time; `find_ms`, `find_min_ms` and `find_max_ms` are the median, minimum and maximum of the timed calls after one warmup call (the median of an even count is the mean of the two middle calls); `remove_ms` is the time to remove every tenth rule; `matches` and `matches_after_removal` count the results; `vocabulary` repeats the vocabulary size; and `fingerprint` and `fingerprint_after_removal` are 64-bit FNV-1a hashes of the ordered matches. Equal fingerprints on the same machine indicate that two builds returned the same ordered matches. Wrap the process with `command time -l` on macOS for its peak memory.

To evaluate a change to the matcher, copy the release binary built from the earlier revision before editing, build the new one, and run both alternately on the same arguments over several rounds. Matching time grows with the document length multiplied by the number of patterns that can start at each token, so cover small and large documents, pattern counts, vocabulary sizes and each mix. Workloads with millions of matches also measure output allocation, and their peak memory varies with the system allocator's reuse of freed memory between calls.

The following results compare the revision before first-token indexing, which started every pattern at every token, with the indexed matcher described in the [Token Matcher guide](TOKEN_MATCHER.md#scope-and-cost), including its faster rule removal. They were recorded on an Apple M3 Max (14 cores, 96 GB RAM, macOS 27.0.1) with rustc 1.93.0 and the workspace release profile (optimized, with line debug information), in three alternating rounds; the matching times come from commit ef55c57, whose matching code is unchanged in later commits, and the registration and removal times below come from commit 687eb00. Each value is the median of the per-round medians, with the range of those medians in parentheses. Matching used five timed calls, or three when tokens multiplied by rules exceeded ten million. Every workload produced equal fingerprints for both builds.

| Mix | Tokens | Rules | Vocabulary | Matches | Before (ms) | After (ms) |
|---|---:|---:|---:|---:|---:|---:|
| indexed | 1,000 | 100 | 200 | 498 | 1.53 (1.49–1.65) | 0.13 (0.12–0.13) |
| indexed | 1,000 | 10,000 | 200 | 49,900 | 168 (168–169) | 7.6 (7.5–7.6) |
| indexed | 10,000 | 1,000 | 200 | 50,010 | 173 (173–174) | 7.7 (7.7–7.8) |
| indexed | 100,000 | 100 | 200 | 50,015 | 152 (150–152) | 12.1 (12.1–12.2) |
| indexed | 100,000 | 10,000 | 200 | 5,000,500 | 17,129 (17,071–17,544) | 881 (849–986) |
| indexed | 10,000 | 10,000 | 10,000 | 10,002 | 1,408 (1,408–1,428) | 2.6 (2.6–2.6) |
| indexed | 1,000 | 100,000 | 100,000 | 998 | 1,887 (1,884–1,923) | 4.3 (4.2–4.3) |
| mixed | 10,000 | 1,000 | 200 | 50,197 | 288 (287–290) | 235 (232–239) |
| mixed | 100,000 | 1,000 | 200 | 501,960 | 2,926 (2,901–2,949) | 2,371 (2,363–2,387) |
| unindexed | 1,000 | 100 | 200 | 502 | 3.60 (3.54–3.62) | 3.55 (3.51–3.58) |
| unindexed | 10,000 | 100 | 200 | 5,035 | 35.0 (34.8–36.3) | 35.5 (35.5–35.9) |
| unindexed | 10,000 | 10,000 | 200 | 503,900 | 3,883 (3,841–3,979) | 3,837 (3,827–3,974) |

With the 200-word vocabulary every document token had indexed patterns, and indexed patterns matched 12 to 22 times faster. With 10,000 or 100,000 distinct values, most patterns named words absent from the document, and matching was several hundred times faster. Mixed workloads were 1.23 to 1.29 times faster, because their unindexed half still runs at every token. Unindexed workloads ranged from 0.99 to 1.02 times the earlier speed. In ten further alternating rounds of 21 calls on the 100-rule unindexed cases, the median paired change was a 0.7% slowdown for both the 1,000-token and the 10,000-token document, with individual rounds between 2.9% slower and 3.1% faster, and between 2.5% slower and 1.5% faster, respectively. The new build was faster in 3 of 10 rounds for each, so a slowdown below 1% is possible.

Registration of indexed rules took 4% to 26% longer: 3.7 instead of 3.5 ms for 10,000 rules over 200 values, and 44 instead of 36 ms for 100,000 rules with distinct values. Unindexed registration was unchanged. In nine alternating rounds, removing every tenth rule took 36 ms instead of 79 ms for 10,000 indexed rules, 50 ms instead of 78 ms when those rules had 10,000 distinct values, and 31 ms instead of 80 ms for 10,000 unindexed rules; in three rounds with 100,000 distinct values it took 8.9 s instead of 9.5 s. Removal no longer hashes every later rule name, but it still visits every remaining pattern and every distinct indexed value.

A separate allocation count, made with a temporary counting allocator that is not part of the repository and a 1,000-token document and 10,000 one-token rules over 200 values, found 12.13 MB of live memory before matching instead of 12.46 MB, because rules no longer keep a second copy of their names alongside the name lookup, and the same additional peak during matching to within 0.4 MB. Index storage grows with the number of distinct values and pattern-value pairs. Peak process memory agreed within 7 MB, except for the five-million-match workload, which measured 1.0 GB before and 1.6 GB after. The allocation count found the same live memory after each call in both builds, so that difference is attributed to the system allocator's reuse of freed memory rather than matcher storage.
