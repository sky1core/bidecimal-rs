# Comparison with `rust_decimal`

This package compares bidecimal with `rust_decimal` on the operations both
crates provide. It is separate from the bidecimal crate, which neither depends
on `rust_decimal` nor builds or tests anything here; the comparison is a
reference measurement and not part of the specification.

## Method

| Item | Value |
|---|---|
| Library | The crate's `src/` in the working tree, as a path dependency; the report records the commit and whether the working tree is clean; no benchmark-specific code |
| Toolchain | Exact release from the crate's `rust-toolchain.toml`; the runner checks that the `cargo` and `rustc` on PATH report it and builds with that release |
| Dependencies | rust_decimal =1.43.0 (default features), criterion =0.8.2 (no default features, cargo_bench_support), num-bigint =0.5.1 |
| Profile | bench/release: opt-level=3, codegen-units=1, lto=fat, debug=false; no target-cpu override |
| Sessions | 3 interleaved; one paired function per comparison path; each iteration times the full bidecimal set and full rust_decimal set back to back with separate timers; every sample (iter_custom call) starts from the session's configured order and alternates the order per iteration; sessions 1 and 3 start with bidecimal, session 2 with rust_decimal |
| Sampling | Every paired and wide_* function: Flat sampling, 2 s warm-up, 5 s measurement target, 100 samples; criterion selects a constant iteration count per sample |
| Statistics | Per side and sample: aggregated side nanoseconds / iterations / set size (ns per element); median, minimum, nearest-rank p95 (95th of 100); no outlier removal |
| Distribution | Each side sample is the mean time per element over repeated full sets; paired samples cover adjacent timing windows; all 100 samples have equal statistical weight; these are batch means, not individual-operation latencies |
| Cross-check | Paired: raw iteration counts equal criterion sample.json counts sample by sample, and the side time sum equals the criterion sample time exactly (criterion records the returned duration as integer nanoseconds). wide_*: raw statistics recomputed independently and median compared with median.point_estimate / set size |
| Raw paired samples | raw/<path>.json: iterations, bidecimal_ns, rust_decimal_ns arrays; final 100 iter_custom calls, excluding warm-up, for pinned criterion 0.8.2; test mode records its single call; criterion receives the sum of side times |
| Diagnostics | Median of sample-by-sample ratios; fraction bidecimal <= baseline; signed half-median change, second-half median / first-half median - 1 for each side's series and (second-half median - first-half median) / first-half median for the sample-by-sample ratios; per-side slow fractions and their absolute difference; sample ratio IQR / median, with nearest-rank Q1 and Q3 (25th and 75th of 100). Only the sample ratio half-change enters the stability rule; all other fields are diagnostic only |
| Slow sample | Side time per element >= 1.5 times that side's minimum in the same path-session; fixed minimum-relative threshold, not a fitted state classifier |
| Stability / rerun | A path-session is unstable if and only if the half-change abs((second-half median sample ratio - first-half median sample ratio) / first-half median sample ratio), computed in binary64 floating point, is >= 0.1; first 50 vs last 50 samples in acquisition order; each sample ratio is the bidecimal time per element divided by the rust_decimal time per element of the same sample. A session is unstable if any comparison path is unstable. The session table's rerun cause lists a start load >= 1.0 and the unstable paths; either is a recorded cause for rerunning all three sessions; no automatic rerun |
| Ratio | bidecimal median / rust_decimal median |
| Load wait | Before each session, poll the 1-minute load average every 5 s until it is below 1.0, for at most `--max-wait-seconds` (default 120; 0 allowed) |
| Load record | Waited seconds and the 1-minute load at session start and end are recorded; a session starting at load >= 1.0 is preliminary |
| Build environment | The runner refuses to run when any of CARGO, RUSTC, RUSTC_BOOTSTRAP, RUSTC_WORKSPACE_WRAPPER, RUSTC_WRAPPER, RUSTDOC, RUSTDOCFLAGS or RUSTFLAGS is set (even empty), when any CARGO_* variable is set other than CARGO_HOME, CARGO_TARGET_DIR, CARGO_BUILD_TARGET_DIR, CARGO_BUILD_BUILD_DIR, CARGO_LOG, CARGO_CACHE_RUSTC_INFO, CARGO_CACHE_AUTO_CLEAN_FREQUENCY, CARGO_FUTURE_INCOMPAT_REPORT_FREQUENCY and the CARGO_TERM_*, CARGO_HTTP_* and CARGO_NET_* groups, or when `.cargo/config.toml` or `.cargo/config` exists in the working directory or any ancestor directory, or `config.toml` or `config` exists in the Cargo home (CARGO_HOME when set and non-empty, otherwise `.cargo` in the home directory); their absence is recorded. The benchmark build passes no `--target`, so with these checks the recorded Target is the host of the pinned rustc |
| Output | A new output directory per run: preflight output, per-session full output, raw criterion and paired JSON, removed inputs per path, metadata, report; the source fingerprint is compared between sessions |

## Workload

| Item | Generation |
|---|---|
| RNG | SplitMix64, seed 0x243f6a8885a308d3, one stream |
| Size | 8,192 inputs per path, generated once and then partitioned |
| Operand a | Coefficient 1–7 digits, 0–4 fractional digits; the second half has 7-digit coefficients |
| Decimal64 operand b | Coefficient 1–16 digits, 8 fractional digits; every 32nd input is zero |
| Decimal64 pair d, e | Both coefficients 1–16 digits, 8 fractional digits; pairs whose absolute coefficient sum is at most 9,999,999,999,999,999, so add/sub results are exact in Decimal64 |
| Decimal128 operand c | 8 and 12 fractional digits alternating; first half 1–28 coefficient digits, second half 29–34; at most 26 / 22 integer digits |
| Decimal128 multiplication operand | 8 fractional digits; first half 1–21 coefficient digits, second half 23–27; products with operand a have at most 34 digits |
| Sign | Each nonzero value takes a random sign |
| Step | 1 or 5 × 10^k, k = -4..0 |
| Decimal64 comparison | Four-way cycle: b vs identical encoding / b (8 fractional digits) vs a (0–4 fractional digits) / a, given one fractional digit if it has none, vs the same value with one more trailing zero / b vs d |
| Decimal128 comparison | c vs d |

| Partition and checks | Definition |
|---|---|
| Inside value | Equals C × 10^E for an integer C of at most 28 digits and E in -28..=0 |
| Equivalent test | With trailing zeros removed to c', e': zero is inside; otherwise e' >= -28 and digits(c') + max(e', 0) <= 28 |
| Inside input | Every operand and the independently computed result are inside |
| Expected results | num-bigint integers, aligned to the smaller exponent for add/sub and the coefficient product at the sum of the exponents for mul; multiple: floor/ceiling of the exact quotient times the step; quantize: floor/ceiling to a multiple of 0.01 |
| Parse, format, comparison | Parse/format: the value itself; comparison: both operands |
| Preflight | Every bidecimal result is checked first against the expected result and the run stops on any mismatch or error; numeric results are read through `to_bits()` and an independent BID decoder, formatting through its actual output; then inside rust_decimal results are compared numerically |
| Removal | Only inputs whose rust_decimal result differs from the expected result leave the comparison set; the count is summarized and every removed input is written to `removed-inputs/<path>.txt` |
| Outside | Measured for bidecimal alone |
| Timed region | Iteration over prepared inputs and the operation; inputs and outputs through `black_box`; preparation, conversion, BigInt, checks, and error mapping excluded |
| Formatting | Both sides write with `write!` into the same 128-byte stack writer with identical setup |

## Paths

| Path | bidecimal | rust_decimal |
|---|---|---|
| parse32 / parse64 / parse128 | Decimal32/64/128::parse | Decimal::from_str |
| format32 / format64 / format128 | Display into the stack writer | Display into the same stack writer |
| add64 / sub64 | Decimal64::add / sub | checked_add / checked_sub |
| add_mixed / sub_mixed | Decimal128::add / sub of Decimal64 and Decimal128 | checked_add / checked_sub |
| mul64x64 | Decimal128::mul of two Decimal64 operands (a, b) | checked_mul |
| mul32x128 | Decimal128::mul of a Decimal32 and a Decimal128 operand | checked_mul |
| multiple64_floor / multiple64_ceil; multiple32_floor / multiple32_ceil | Decimal64 / Decimal32 round_to_multiple | checked_div, floor / ceil, checked_mul |
| quantize_floor / quantize_ceil | Decimal128::quantize(-2, direction) | round_dp_with_strategy(2, ToNegativeInfinity / ToPositiveInfinity) |
| cmp64 / cmp128 | partial_cmp, mapping Less/Equal/Greater to -1/0/1 and unordered to 2 | cmp as i8 |
| eq64 / eq128 | == | == |
| wide_* | The outside set of the path | — |

## Reproduction

From this directory with the toolchain and dependencies cached, into a new output directory:

```sh
channel=$(sed -n 's/^channel = "\(.*\)"/\1/p' ../rust-toolchain.toml)
rustup run "$channel" cargo test --offline --locked --test perf_harness
mkdir -p target
rustup run "$channel" rustc --edition 2024 --test perf.rs -o target/perf-self-tests
target/perf-self-tests
rustup run "$channel" cargo bench --offline --locked --bench paths -- --test
rustup run "$channel" rustc --edition 2024 perf.rs -o target/perf-runner
target/perf-runner target/perf-run --max-wait-seconds 120
```

The `cargo` and `rustc` first on PATH must be the pinned release.

| Self-test | Coverage |
|---|---|
| perf_harness | The partition predicate against an enumeration of E = -28..=0 on 27/28/29 digits, E = -29/-28/0/1, trailing zeros, zero, both signs, and 20,000 random values; expected results and inside checks; the BID decoder on boundary encodings of all widths (sign, both combination-field forms, exponent, coefficient); workload digit and cohort coverage |
| perf-self-tests | Paired median/min/nearest-rank p95/ratio and diagnostics on synthetic raw JSON; preliminary boundary; iteration identity, time sum differing from the criterion sample time, invalid samples; lockstep 1.7x half-step without instability; ratio half-steps at exactly ±10%, the adjacent representable values just below and above the boundary; unequal slow fractions with stable ratio without instability; ratio half-change and nearest-rank IQR/median; paired file aggregation and wide_* criterion estimate mismatch; refusal of a toolchain mismatch, of each refused build environment variable and of a Cargo configuration file in the working directory, an ancestor directory or the Cargo home; Cargo configuration probing order, Cargo home resolution and metadata check rows naming every checked variable and file; load wait and timeout, CLI default, fixture cleanup |
| paths --test | criterion test mode; preflight of the comparison and outside sets of every path |
