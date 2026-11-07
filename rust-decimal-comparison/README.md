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
| Profile | Primary (default): bench, Rust default release settings, opt-level=3, lto=false, codegen-units=16, debug=false. Secondary: bench-fat-lto, inherits bench with lto="fat", codegen-units=1. No target-cpu override; selected only with --profile, recorded in metadata and report |
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
| Build environment | The runner refuses to run when any of CARGO, RUSTC, RUSTC_BOOTSTRAP, RUSTC_WORKSPACE_WRAPPER, RUSTC_WRAPPER, RUSTDOC, RUSTDOCFLAGS or RUSTFLAGS is set (even empty), when any CARGO_* variable is set other than CARGO_BUILD_JOBS, CARGO_HOME, CARGO_TARGET_DIR, CARGO_BUILD_TARGET_DIR, CARGO_BUILD_BUILD_DIR, CARGO_LOG, CARGO_CACHE_RUSTC_INFO, CARGO_CACHE_AUTO_CLEAN_FREQUENCY, CARGO_FUTURE_INCOMPAT_REPORT_FREQUENCY and the CARGO_TERM_*, CARGO_HTTP_*, CARGO_NET_*, CARGO_ALIAS_*, CARGO_CREDENTIAL_ALIAS_*, CARGO_REGISTRY_* and CARGO_REGISTRIES_* groups, or when `.cargo/config.toml` or `.cargo/config` in the working directory or any ancestor directory, or `config.toml` or `config` in the Cargo home (CARGO_HOME when set and non-empty, otherwise `.cargo` in the home directory), sets anything other than `build.build-dir`, `build.target-dir` and the `alias`, `cache`, `credential-alias`, `future-incompat-report`, `http`, `net`, `registries`, `registry` and `term` tables, or has a line the runner does not read: a multi-line string, an array of tables, a quoted key where the setting is decided, or a line that is neither a table header nor a key; these checks are recorded. The benchmark build passes no `--target`, so with these checks the recorded Target is the host of the pinned rustc |
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
| wide_* | The outside set of the path; reported as `not-compared` because it is outside the comparison policy, not as unrepresentable in rust_decimal | — |

## Binary floating-point conversion

These paths use fixed individual fixtures, not the generated arithmetic workload.
Each path measures a batch of 256 copies of one prepared input, with the same
paired timers, order alternation, sinks and sample statistics as above. The
`float_` filter selects all new paths, including `wide_float_*` and the core
parser baseline. The normal runner also includes them and preserves their
`FLOAT_INPUT` and `ACCURACY` records in its report.

| Path (W = 32, 64, 128; F = fixture) | bidecimal | Baseline |
|---|---|---|
| float_to_f64_W_F / float_to_f32_W_F | DecimalW::to_f64 / to_f32 | rust_decimal ToPrimitive::to_f64 / to_f32 |
| float_from_f64_W_F | DecimalW::from_f64 | rust_decimal FromPrimitive::from_f64 |
| float_from_f64_retain_W_F | DecimalW::from_f64 | rust_decimal Decimal::from_f64_retain |
| float_from_f32_W_F | DecimalW::from_f32 | rust_decimal FromPrimitive::from_f32 |
| float_core_parse_f64_W_F/core | — | core str::parse::<f64> on the same `{coefficient}e{exponent}` text that the current to_f64 builds, prepared before timing; parse cost only |
| wide_float_to_* | Decimal input not exactly representable by rust_decimal; bidecimal alone | — |

| Fixtures | Exact decimal text |
|---|---|
| All widths | short01 = 0.1, short15 = 1.5, short12345 = 123.45, integer = 42; tiny = `0.` + 27 zeros + `1`; huge = `1` + 28 zeros; extended32 = `1` + 11 zeros; large_exp = `1` + 80 zeros; subnormal32 = `0.` + 44 zeros + `1` |
| Decimal64 and Decimal128 | digits15 = 1.23456789012345, digits16 = 1.234567890123456; subnormal64 = `0.` + 323 zeros + `5`; subnormal64_max = `0.` + 307 zeros + `2225073858507201` |
| Decimal128 only | digits17 = 1.2345678901234567, digits18 = 1.23456789012345678, digits19 = 1.234567890123456789, digits34 = 1.234567890123456789012345678901234; f32_double_round = 1.000000059604644775390625001; f32_midpoint = 1.000000059604644775390625000000001 |
| range_tiny, by width | plain strings: `0.` followed by 100/397/6175 zeros and `1` |
| range_huge, by width | plain strings: 7/16/34 nines followed by 90/369/6111 zeros |
| Additional binary inputs | subnormal = from_bits(1), power2 = 2^80, separately for f64 and f32 |

A single repeated input lets the branch predictor learn its path. The mixed
paths instead measure 256 inputs drawn independently with a fixed SplitMix64
seed, each with a random sign; draws can repeat:

| Path (W = 32, 64, 128) | Inputs | Baseline |
|---|---|---|
| float_mixed_to_f64_W_short / float_mixed_to_f32_W_short | 1–7 random significant digits, adjusted exponent −8..2 | rust_decimal, built exactly from coefficient and scale |
| wide_float_mixed_to_f64_W_full / wide_float_mixed_to_f32_W_full | all P digits, adjusted exponent −37..38 | — |
| wide_float_mixed_to_f64_W_wide | all P digits, adjusted exponent −307..308, clipped to −95..96 for Decimal32 | — |
| float_mixed_from_f64_W_unit | random f64 bits with biased exponent 1013..1032 | rust_decimal FromPrimitive::from_f64 |
| wide_float_mixed_from_f64_W_normal / wide_float_mixed_from_f32_W_normal | random normal f64 or f32 bits over every biased exponent | — |

Each mixed path's `FLOAT_INPUT ... mixed` record gives its number of distinct
inputs and, for paired paths, how many of the 256 rust_decimal results equal
the exact result. Only paired mixed paths print `ACCURACY`, and it gives only
the resulting exact or inexact status. Records of standalone paths use the
path name without the `wide_` prefix. If rust_decimal's `from_f64` returned
`None` for any unit input, that path would instead be measured standalone as
`wide_float_mixed_from_f64_W_unit` without `ACCURACY`; with the fixed seed all
three unit paths are paired.

All fixed decimal fixture texts are plain strings and parse exactly in the
designated width. The `huge`, `extended32` and `large_exp` bidecimal inputs
use `from_coefficient_and_exponent(1, 28/11/80)` to preserve their original
positive-exponent cohorts. Mixed decimal inputs use that constructor for the
generated signed coefficient and exponent, with a stored-cohort assertion.
Their scientific text is used only by the independent numerical oracle, not
by bidecimal's text parser. Binary inputs are parsed directly as f64/f32 from
the fixed decimal strings, plus the two binary fixtures. The range extremes can become signed zero or infinity in the binary
source; `FLOAT_INPUT` records the actual bits and value, so these are special
value paths rather than finite conversions. Input parsing, text construction,
BID decoding, BigInt calculations and all accuracy checks are outside timers.

Before timing, every bidecimal result must equal an independent oracle:
`to_*` compares bits with a num-bigint rational rounding of the exact decimal
value (neighbor and midpoint comparison with nearest-even, binary32 computed
directly, including subnormal and overflow to infinity), independent of the core
float parser; `from_*` reconstructs the exact binary rational
using num-bigint and applies the SPEC significant-digit limit, one nearest-even
rounding including the minimum exponent, exponent fitting and fractional-zero
trimming. It checks the result's coefficient, exponent and sign through the
independent BID decoder, or the infinity bits. Tests include decimal ties,
carry, signed zero, subnormal, underflow and overflow cases.

For conversions only, rust_decimal inexact results **remain timed**.
`ACCURACY rust_decimal_status=exact|inexact|unavailable` records the result.
`exact` means equality to the correctly rounded binary bits for `to_*`, or
numeric equality to the SPEC-rounded destination value for `from_*` (including
zero sign). `from_f64_retain` deliberately has a different precision policy.
For `to_*`, `unavailable` means the decimal source cannot be represented exactly
in rust_decimal, so only bidecimal is timed and no ratio is reported. For
`from_*`, `unavailable` means rust_decimal returned `None` (the adjacent
`FLOAT_INPUT ... rust_decimal_available=false` record shows it); that call is
still timed and its ratio row is marked **different-policy**, like `inexact`.
In 1.43.0 the trait's default `to_f32` converts through `to_f64`; it is still
the public path measured here. An accuracy note must accompany speed ratios.

A short single-session observation, with two build jobs and only these paths:

```sh
export CARGO_BUILD_JOBS=2
channel=$(sed -n 's/^channel = "\(.*\)"/\1/p' ../rust-toolchain.toml)
mkdir -p target/float-short/raw
PERF_RAW_DIR=target/float-short/raw CRITERION_HOME=target/float-short/criterion \
  rustup run "$channel" cargo bench --locked --offline --bench paths -- \
  float_ --warm-up-time 0.05 --measurement-time 0.1 --sample-size 10 \
  --nresamples 1000 --noplot
```

Use a new output directory each time. Record date, commit, CPU, rustc version
and start/end 1-minute load with the output. Ten sample batch means and one
session are preliminary, especially on aarch64 macOS; they do not satisfy the
three-session protocol. The runner's fixed 100-sample cross-check applies to
normal runs, not this short invocation. The paired raw buffer keeps up to 100 calls; with fewer than 100 samples it
also contains warm-up calls. Select its final `sample.json` sample count and
cross-check iteration identity and summed side times before computing short-run
statistics. Criterion `sample.json` itself contains only measured samples.

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
target/perf-runner target/perf-run --max-wait-seconds 120 --profile bench
# Secondary measurement, in a separate output directory:
target/perf-runner target/perf-fat-lto-run --max-wait-seconds 120 --profile bench-fat-lto
```

The `cargo` and `rustc` first on PATH must be the pinned release.
The runner defaults to `--profile bench`; default release results are primary.
For the secondary preflight, add `--profile bench-fat-lto` to the `cargo bench`
command. Build environment overrides, including `CARGO_PROFILE_*`, remain
refused for either profile.

| Self-test | Coverage |
|---|---|
| perf_harness | The partition predicate against an enumeration of E = -28..=0 on 27/28/29 digits, E = -29/-28/0/1, trailing zeros, zero, both signs, and 20,000 random values; expected results and inside checks; the BID decoder on boundary encodings of all widths (sign, both combination-field forms, exponent, coefficient); workload digit and cohort coverage; binary conversion oracle ties, carry, exponent fitting, sign, range and direct binary32 rounding |
| perf-self-tests | Paired median/min/nearest-rank p95/ratio and diagnostics on synthetic raw JSON; preliminary boundary; iteration identity, time sum differing from the criterion sample time, invalid samples; lockstep 1.7x half-step without instability; ratio half-steps at exactly ±10%, the adjacent representable values just below and above the boundary; unequal slow fractions with stable ratio without instability; ratio half-change and nearest-rank IQR/median; paired file aggregation and wide_* criterion estimate mismatch; refusal of a toolchain mismatch, of each refused build environment variable and of a Cargo configuration file with a build setting in the working directory, an ancestor directory or the Cargo home, acceptance of one with only allowed settings; allowed, refused and unread Cargo configuration lines; Cargo configuration probing order, Cargo home resolution and metadata check rows naming every checked variable and file; load wait and timeout, CLI defaults and profile selection/refusal, fixture cleanup |
| paths --test | criterion test mode; preflight of the comparison and outside sets of every path |
