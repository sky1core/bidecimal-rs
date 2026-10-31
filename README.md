# bidecimal

Fixed-width IEEE 754-2019 decimal types in pure Rust.

- `Decimal32`, `Decimal64`, and `Decimal128`, stored in exactly 4, 8, and 16
  bytes (BID encoding).
- Every operation returns the exact result, a result rounded in the direction
  you specify, an IEEE 754-2019 infinity or NaN result, or an error. Nothing is
  rounded or overflowed silently.
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

## Operations

Methods without a direction return the exact result or an error; `_rounded`
methods take a `Rounding` (`TowardNegative` or `TowardPositive`) and round
once, and their overflow gives the IEEE 754-2019 result for that direction (an
infinity or the largest finite value). There is no default rounding. Infinity
and NaN operands give the IEEE 754-2019 results, and so does division by
zero: `1 / 0` is an infinity and `0 / 0` is NaN; see
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
| Parsing | `T::parse(s)`, `str::parse`, `T::parse_rounded(s, dir)` | `T` | Without / with |
| Formatting | `Display`; `x.stripped()` removes trailing fractional zeros | Text | None; never rounds |
| Comparison | `==`, `<` and the other comparison operators within and across widths (a NaN is unordered); `total_cmp` (IEEE 754-2019 totalOrder); `is_zero`, `is_positive`, `is_negative` | `bool` / `Option<Ordering>` / `Ordering` | None |
| Keys | `OrderedDecimal(x)`: `Eq`, `Ord`, and `Hash`, with every NaN one key and numerically equal values one key | `OrderedDecimal<T>` | None |
| Classification | `is_nan`, `is_infinite`, `is_finite`, `classify`; constants `NAN`, `INFINITY`, `NEG_INFINITY` | `bool` / `FpCategory` | None |
| Encoding | `x.to_bits()`, `T::from_bits(bits)` | `u32`/`u64`/`u128` / `T` | None |

## Status

Every supported operation is implemented. Correctness evidence combines the
integer arguments, exhaustive Decimal32 checks, independent
arbitrary-precision tests, and the infinity and NaN tests described in
[VERIFICATION.md](docs/VERIFICATION.md).
The optional reciprocal verifier additionally checks a full-domain integer
model using the current production constants. Direct machine verification of
the Rust arithmetic is incomplete; the verified model, its trust boundary, and
the obligations without a direct machine proof are listed separately there.

| SPEC item | Status | Evidence |
|---|---|---|
| Goal 1: exact or explicit | Met | [VERIFICATION.md](docs/VERIFICATION.md): Integer obligations, Class coverage argument, Infinity and NaN |
| Goal 2: proven correct | Met | [VERIFICATION.md](docs/VERIFICATION.md): Integer obligations, Exhaustive and differential evidence, Class coverage argument, Infinity and NaN |
| Goal 3: compact | Met | Compile-time size assertion for each type in `src/types.rs` |
| Goal 4: pure Rust | Met | Empty `[dependencies]` in `Cargo.toml` (dev-dependencies serve tests and benchmarks only); `#![no_std]` and `forbid(unsafe_code)` in `src/lib.rs` for every normal build (it is lifted only under the Verus verification cfg, see `docs/VERIFICATION.md`) |
| Addition and subtraction | Met | [VERIFICATION.md](docs/VERIFICATION.md): Bounded-gap exact sum, Exact addition and subtraction, Compressed addition and subtraction |
| Multiplication | Met | [VERIFICATION.md](docs/VERIFICATION.md): Exact product |
| Division | Met | [VERIFICATION.md](docs/VERIFICATION.md): Division |
| Rounding to a multiple | Met | [VERIFICATION.md](docs/VERIFICATION.md): Rounding to a multiple |
| Quantize | Met | [VERIFICATION.md](docs/VERIFICATION.md): Quantize, Decimal32 exhaustive quantize reference |
| Sign operations | Met | [VERIFICATION.md](docs/VERIFICATION.md): Sign operations |
| Comparison | Met | [VERIFICATION.md](docs/VERIFICATION.md): Numeric comparison, equality and canonical hashing; Infinity and NaN |
| Infinity and NaN | Met | [VERIFICATION.md](docs/VERIFICATION.md): Infinity and NaN |
| Width conversion | Met | [VERIFICATION.md](docs/VERIFICATION.md): Fitting a coefficient; widening and narrowing rows of Exhaustive and differential evidence |
| Parsing | Met | [VERIFICATION.md](docs/VERIFICATION.md): String parsing |
| Formatting | Met | [VERIFICATION.md](docs/VERIFICATION.md): Plain notation formatting, Decimal32 exhaustive format/parse round trip |
| Verification 1: integer routine proofs | Met | [VERIFICATION.md](docs/VERIFICATION.md): Integer obligations |
| Verification 2: exhaustive Decimal32 enumeration | Met | [VERIFICATION.md](docs/VERIFICATION.md): Exhaustive and differential evidence, Decimal32 exhaustive format/parse round trip, Decimal32 exhaustive quantize reference |
| Verification 3: differential testing | Met | [VERIFICATION.md](docs/VERIFICATION.md): Scope and domains, Exhaustive and differential evidence |
| Verification 4: class enumeration and coverage argument | Met | [VERIFICATION.md](docs/VERIFICATION.md): Class coverage argument, Infinity and NaN |

## Benchmarks

`benches/finite.rs`, `benches/keys.rs`, `benches/special.rs`, and
`benches/ops.rs` measure bidecimal's own operations. Before timing, the
finite, keys, and ops benchmarks check their inputs and results against an
exact reference, down to the sign, coefficient, and exponent of each decimal,
and the special benchmark checks the comparisons and result
classes of operations with an infinity or NaN operand against `f64`.
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
machine: it builds both with the benchmark code of one commit, runs every
Criterion target, or those given with `--bench`, for each side in alternating
order, and reports each benchmark's median time and range per side and the
candidate/base ratio, the median over rounds of the ratio of the two runs of
each round taken on a log scale, largest change first. A ratio within the
spread of an A/A comparison, the same commit on both sides, on the same
machine is noise. With `--bench instructions`, it counts the instructions of
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

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
