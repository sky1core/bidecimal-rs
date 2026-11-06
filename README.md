# bidecimal

Fixed-width IEEE 754-2019 decimal types in pure Rust.

- `Decimal32`, `Decimal64`, and `Decimal128`, stored in exactly 4, 8, and 16
  bytes (BID encoding).
- Arithmetic returns the exact result, a result rounded in the direction you
  specify, an IEEE 754-2019 infinity or NaN result, or an error. Explicit
  binary floating-point conversions use a fixed nearest, ties-to-even rule,
  with documented precision and range limits.
- As for `f64`, infinities and quiet and signaling NaNs are values, without
  IEEE 754-2019 status flags, and comparison is the IEEE comparison, under
  which a NaN is unordered. `OrderedDecimal` gives `Eq`, `Ord`, and `Hash` for
  collection keys, as `OrderedFloat` does for `f64`; `total_cmp` is IEEE
  754-2019 totalOrder.

The contract is [docs/SPEC.md](docs/SPEC.md).

## Example

```rust
use bidecimal::{Decimal32, Decimal64, Decimal128, Error, Rounding};

fn main() -> Result<(), Error> {
    let a = Decimal32::parse("1.53")?;
    let b = Decimal64::parse("2.5")?;

    let x = Decimal128::mul(a, b)?;
    let y = x.quantize(-2, Rounding::TowardPositive)?;
    let z = a.round_to_multiple(Decimal32::parse("0.05")?, Rounding::TowardNegative)?;

    assert_eq!(x.to_string(), "3.825");
    assert_eq!(y.to_string(), "3.83");
    assert_eq!(z.to_string(), "1.50");
    Ok(())
}
```

## Features

Enable `serde` for `Serialize` and `Deserialize` on all three decimal types
and `OrderedDecimal<T>`. Serialization uses `Display` strings.
Deserialization accepts strings or exact integers and requires a format
supporting `deserialize_any`. Fractional values must be sent as strings;
floating-point inputs are rejected. Strings preserve the value but do not
retain positive exponents or NaN sign, payload, or signaling state.
The feature keeps `no_std` and does not require allocation.

## Operations

Except for the explicit binary floating-point conversions below, methods
without a direction return the exact result or an error; `_rounded` methods
take a `Rounding` (`TowardNegative` or `TowardPositive`) and round once, and
their overflow gives the IEEE 754-2019 result for that direction (an infinity
or the largest finite value). There is no other default rounding. Decimal
arithmetic gives the IEEE 754-2019 results for infinity and NaN operands and
for division by zero: `1 / 0` is an infinity and `0 / 0` is NaN; see
[docs/SPEC.md](docs/SPEC.md#infinity-and-nan).

| Operation | Method | Result type | Direction argument |
|---|---|---|---|
| Addition | `T::add(a, b)`, `T::add_rounded(a, b, dir)` | `T`, any of the three types; operands of any width | Without / with |
| Subtraction | `T::sub(a, b)`, `T::sub_rounded(a, b, dir)` | `T`, any of the three types; operands of any width | Without / with |
| Multiplication | `T::mul(a, b)`, `T::mul_rounded(a, b, dir)` | `T`, any of the three types; operands of any width | Without / with |
| Division | `T::div(a, b)`, `T::div_rounded(a, b, dir)` | `T`, any of the three types; operands of any width | Without / with |
| Rounding to a multiple | `x.round_to_multiple(step, dir)`; any positive `step` | Same as `x` | Always |
| Quantize | `x.quantize(exp, dir)` | Same as `x` | Always |
| Sign | `-x`, `x.abs()`, `x.copysign(y)`; `is_sign_negative`, `is_sign_positive` | Same as `x` / `bool` | None; always exact |
| Widening | `From` | Wider type | None; always exact (a NaN becomes quiet) |
| Narrowing | `TryFrom`, `Decimal32::from_d64_rounded`, `Decimal32::from_d128_rounded`, `Decimal64::from_d128_rounded` | Narrower type | Without / with |
| Conversion to binary64 | `x.to_f64()` | `f64` | None; nearest, ties to even |
| Conversion to binary32 | `x.to_f32()` | `f32` | None; nearest, ties to even |
| Conversion from binary64 | `T::from_f64(value)` | `T`, returned directly | None; nearest, ties to even; 7 / 15 / 15 digits |
| Conversion from binary32 | `T::from_f32(value)` | `T`, returned directly | None; nearest, ties to even; 6 digits |
| Components | `coefficient()`, `mantissa()` (alias), `exponent()`, `scale()` (= −exponent); finite values only | `Result<i32/i64/i128>` / `Result<i32>` | None |
| Coefficient and exponent | `T::from_coefficient_and_exponent(coefficient, exponent)`, `_rounded` variant, `x.to_coefficient_and_exponent()` | `T` / `(i32/i64/i128, i32)` | Without / with for construction |
| Mantissa and scale | `T::from_mantissa_and_scale(mantissa, scale)`, `T::from_mantissa_and_scale_rounded(mantissa, scale, dir)`, `x.to_mantissa_and_scale()`; value = integer × 10^(−scale) | `T` / `(i32/i64/i128, i32)` | Without / with for construction |
| Fractional trailing-zero removal | `x.trim_fraction_zeros()` | Same as `x` | None; always exact |
| Coefficient trailing-zero removal | `x.strip_trailing_zeros()`; stops at the format's greatest exponent | Same as `x` | None; always exact |
| Parsing | `T::parse(s)`, `str::parse`, `T::parse_rounded(s, dir)` | `T` | Without / with |
| Formatting | `Display`; `x.stripped()` removes trailing fractional zeros | Text | None; never rounds |
| Comparison | `==`, `<` and the other comparison operators within and across widths (a NaN is unordered); `total_cmp` (IEEE 754-2019 totalOrder); `is_zero`, `is_positive`, `is_negative` | `bool` / `Option<Ordering>` / `Ordering` | None |
| Keys | `OrderedDecimal(x)`: `Eq`, `Ord`, and `Hash`, with every NaN one key and numerically equal values one key | `OrderedDecimal<T>` | None |
| Classification | `is_nan`, `is_infinite`, `is_finite`, `classify`; constants `NAN`, `INFINITY`, `NEG_INFINITY` | `bool` / `FpCategory` | None |
| Encoding | `x.to_bits()`, `T::from_bits(bits)` | `u32`/`u64`/`u128` / `T` | None |

The four binary conversion methods are inherent methods on every decimal
width. `to_f64` and `to_f32` round the exact decimal value once to the named
binary format; `to_f32` never goes through f64. `from_f64` rounds the exact
binary value once to `min(P, 15)` significant digits, and `from_f32` to
`min(P, 6)`, then applies `trim_fraction_zeros`. For example,
`Decimal64::from_f64(0.1 + 0.2)` is `0.3`, and `from_f32(0.1)` is `0.1` in
all three widths. These methods return values without `Result`, `Option`,
status flags, or conversion-trait implementations.

Rounded overflow gives signed infinity; underflow gives a subnormal or signed
zero. Binary-to-decimal zeros have exponent zero. NaNs retain their sign,
become quiet, and retain their integer payload only if it fits. Binary
subnormals use the same digit rule and can retain expansion noise:
`Decimal64::from_f64(5e-324)` has value `4.94065645841247E-324`.
See the [binary conversion contract](docs/SPEC.md#binary-floating-point-conversion)
for range, payload, cohort and round-trip limits. Exact parsing, decimal
arithmetic and width conversion are unchanged; serde still rejects floats.

Both trailing-zero methods return a decimal value, unlike the `stripped()`
formatting adapter. For `1500`, `trim_fraction_zeros()` keeps coefficient 1500
and exponent 0; `strip_trailing_zeros()` returns coefficient 15 and exponent 2.
Both still display as `1500`. To serialize without fractional trailing zeros,
serialize `x.trim_fraction_zeros()` with the existing serde implementation.
See the [value-transformation contract](docs/SPEC.md#trailing-zero-value-transformations)
for signed zeros, NaNs, and cohort changes.

## Status

Binary floating-point conversion has completed independent-reference,
fixed-seed and boundary-class tests, exhaustive Decimal32 output checks,
and full binary32 input coverage. Its integer arguments and checked premises,
fixed-version core parser dependency, and verification limits are recorded in
[VERIFICATION.md](docs/VERIFICATION.md#binary-floating-point-conversion).
The existing operations' evidence combines integer arguments, exhaustive
Decimal32 checks, independent arbitrary-precision tests, and special-value
coverage described in the same document. Direct machine verification remains
incomplete and is not implied by these results.
The trailing-zero methods also have a full-domain integer argument, independent
generated tests, and a completed exhaustive check of every canonical
Decimal32 input; the exact scope and recorded results are listed there.
The optional reciprocal verifier additionally checks a full-domain integer
model using the current production constants. Direct machine verification of
the Rust arithmetic is incomplete; the verified model, its trust boundary, and
the obligations without a direct machine proof are listed separately there.

| SPEC item | Status | Evidence |
|---|---|---|
| Goal 1: exact or explicit | Met under the documented contract and verification limits | [VERIFICATION.md](docs/VERIFICATION.md): Integer obligations, Class coverage argument, Infinity and NaN |
| Goal 2: proven correct | Met under the documented contract and verification limits | [VERIFICATION.md](docs/VERIFICATION.md): Integer obligations, Exhaustive and differential evidence, Class coverage argument, Infinity and NaN |
| Goal 3: compact | Met | Compile-time size assertion for each type in `src/types.rs` |
| Goal 4: pure Rust | Met | `#![no_std]` and `forbid(unsafe_code)` in `src/lib.rs` for every normal build (it is lifted only under the Verus verification cfg, see `docs/VERIFICATION.md`), so the library's own code has no FFI. No dependencies by default; every entry in `[dependencies]` is optional and without default features: `serde`, the versioned database adapter crates, and `flate2` and `bytes` for those adapters. With all features, the dependency tree for every target has no `cc`, `bindgen` or `pkg-config` crate; the database drivers' dependencies call operating-system APIs through binding crates such as `libc` and `windows-sys` |
| Addition and subtraction | Met | [VERIFICATION.md](docs/VERIFICATION.md): Bounded-gap exact sum, Exact addition and subtraction, Compressed addition and subtraction |
| Multiplication | Met | [VERIFICATION.md](docs/VERIFICATION.md): Exact product |
| Division | Met | [VERIFICATION.md](docs/VERIFICATION.md): Division |
| Rounding to a multiple | Met | [VERIFICATION.md](docs/VERIFICATION.md): Rounding to a multiple |
| Quantize | Met | [VERIFICATION.md](docs/VERIFICATION.md): Quantize, Decimal32 exhaustive quantize reference |
| Sign operations | Met | [VERIFICATION.md](docs/VERIFICATION.md): Sign operations |
| Trailing-zero value transformations | Met | [VERIFICATION.md](docs/VERIFICATION.md#trailing-zero-value-transformations): contract, integer argument, independent-reference and exhaustive checks |
| Comparison | Met | [VERIFICATION.md](docs/VERIFICATION.md): Numeric comparison, equality and canonical hashing; Infinity and NaN |
| Infinity and NaN | Met | [VERIFICATION.md](docs/VERIFICATION.md): Infinity and NaN; binary conversion signed-special and payload-fit reference tests |
| Binary floating-point conversion | Manual bounds, independent-reference checks and stated exhaustive domains checked | [VERIFICATION.md](docs/VERIFICATION.md#binary-floating-point-conversion): certified integer and Clinger paths, direct limb rounding, exact-text parser contract, independent-reference and exhaustive results |
| Binary conversion NaN mapping | Signed-special and payload-fit checks passed | [VERIFICATION.md](docs/VERIFICATION.md#binary-floating-point-conversion): signed quiet NaNs, payload fit and bit-transport limits |
| Width conversion | Met | [VERIFICATION.md](docs/VERIFICATION.md): Fitting a coefficient; widening and narrowing rows of Exhaustive and differential evidence |
| Parsing | Met | [VERIFICATION.md](docs/VERIFICATION.md): String parsing |
| Formatting | Met | [VERIFICATION.md](docs/VERIFICATION.md): Plain notation formatting, Decimal32 exhaustive format/parse round trip |
| Verification 1: integer routine proofs | Manual full-domain arguments with checked numerical premises; no direct conversion machine proof | [VERIFICATION.md](docs/VERIFICATION.md#checked-integer-argument-premises): finite enumeration of expansion, limb/carry, rounding-index, format and text-capacity bounds; Integer obligations for existing helpers |
| Verification 2: exhaustive Decimal32 enumeration | Recorded full-domain checks passed | [VERIFICATION.md](docs/VERIFICATION.md#binary-floating-point-conversion): all raw Decimal32 patterns, independent rejection checks and both binary destinations; existing decoding, widening, format/parse and quantize records below |
| Verification 3: differential testing | Passed in the recorded domains | [VERIFICATION.md](docs/VERIFICATION.md#independent-reference-and-class-coverage-obligations): independent BigUint rational oracle, 10,000 seeded cases per method/width group and exhaustive Decimal32/binary32 domains |
| Verification 4: class enumeration and coverage argument | Boundary, cohort, signed-special and error-path arguments and tests recorded | [VERIFICATION.md](docs/VERIFICATION.md#independent-reference-and-class-coverage-obligations): class partition, source-range and rounding arguments; existing Class coverage argument and Infinity and NaN sections |

## Benchmarks

`benches/finite.rs`, `benches/keys.rs`, `benches/special.rs`,
`benches/ops.rs`, and `benches/serde.rs` measure bidecimal's own operations.
Before timing, the
finite, keys, and ops benchmarks check their inputs and results against an
exact reference, down to the sign, coefficient, and exponent of each decimal,
and the special benchmark checks the comparisons and result
classes of operations with an infinity or NaN operand against `f64`.
The serde benchmark needs the `serde` feature; it checks each value's
`Display` text, padded text, and `serde_json` string against text written
from the decoded sign, coefficient, and exponent, and that the string reads
back as the same number, then measures `serde_json` serialization and
`Display` with and without padding, from short text to the longest finite
text.
Each Criterion benchmark warms up for 0.5 s, then takes 100 samples over
about 1 s, with 2,000 bootstrap resamples for Criterion's own intervals; the
median that `scripts/bench-ab.py` compares is the sample median and does not
depend on the resample count. The keys benchmark measures `BTreeMap` and `HashSet`
with 1,024 keys; the special benchmark measures the same two containers on a
sequence of 1,024 finite keys and 72 infinities and NaNs, and its operation benchmarks
replace no operand slot, each slot with probability 0.1, or every slot with a
NaN or an infinity.

The primary profile is `bench`: Rust's default release settings, stated
explicitly as opt-level=3, lto=false, codegen-units=16, debug=false. Library
profile settings do not apply to dependent projects. The secondary
`bench-fat-lto` profile inherits `bench` with lto="fat" and codegen-units=1.
Pull request performance judgment uses the default release profile first.

`bench-instructions/benches/instructions.rs` counts the executed instructions
of key hashing, key comparison, Decimal128 multiplication and division
rounded toward negative, and `HashSet` lookups with Valgrind Cachegrind through
[Gungraun](https://github.com/gungraun/gungraun). Its inputs are checked
against the same kind of exact reference in setup, outside the counted code.
It runs on Linux with Valgrind 3.22 or later and `gungraun-runner` of the
version in `Cargo.lock`, and building it needs libclang; Gungraun turns off address space layout
randomization with `setarch -R`, which a container's default seccomp profile
refuses. An instruction count is not a time: it shows which code a change adds
or removes, not how long that code takes. It uses `bench-instructions`, which
inherits `bench` and enables debug information.

`scripts/bench-ab.py` (Python 3.12 or later) compares two commits on one
machine: it builds both with the benchmark code of one commit, runs the
finite, keys, special, and ops Criterion targets, or those given with
`--bench`, for each side in alternating
order, and reports each benchmark's median time and range per side and the
candidate/base ratio, the median over rounds of the ratio of the two runs of
each round taken on a log scale, largest change first. A ratio within the
spread of an A/A comparison, the same commit on both sides, on the same
machine is noise. The serde target runs only when named with `--bench serde`
and is built with `--features serde` on both sides. With `--bench instructions`,
it counts the instructions of
each side once and reports the counts and their ratio. A target whose
benchmark code does not compile against the base library because it uses an
item the library lacks or has with another signature or visibility is reported
as not comparable, and its candidate results are still checked; any other build
failure fails the comparison. Both sides are built with the `rustc` in the
sysroot of `RUSTC`, or else of `rustc`, and no rustc wrapper; Cargo's
`build.rustc` and wrapper settings do not apply, and `CARGO_BUILD_RUSTC` and
the wrapper environment variables are refused. `results.json` also records
each executable's SHA-256 and that compiler's full `-vV` output. With
`--analyze-codegen` (it needs the `llvm-tools` component in that sysroot,
`rustup component add llvm-tools`, and reads x86-64 or AArch64 ELF
executables, whose symbols give each function's size), it also disassembles
both sides' Criterion executables and compares, per benchmark function and
function of bidecimal, the number of instructions, their encoded size, the
calls and jumps, conditional and tail ones included, to the 128-bit division
helpers, including those through a GOT slot on x86-64, the other x86-64
instructions with a rip-relative operand that is a helper's GOT slot or
address, such as a load of the slot for a call through a register, and the
indirect calls and jumps whose target it cannot resolve, through registers and
jump tables included, any of which may reach a helper; a function that
disappears may have been inlined, and the complete and the reduced disassembly
are kept in `codegen/`.

```sh
python3 scripts/bench-ab.py --base main --candidate HEAD --work ../bench-trees --out ../bench-out
python3 scripts/bench-ab.py --profile bench-fat-lto --base main --candidate HEAD --work ../bench-fat-trees --out ../bench-fat-out
```

`--profile` selects the timed profile and defaults to `bench`; the summary
and `results.json` record the profile with its settings as written in the
harness commit's `Cargo.toml`. Combining `--profile bench-fat-lto` with
`--bench instructions` is rejected.

The runner rejects build-profile, compiler-flag and target overrides from the
environment, including empty variables, and from Cargo configuration files in
either build tree (`--work` `base` and `candidate`), its ancestors, or its
Cargo home. This keeps the manifest profile recorded in the report consistent
with the build. Cache, transport, logging, alias and registry settings are
allowed; a configuration file may set only `build.build-dir`,
`build.target-dir`, and the `alias`, `cache`, `credential-alias`,
`future-incompat-report`, `http`, `net`, `registries`, `registry` and `term`
tables. Each build tree gets its own build directory. `RUSTC` remains an
explicit compiler selector; the resolved compiler and its version are recorded.
The checks run before setup and again before every build. Run `python3
scripts/test_bench_ab.py` for the guard regression tests (no compilation or
timing required).

`--filter <regex>` passes a regular expression to Criterion, so that only the
benchmarks whose IDs, such as `keycmp_d64_same/set`, match it are listed and
timed; the summary and
`results.json` record the filter. A selected target with no matching
benchmark fails the comparison, and `--filter` with `--bench instructions` is
rejected. Without `--bench`, every target must match, so a filter usually goes
with the `--bench` targets that contain the benchmarks.

```sh
python3 scripts/bench-ab.py --bench keys --filter '^keycmp_' --base main --candidate HEAD --work ../bench-keycmp-trees --out ../bench-keycmp-out
```

During development, compare only the benchmarks of the changed paths with
`--filter`. For a pull request's performance judgment, run the full
comparison once per platform on the final candidate; a ratio within the
spread of an A/A comparison on the same machine is noise. On some machines
the time of a short benchmark varies between processes more than two rounds
average out; on an Apple M1 Max, the standard deviation of the log time over
six runs of one executable reached 21 percent for a comparison benchmark.
Before judging a benchmark whose ratio is outside the A/A spread, compare it
again with `--filter` and more rounds, such as `--rounds 6`.

For pull requests that change the library or the benchmarks, the benchmark
workflow compares the times and the code of the `ops`, `finite`, `keys`, and
`special` targets and the instruction counts on x86-64 and ARM64 Linux; a
manual run takes the timed targets and profile as inputs (default profile:
`bench`). A slower result or a larger count is reported, not a failed check.

## Database integration (optional)

MySQL DECIMAL is supported through these versioned,
default-off features: `sqlx-0_9-mysql` (SQLx 0.9), `diesel-2-mysql` (Diesel 2.3),
and `mysql_common-0_37` (mysql 28 / mysql_async 0.37). All three decimal widths
have driver traits. SQLx runtime `query().bind(decimal)` and typed row access
work with DECIMAL columns. Diesel exposes Numeric expression/row traits.
mysql_common parameters need explicit fallible conversion:

```rust,ignore
let parameter = mysql::Value::try_from(decimal)?;
connection.exec_drop("INSERT INTO amounts (amount) VALUES (?)", (parameter,))?;
```

Read with the driver's fallible row/value API. `FromValue::from_value` is an
upstream convenience method that may panic; prefer `from_value_opt` or
`from_row_opt`. The adapter itself returns errors. Float/Double inputs are
not accepted. mysql_common Bytes has no SQL metadata, so applications requiring a
DECIMAL-only schema must check the column metadata too.

Client conversions are exact and preserve fractional scale when representable.
They do not prevent server column rounding or preserve negative-zero storage.
In particular, strict MySQL mode does not prohibit fractional rounding.
MySQL permits up to 65 total decimal digits and 30 fractional digits. Values
outside that range and NaN/infinities are rejected before writing. A cohort
whose trailing fractional zeros are the only excess is normalized by the
existing trim_fraction_zeros method in the actual parameter bytes. SQLx advertises string parameters but accepts DECIMAL
results only: an uncast `SELECT ?` string result is intentionally not a decimal
round-trip. SQLx compile-time query macros need their usual custom-type
annotations; runtime query APIs are covered by the integration harness.

PostgreSQL NUMERIC adapters use `sqlx-0_9-postgres`, `diesel-2-postgres`,
or `postgres-types-0_2` (shared by postgres and tokio-postgres). They encode
base-10000 binary NUMERIC directly into the driver buffer and validate every
incoming group before exact fitting. Scalar NUMERIC is supported; arrays and
domains are not added here. SQLx also decodes its NUMERIC text protocol.
Canonical payload-free positive quiet NaN and both infinities are supported;
signed/payload/signaling NaNs are rejected on output. Wire zero is positive,
and representable fractional scale is preserved.

Library adapters require no native client. Diesel applications using
MysqlConnection still require libmysqlclient. The mysql_common adapter selects
flate2's Rust backend as specified for this feature; a downstream application's
feature unification can additionally enable another compression backend.

The shared text codec checks numeric range before formatting and reuses the
existing serde short-value lane and Display Parts, with borrowed input and
fixed coefficient-sized stack scratch. SQLx writes directly into its argument buffer; mysql_common must own
its Value::Bytes Vec and Diesel owns its per-bind Vec. Buffer growth and driver
metadata allocations are not claimed to be zero.

See [database integration tests](db-integration/README.md) for live-server test
commands and environment requirements, and [verification](docs/VERIFICATION.md#database-integration-verification)
for recorded results and limits.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
