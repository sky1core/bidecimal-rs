# Correctness evidence

## Scope and domains

This record covers canonical BID storage, finite decoding/encoding, numeric
comparison, OrderedDecimal key hashing, width conversion, addition, and
subtraction for precisions 7, 16, and 34, multiplication and division of any
two widths into any result width, quantize for all three widths, rounding to
a multiple for all three widths, sign operations, parsing, and
allocation-free plain formatting with
optional fractional zero stripping. It does not cover performance.
Test-only reference decoding reconstructs the interchange sign, combination
field, exponent continuation, and coefficient independently of the library's
layout routines. The finite numeric oracle uses arbitrary-precision signed
integers for comparison, exact sums/differences and products, and shared
arbitrary-precision quotient/remainder rounding for conversion,
addition/subtraction, multiplication, rounding to a multiple, and parsing.
The quantize oracle rounds its own arbitrary-precision quotient once at the
target exponent. The division oracle has its own
rounding: it finds the least exponent at which the integer part of the exact
fraction has at most `P` digits and rounds that integer part once. The
independent string parser checks grammar by splitting fields, constructs
arbitrary-precision integers, and uses the shared rounding oracle; expected
formatting uses independently decoded fields and test-only allocated strings.
Neither oracle calls a production kernel or layout routine.

The library admits only canonical finite, infinity and NaN encodings. Finite
storage is constructed from validated bits, finite constants, exact widening,
`fit` output, values accepted by `InRange::checked` on the fast paths of
addition and subtraction, multiplication and parsing, quantize output and the
unit-step multiple result with explicitly checked coefficient and exponent
bounds, the encoded zero at qmin for finite/infinity division, or a
sign operation on stored bits, which changes only the sign bit. Special
constants and result constructors produce canonical special encodings.
After special classification, unpacking a finite value has coefficient
`0 <= c < 10^P` and `qmin <= q <= qmax`. Widening preserves these fields and
embeds both source intervals in the destination intervals.

## Infinity and NaN

`src/operand.rs` seals `is_special`, `class` and finite `unpack` to the three
decimal types. Their implementations in `src/types.rs` classify validated
storage as finite, infinity or NaN. Public arithmetic checks `is_special`
before unpacking; `add_sub_special`, `mul_special` and `div_special` match
the operand classes. Finite pairs reach the finite kernels with
the coefficient and exponent bounds above, except that public division
handles a zero divisor first: zero/zero gives `NAN`, and nonzero/zero gives
XOR-signed infinity. An infinity divided by a finite value gives XOR-signed
infinity; finite/infinity gives XOR-signed zero at the result's `MIN_EXP`.
Opposite effective infinities in addition/subtraction, zero times infinity
and infinity/infinity give positive quiet NaN with payload zero.

`quantize_special` quiets NaNs and maps infinity to `NAN` before target
validation; finite quantize passes in-range fields to its finite
kernel. `multiple_special` propagates the first NaN before any step
error, rejects a negative infinite step, then zero or negative finite steps,
keeps an infinite value with a positive finite step, and maps a positive
infinite step to `NAN`. Its finite complement reaches the finite multiple
kernel. Width conversions use `special_of` before finite unpacking; finite
widening preserves fields and finite narrowing uses `fit`.
`nan` keeps the first NaN's sign and payload if the payload is below
`10^(P-1)` for the result, otherwise replacing the payload with zero; its
encoding is always quiet. Bit decoding and sign operations preserve signaling
NaNs; sign operations change only the sign bit.

Finite kernel overflow is mapped by `overflow`, through `finish` for
multiplication, division, narrowing and parsing, and directly for
addition/subtraction. Without a direction it is `Err(Overflow)`;
toward positive it gives +infinity or `MIN`, and toward negative `MAX` or
-infinity, according to the result sign. Rounding to a multiple reports
overflow as an error. `parse_special` accepts optionally signed,
case-insensitive `inf`, `infinity` and `nan` after finite parsing rejects the
syntax. Public formatting classifies before `display::fmt`: infinities are
`inf`/`-inf`, and NaNs are `NaN` without sign or payload.

Numeric equality and `partial_cmp` classify special operands before finite
comparison, with an identical-bit shortcut that still rejects NaNs. NaNs are
unequal and unordered; infinities bracket finite values. `total_cmp` also
uses class, sign, finite cohort exponent, NaN signaling bit and payload.
`src/ordered.rs` instead orders keys as -infinity < finite < +infinity < NaN,
merging all NaNs and numerically equal finite values, including signed zeros.
Only `OrderedDecimal` provides `Eq`, `Ord` and `Hash`. Finite keys hash the
normalized `(bool,u128,i32)` tuple below; NaNs hash `(false,0,i32::MIN)` and
infinities `(neg,0,i32::MAX)`, disjoint from finite zero's `(false,0,0)`.

The following are all tests in `src/special_tests.rs`. Unless a row names
particular widths, it checks each width separately. The shared sample set
has both signs of quiet NaN (payload 0), signaling NaN (payload 7), infinity,
zeros at 0 and qmin, 1, 2.5, the least positive magnitude, the largest finite
magnitude, and 300. These are class representatives, not all special payloads
or all finite values. Result decoding uses the independent BID classifier
and checks canonical special bits.

| Test | Input domain and checks |
|---|---|
| `special_value_arithmetic_matches_ieee_and_f64` | Sample pairs excluding nonzero finite values that become zero or infinity in f64: exact add/sub/mul/div with a special operand, plus finite division by zero, check result class, zero/nonzero and non-NaN sign against f64, and finite/infinity quotient exponent qmin. Directed add/mul/div with special operands must match their exact forms. |
| `special_arithmetic_every_width_combination` | All 27 operand/operand/result width combinations, sample pairs augmented with negative quiet and positive signaling NaNs at payloads 999999, 1000000 and the source maximum when they fit. For special operands or finite division by zero, exact and both directed add/sub/mul/div match an independent class rule, including NaN sign, first-operand priority, quieting, payload fitting and finite/infinity zero fields. |
| `nan_payloads_propagate_and_signaling_nans_are_quieted` | Negative quiet NaN with maximum payload, positive signaling NaN with payload 5, one, infinities and zero: selected arithmetic, quantize and multiple calls check first-NaN sign/payload and quieting, infinity quantize/multiple results, invalid infinite step, zero step and NaN priority over zero-step error. |
| `directed_overflow_gives_the_ieee_infinity_or_largest_finite_value` | MAX plus one ulp at qmax and its negative counterpart, positive 6*10^(P-1)*10^qmax multiplied by +/-2 or divided by +/-0.5, and parsing +/-1e(qmax+P): exact calls return Overflow and both directions give the specified infinity or largest finite value. |
| `narrowing_overflow_and_specials` | Decimal128 1e400 narrowed to Decimal64 checks exact overflow; both signs check all directed endpoints. Selected conversions check both infinity signs, a negative signaling Decimal128 NaN with payload 123456789 fitting Decimal64 but not Decimal32, and signaling Decimal32 payload 999999 widened to both widths; selected mixed arithmetic checks infinity cancellation and finite/negative-infinity zero fields. |
| `from_bits_accepts_exactly_canonical_specials` | Both signs, both NaN kinds and payloads 0, 1, maximum and first invalid; infinities with zero or selected stray bits and NaNs with selected forbidden bits: acceptance, rejection, decoded NaN fields and NaN bit identity. This is a boundary test, not enumeration of every encoding. |
| `special_value_comparisons_and_classification_match_f64` | Faithful f64 images of sample pairs: equality, inequality, partial comparison and relational predicates; total comparison only for non-NaN, non-cohort pairs. Samples check sign/value tests and categories, with explicit decimal subnormal/normal boundary cases. |
| `cross_width_comparisons_with_specials` | Decimal32 NaN versus Decimal64 one in both orders, infinities versus finite Decimal128 extrema, same/opposite infinity signs across widths, and Decimal128 NaN versus itself: equality and partial-order results. |
| `total_cmp_is_the_ieee_total_order` | Sample values, 200 seeded finite values per width, signed cohorts, extreme zeros and NaNs of both kinds with payloads 0/7/9: all pairs checked against an independent integer/cohort/NaN order, reversal, and adjacent order after sorting. |
| `ordered_decimal_keys_merge_nans_zeros_and_cohorts` | Five NaN variants, five signed zero cohorts, every representable cohort of one and a ten-value order ladder: key equality/order/hash agreement, max, BTreeSet and HashMap insertion/lookup/removal, wrapper size/alignment, formatting and accessors. |
| `special_spellings_parse_like_f64` | 30 fixed signed/case-varied special spellings and malformed strings: acceptance agrees with f64; accepted results check class/sign, quiet zero-payload NaNs, and bit agreement with toward-negative parsing and FromStr. |
| `special_values_format_like_f64` | Parsed +/-NaN and +/-infinity under 17 Display/Debug flag combinations, plus plain/padded stripped output, agree with f64; a negative signaling NaN with payload 3 displays/debugs as `NaN`. |
| `special_constants_and_negation` | Decimal32 NAN/NEG_INFINITY, Decimal64 NAN/INFINITY/NEG_INFINITY/MIN and Decimal128 INFINITY: constant fields, negation, abs and copysign results, three explicit special encodings, and negative-infinity text. |

The Decimal32 exhaustive tests have separate special branches:

| Test | Special branch |
|---|---|
| `decimal32_exhaustive_reference` | Public acceptance and bit identity use an independent canonical-special predicate; the finite decoder rejects specials. Accepted specials check negation, abs, negative sign and NaN/infinity/finite tests, without copysign or the positive sign test. The same checks serve the structured Decimal64/128 and all-exponent bit tests. |
| `decimal32_exhaustive_widening_reference` | Canonical infinities and NaNs widen to independently constructed Decimal64 and Decimal128 encodings, retaining sign/payload and quieting NaNs; rejected patterns are independently non-canonical. |
| `decimal32_exhaustive_format_parse_round_trip` | Canonical specials use plain output only; reparsing checks NaN/infinity classification and infinity sign, with a positive NaN. No NaN payload or signaling-bit assertion is made here. |
| `quantize_decimal32_exhaustive_reference` | At all fifteen targets and both directions, canonical NaNs retain sign/payload with the signaling bit cleared; infinities give positive quiet NaN with payload zero, all checked bitwise. |

The checks below, in other test files, use finite inputs. In `addsub_tests.rs`,
`mul_tests.rs`, `div_tests.rs`, `convert_tests.rs` and `parse_tests.rs`, expected
directed overflow is mapped to infinity or the signed finite extremum;
`test_support.rs` classifies actual finite/special results independently and
checks canonical storage. This result form does not inspect NaN sign, payload
or signaling bit. `div_tests.rs` additionally expects NaN for zero/zero and
XOR-signed infinity for nonzero/zero. `compare_tests.rs` checks finite ordering
and hashing through `OrderedDecimal` and decimal partial comparisons.
`format_tests.rs` states its finite parse expectations in the classified
result form; the rejection set of `parse_tests.rs` contains malformed special
spellings and no valid ones. `decimal128_construction_paths_store_small_form` in
`bits_tests.rs` filters out special results before checking finite small form.

The Kani harnesses in `src/layout_proofs.rs` that check infinities and NaNs are
`public_storage_32_matches_decode`, `public_storage_64_matches_decode`,
`public_storage_128_matches_decode`, `constants_encode_their_fields`,
`widen_32_to_64_and_narrow_back`, `widen_32_to_128_and_narrow_back` and
`widen_64_to_128_and_narrow_back`. Their exact domains and assertions are in
the Kani table below. The public-storage proofs independently characterize
canonical special acceptance and bit preservation, but do not check special
`class` sign/payload fields. Widening compares destination special bits with
an independently calculated encoding (destination sign and combination field,
original payload, no signaling bit), then checks class/sign and both forms
of narrowing back to the quieted source bits. The constant proof checks the
three special constants' exact bits at every width.

The Verus proofs cover only their stated domains; they do not prove special
dispatch, special result construction or the public overflow mapping. Those
rest on source arguments and test coverage, as recorded in the obligations
table, rather than direct machine proofs over every supported input.

## Machine verification and remaining obligations

The integer arguments and differential tests below are evidence of the kinds
required by SPEC. They are not direct machine proofs of every Rust operation.
The direct Kani and Verus proofs below cover only the listed harnesses,
functions and domains. An absence of counterexamples in an unfinished run does
not establish correctness.

### Reciprocal integer model

`verification/reciprocals.py` compiles `verification/export_tables.rs`, which
imports the current production `src/tables.rs`, and checks all 116 exported
power and reciprocal entries using arbitrary-precision Python integers.
No reciprocal constants are copied into the verifier. Z3 4.15.3.0 then proves
114 obligations: `reciprocal_64_k01` through `reciprocal_64_k19`,
`reciprocal_128_k01` through `reciprocal_128_k38`, and a corresponding
`high_half_then_shift_*` obligation for each constant.

For width `w`, the domain is every integer `0 <= x < 2^(w-1)`. For each
nonzero table exponent, independently define the Euclidean reference by
`x=q*d+r`, `q>=0`, and `0<=r<d`, with `d=10^k`. Using the actual exported
multiplier `m` and post-shift, the solver proves

```text
q * 2^(w+post) <= x*m < (q+1) * 2^(w+post)
0 <= x*m < 2^(2*w)
0 <= q*d <= x
x-q*d = r
floor(floor(product/2^w)/2^post) = floor(product/2^(w+post))
```

The quotient reference is not assumed to equal the reciprocal result; that
equality follows from the first two inequalities and the shift identity.
The multiplication is by a fixed exported constant, so these obligations use
linear integer arithmetic instead of a symbolic wide-multiplier circuit.
The exponent partitions are complete: `k=0` is the identity branch;
`1..=19` or `1..=38` use the proved entries; and `k>=20` or `k>=39`
have quotient zero because `x` is smaller than `10^k`. These partitions cover
every `u32` exponent. Only the table and reciprocal integer model are
machine-checked by this optional gate.

Trust boundary: Rust const evaluation exports the actual tables, but the
translation from `carrying_mul(..., 0).1`, unsigned shifts, and bounded
subtraction to the displayed integer equations is reviewed by a person.
The verifier does not check the Rust control flow, generated machine code,
compiler, standard-library primitive implementation, or callers' premises.
It trusts Python integer arithmetic and the pinned Z3 solver. Changes to
the reciprocal branches in `src/kernel.rs` require reviewing this
correspondence again even when every model obligation still passes.
In particular,
this model does not authorize replacing an unproved Rust function with a
stub in a direct Kani proof. It does not prove the general arithmetic or
parsing paths listed below.

Run separately from the normal test gates, with the Rust toolchain selected
by `rust-toolchain.toml`:

```sh
python3 -m venv target/reciprocal-verification/venv
target/reciprocal-verification/venv/bin/python -m pip install -r verification/requirements.txt
target/reciprocal-verification/venv/bin/python verification/reciprocals.py
```

The run fails on a missing or duplicate table entry, a mismatched constant,
a solver counterexample, a timeout, an unknown result, or a solver version
mismatch. Generated executables stay under `target/`; no execution logs or
proof-result files are committed.

### Direct Kani harnesses

The harnesses in `src/layout_proofs.rs`, `src/kernel_proofs.rs` and
`src/parse_proofs.rs` are compiled only under `cfg(kani)`. The current source
passed all 62 harnesses listed by the suite with two parallel jobs, using
Kani 0.68.0 (CBMC 6.11.0, nightly-2026-08-21) on aarch64 macOS.
Each row is a direct proof of the Rust code over the stated domain and nothing
more. The files contain only the harnesses listed in the two tables below.

Running them is an optional gate, separate from the normal test gates. Install
the pinned tools with `verification/kani-setup.sh`, then run

```sh
verification/kani-suite.sh
```

The script verifies every harness in both tables with a 30 minute limit per
harness. It exits with a nonzero status if any listed harness fails, times out
or is missing, and it checks that the number of verified harnesses equals the
length of its list.

The harnesses in this first table use no stub.

| Harness | Domain | Property | Tool |
|---|---|---|---|
| `layout_proofs::decode_32_matches_interchange_format`, `layout_proofs::decode_64_matches_interchange_format`, `layout_proofs::decode_128_matches_interchange_format` | every `u32`, `u64` or `u128` bit pattern | Decoding agrees with an independent reading of the IEEE 754 decimal interchange layout, rejects exactly the non-canonical and special patterns with `InvalidInput`, and re-encodes accepted patterns to the same bits. | Kani 0.68.0 / CBMC 6.11.0 |
| `layout_proofs::encode_32_round_trips_every_in_range_field`, `layout_proofs::encode_64_round_trips_every_in_range_field`, `layout_proofs::encode_128_round_trips_every_in_range_field` | every sign, coefficient `c<10^P` and exponent in `qmin..=qmax` | The checked constructor accepts the fields, and encoding then decoding returns the same fields. | Kani 0.68.0 / CBMC 6.11.0 |
| `layout_proofs::in_range_32_accepts_exactly_the_layout_bounds`, `layout_proofs::in_range_64_accepts_exactly_the_layout_bounds`, `layout_proofs::in_range_128_accepts_exactly_the_layout_bounds` | every sign, `u128` coefficient and `i128` exponent | The checked constructor accepts exactly the interchange-format bounds and keeps the fields unchanged. | Kani 0.68.0 / CBMC 6.11.0 |
| `layout_proofs::public_storage_32_matches_decode`, `layout_proofs::public_storage_64_matches_decode`, `layout_proofs::public_storage_128_matches_decode` | every bit pattern | `from_bits` accepts exactly finite-decodable or independently canonical special patterns and preserves their bits. Finite `class` fields and zero/positive/negative tests agree with decoding. Accepted specials have a non-finite `class`, are neither zero nor finite, and `is_nan` agrees with the combination field. | Kani 0.68.0 / CBMC 6.11.0 |
| `layout_proofs::unpack_canonical_128_matches_decode` | every finite canonical `u128` pattern | The unchecked Decimal128 unpacking agrees with decoding. | Kani 0.68.0 / CBMC 6.11.0 |
| `layout_proofs::constants_encode_their_fields` | `ZERO`, `MAX`, `MIN`, `NAN`, `INFINITY` and `NEG_INFINITY` of each width | Finite constants have the expected fields; special constants have the independently specified canonical bits. | Kani 0.68.0 / CBMC 6.11.0 |
| `layout_proofs::widen_32_to_64_and_narrow_back`, `layout_proofs::widen_32_to_128_and_narrow_back`, `layout_proofs::widen_64_to_128_and_narrow_back` | every accepted bit pattern of the narrower width, both directions | Finite widening preserves decoded fields and narrowing returns the original bits. Special widening matches independently computed destination bits, preserves NaN/infinity classification and sign, and checked and directed narrowing return the original bits with the signaling bit cleared. | Kani 0.68.0 / CBMC 6.11.0 |
| `kernel::proofs::power_tables_are_powers_of_ten` | every table index | `pow10(k)=10^k` for `k<=38` and `pow10_64(k)=10^k` for `k<=19`. | Kani 0.68.0 / CBMC 6.11.0 |
| `kernel::proofs::digits_is_the_decimal_length`, `kernel::proofs::digits_64_is_the_decimal_length` | every `u128` or `u64` | The result is the decimal length, with zero for zero. | Kani 0.68.0 / CBMC 6.11.0 |
| `kernel::proofs::div_rem_pow10_outside_table`, `kernel::proofs::div_rem_pow10_64_outside_table` | every `x` and every `k` with `k=0` or `k>=39` (`k>=20` for 64 bits) | The identity branch and the quotient-zero branch. | Kani 0.68.0 / CBMC 6.11.0 |
| `kernel::proofs::div_rem_pow10_k36`, `kernel::proofs::div_rem_pow10_k37`, `kernel::proofs::div_rem_pow10_k38` | `x<2^127`, the named `k` only | `x=q*10^k+r` and `r<10^k`. | Kani 0.68.0 / CBMC 6.11.0 |
| `kernel::proofs::div_rem_pow10_64_k15` through `kernel::proofs::div_rem_pow10_64_k19` | `x<2^63`, the named `k` only | `x=q*10^k+r` and `r<10^k`. | Kani 0.68.0 / CBMC 6.11.0 |
| `kernel::proofs::strip_trailing_zeros_64_zero`, `kernel::proofs::strip_trailing_zeros_64_z15` | zero; and `unit*10^15` with `unit` not a multiple of ten and `unit<10` | The result is `(0, 0)`, respectively `(unit, 15)`. | Kani 0.68.0 / CBMC 6.11.0 |
| `kernel::proofs::zero_sign_follows_exact_zero_rule` | both operand signs, no direction and both directions | The sign of an exact zero result follows the stated rule. | Kani 0.68.0 / CBMC 6.11.0 |
| `kernel::proofs::eq_bounded_32_matches_general`, `kernel::proofs::cmp_bounded_32_matches_general` | finite canonical Decimal32 operands (`P=7`), every `abs(gap)<=31` | The bounded `eq`/`cmp` agree with `cmp_general`. This covers the whole bounded domain for `P=7`. | Kani 0.68.0 / CBMC 6.11.0 |
| `kernel::proofs::bounded_64_gap_m22_matches_general`, `kernel::proofs::bounded_64_gap_p00_matches_general`, `kernel::proofs::bounded_64_gap_p22_matches_general`, `kernel::proofs::bounded_128_gap_p00_matches_general` | finite canonical operands of the named width, the named gap only | The bounded `eq` and `cmp` agree with `cmp_general`. These gaps do not cover the bounded domain for `P=16` or `P=34`. | Kani 0.68.0 / CBMC 6.11.0 |
| `parse::proofs::zero_lanes_marks_exactly_the_zero_bytes` | every `u64` | The mark is `0x80` exactly in the zero byte lanes. | Kani 0.68.0 / CBMC 6.11.0 |
| `parse::proofs::parse_eight_reads_eight_ascii_digits` | every 8-byte array | The result is the decimal value when all bytes are ASCII digits and `None` otherwise. | Kani 0.68.0 / CBMC 6.11.0 |
| `parse::proofs::load_short_len0` through `parse::proofs::load_short_len8` | every byte slice of length 0 to 8; together these lengths are every length its callers pass | The bytes are loaded little-endian into the low lanes. | Kani 0.68.0 / CBMC 6.11.0 |
| `parse::proofs::point_lane_constant` | the constant | `LANE_POINT` has `.` in every lane. | Kani 0.68.0 / CBMC 6.11.0 |

The harnesses in this second table replace the fallback parsers with a stub
whose body is `kani::assume(false)`, so every execution that reaches a
fallback is cut off. They prove that each result the fast path returns
without calling a fallback is correct. The fallback receives the original
`s` and `dir` unchanged; results for inputs handled by a fallback are outside
the proof.

| Harness | Domain | Property | Stub | Outside the proof | Tool |
|---|---|---|---|---|---|
| `parse::proofs::parse_32_fast_path_len0` through `parse::proofs::parse_32_fast_path_len9` | every ASCII string of length 0 to 9, every direction and no direction; together these are every length the Decimal32 fast path handles before it passes longer input to `parse_chunks` | A fast-path `Ok` has the sign, coefficient and exponent of the plain decimal reading of the string, with a coefficient below `10^7`; a fast-path `InvalidInput` occurs only for an invalid string whose body is at most one byte; no other error is produced. | `parse_general` and `parse_chunks` are replaced by `kani::assume(false)` | non-ASCII bytes; every input that reaches `parse_general` or `parse_chunks`, and those parsers themselves | Kani 0.68.0 / CBMC 6.11.0 |
| `parse::proofs::parse_64_fast_path_len8` | every ASCII string of length 8, every direction and no direction | The same property for `parse_64`, with a coefficient below `10^16`. | `parse_general` and `parse_chunks` are replaced by `kani::assume(false)` | non-ASCII bytes; every other length; every input that reaches `parse_general` or `parse_chunks`, and those parsers themselves | Kani 0.68.0 / CBMC 6.11.0 |

### Direct Verus proofs

Verus specifications are attached to the Rust code with `#[cfg_attr(verus_keep_ghost, ...)]`
attributes. The lemmas they use live in `src/verus_model.rs`, and the trusted specifications
of standard-library methods live in `src/verus_std.rs`; both modules are compiled only under
Verus. A normal build does not set `verus_keep_ghost`, so it compiles none of these
annotations, and the crate keeps `forbid(unsafe_code)` there. Under Verus that lint is
relaxed because the trusted specifications are written as `assume_specification`, which the
lint rejects. The format layout items (the marker types `W32`, `W64` and `W128`, `Layout`,
the sealed `Width` trait and its implementations) are plain Rust inside a small wrapper
macro in `src/layout.rs`, which passes them through `verus!` under the verification cfg and
emits them unchanged otherwise, because Verus reads trait associated constants only inside
`verus!`.

Every item below was verified with Verus 0.2026.10.18.7d82994 (bundled Rust 1.99.1 and Z3
4.16.0) on aarch64 macOS; the rows from `div_rem_pow10` on were also verified on x86_64
Linux. Each row is a direct proof of the Rust code over the stated domain, relative to the trusted assumptions listed after the table. Each
specification is stated over mathematical integers; apart from the
specifications of the table constants and of the builders that fill them, no
specification calls an implementation function or reads a table. Every function is also proved free of arithmetic
overflow, underflow, division by zero, out-of-bounds indexing and panics, and every
recursion and loop is proved to terminate.

The representation rows state the IEEE 754-2019 BID interchange encoding (3.5.2) over
integers. A format has `k=e+t+4` bits, with `e` exponent bits and `t` trailing significand
bits (`e=8, 10, 14` and `t=20, 50, 110`); field `(low, len)` of an encoding `b` is
`floor(b/2^low) mod 2^len`. The sign is field `(k-1, 1)`. The encoding is infinity or NaN
when field `(k-6, 5)` is 30 or 31, and finite otherwise. When field `(k-3, 2)` is 3, the
coefficient is `2^(t+3)` plus field `(0, t+1)` and the biased exponent is field `(t+1, e)`;
otherwise the coefficient is field `(0, t+3)` and the biased exponent is field `(t+3, e)`.
The exponent is the biased exponent minus the bias. In these finite
representation specifications, canonical means finite with a coefficient
below `10^P`. Because a finite canonical encoding has exactly one such field
reading, a result that is finite canonical and has the fields of a value is
the encoding of that value.

`InRange<W>` carries a Verus type invariant: `W` is one of the three formats, and the stored
value has `c<10^P` and `qmin<=q<=qmax`. Verus checks the invariant at every construction;
`checked` and `unchecked` are the only constructors.

`fit` is specified over the magnitude `X`: `c*10^q` when the sticky flag is clear, and
`(10c+5)*10^(q-1)` when it is set. A set sticky flag stands for a discarded nonzero tail,
so the true magnitude lies strictly between `c*10^q` and `(c+1)*10^q`. With `c>=10^(P-1)`,
no value with a coefficient below `10^P` and an exponent at least `qmin` lies strictly
inside that interval (`verus_model::lemma_gap`). Every comparison with such a value, and
so each property in the `fit` row, is therefore the same for the true magnitude as for `X`.

The domain column is each function's precondition (`requires`). Each proof assumes it;
Verus checks it at every call inside the verified functions, and at the other production
callers it is unproven. The obligations table below lists this as an open obligation.

| Function or constant | Domain | Proved property |
|---|---|---|
| `layout::decode_32`, `decode_64`, `decode_128` | every encoding | `Ok` exactly for canonical encodings, with the encoding's sign, coefficient and exponent, which are in range; otherwise `Err(InvalidInput)`, so infinity, NaN and non-canonical encodings are rejected. |
| `layout::unpack_32`, `unpack_64`, `unpack_128` | every encoding | The encoding's sign, coefficient and exponent fields, read by the definition above, including for non-canonical encodings. |
| `layout::encode_32`, `encode_64`, `encode_128` | every `InRange` value | The result is finite canonical and has the value's sign, coefficient and exponent, so it is the encoding of the value. |
| `layout::unpack_canonical_128` | every finite canonical Decimal128 encoding | The encoding's sign, coefficient and exponent. |
| `layout::InRange::checked` | `W` is one of the three formats; every sign, `u128` coefficient and `i128` exponent | `Some` exactly when `c<10^P` and `qmin<=q<=qmax`, holding the given sign, coefficient and exponent. |
| `layout::InRange::unchecked` | `W` is one of the three formats; `c<10^P` and `qmin<=q<=qmax` | Holds the given value. |
| `layout::InRange::unpacked` | every `InRange` value | The held value, with `c<10^P` and `qmin<=q<=qmax`. |
| `kernel::canonical` | every `c<2^127` and exponent `q<=2^31-39` | Zero gives `(+, 0, 0)`. Otherwise the result `(s, c', q')` has the input sign, `q'>=q`, `c=c'*10^(q'-q)` and `c'` not a multiple of ten: the value with its trailing decimal zeros removed, whose exponent `q'` may exceed the format's `qmax`. |
| `kernel::is_largest_finite` | `W` is one of the three formats | True exactly when `q=qmax` and `c=10^P-1`. |
| `kernel::exceeds_max` | `W` is one of the three formats; `c<=10^P-1` | True exactly when `q=qmax` and either `c=10^P-1`, or `d<=38` and `t>(10^P-1-c)*10^d`. |
| `kernel::fit` | `W` is one of the three formats; `c<2^127`; every `i64` exponent `q` and preference, with `pref<=q` when `c>0`; `c>=10^(P-1)` when sticky; no direction and both directions | `Ok(r)` has the input sign. With no direction, `r=X`. With a direction, `r` is the least value `>=X` when the direction rounds the magnitude up (toward positive for a positive input, toward negative for a negative one), and otherwise the greatest value `<=X`, among all values with a coefficient below `10^P` and an exponent at least `qmin`. When `r=X`, its exponent is the one in its cohort closest to `clamp(pref, qmin, qmax)`; otherwise it is the least possible exponent. `Err(Overflow)` only when `X>=10^P*10^qmax` for a direction that rounds the magnitude down, and otherwise only when `X>(10^P-1)*10^qmax`. `Err(Inexact)` only with no direction, `X<=(10^P-1)*10^qmax`, and no in-range value equal to `X`. No other error is returned. |
| `kernel::add_sub`, `kernel::add_sub_general` | `W` is one of the three formats; both operands in the Decimal128 range (`c<10^34`, `-6176<=q<=6111`), which contains every finite canonical operand of every width; addition and subtraction; no direction and both directions | Let `S` be the exact sum or difference as an integer at the smaller operand exponent `F`. The result has the properties of the `fit` row for `X=\|S\|*10^F` and preference `F`, with the sign of `S`; an exact zero `S` is negative for two negative effective signs, positive for two positive ones, and otherwise negative exactly toward negative (IEEE 754-2019 6.3). |
| `kernel::div_rem_scaled` | `a<10^34`, `0<b<10^34`, every `u32 k` with `a*10^k<b*10^35` | It returns `(q, r)` with `q=floor(a*10^k/b)` and `r=(a*10^k) mod b`, by long division in chunks of at most `38-digits(b)` digits. |
| `kernel::div_rem_pow10` | every `x<2^127` and every `u32 k` | It returns `(q, r)` with `x=q*10^k+r` and `r<10^k`. This covers the identity branch `k=0`, the quotient-zero branch `k>=39` and every reciprocal branch `1<=k<=38`. |
| `kernel::div_rem_pow10_64` | every `x<2^63` and every `u32 k` | The same property, with the quotient-zero branch `k>=20` and the reciprocal branches `1<=k<=19`. |
| `kernel::digits`, `kernel::digits_64` | every `u128` or `u64` | The result `n` is the decimal length: `x<10^n`, and `10^(n-1)<=x` when `n>0`; zero has length 0. The result is at most 39, respectively 20. |
| `kernel::strip_trailing_zeros` | every `c<2^127`, including zero | Zero gives `(0, 0)`. Otherwise the result `(s, z)` has `c=s*10^z` and `s` not a multiple of ten, so `z` is the largest such count; `z<=38`. |
| `kernel::strip_trailing_zeros_64` | every `c<10^16`, including zero | The same property, with `z<=15`. |
| `kernel::zero_sign` | both operand signs, no direction and both directions | The sign of an exact zero sum: the common sign when the operand signs agree, and otherwise negative exactly when the direction is toward negative (IEEE 754-2019 6.3). |
| `kernel::scaled_rem` | `d<113`, `0<c_s<10^34`, `c_x<10^34` | The result is `(c_x*10^d) mod c_s`, on both the direct and the stepwise path. |
| `kernel::scaled_is_multiple` | `0<c_s<10^34`, every `u128 c_x` | For every `d>=113`, the result is true exactly when `c_x*10^d` is a multiple of `c_s`. The proof writes `c_s=2^a*5^b*o` with `o` coprime to ten, shows `a<113` and `b<113`, and shows that `o` divides `c_x*10^d` exactly when it divides `c_x`. |
| `tables::pow10`, `tables::pow10_64` | every `k<=38`, respectively `k<=19` | The result is `10^k`. |
| `tables::recip` | every `1<=k<=38` | With `d=10^k`, the entry has `post<128`, `2^post<=d<2^(post+1)`, and `m*d=2^(128+post)+e` with `0<=e<d`. |
| `tables::recip_64` | every `1<=k<=19` | With `d=10^k`, the entry has `post<64`, `2^post<=d<2^(post+1)`, and `m*d=2^(64+post)+e` with `0<=e<d`. |
| `POW10`, `POW10_64`, `RECIP`, `RECIP_64` and their builders | every entry | The constants are produced by the builders shown, and each entry has the property stated for its accessor. `m<2^128`, respectively `m<2^64`, follows because `10^k` is a multiple of 5 and therefore not a power of two. The 128-bit reciprocals are computed by binary long division of `2^(127+w)` by `10^k`, where `w` is the bit width of `10^k`. |
| `tables::const_mul_128_to_256` | every `a`, `b` | It returns the low and high 128-bit words of `a*b`. |
| `tables::verify_reciprocals`, `tables::verify_reciprocals_64` | every table entry | The compile-time checks never fail: each check passes for every entry. |

Running them is an optional gate, separate from the normal test gates. The scripts change to
the repository root themselves; from the repository root, run

```sh
verification/verus-setup.sh
verification/verus-suite.sh
```

The setup script supports x86_64 Linux and aarch64 macOS. It downloads the pinned Verus
release, checks its SHA-256 digest, installs it under `target/verus/<version>/` in the
repository, installs the Rust toolchain that this Verus release requires, and checks the
reported version. The suite script needs Python 3 and uses only that installed binary, which
it runs twice from the repository root:

```sh
verus src/lib.rs --crate-type=lib --crate-name bidecimal --edition 2024 --num-threads 2 --output-json --log-dir <temporary directory> --log vir-simple
verus src/lib.rs --crate-type=lib --crate-name bidecimal --edition 2024 --num-threads 2 --no-cheating
```

`verification/verus_check.py` then checks what Verus itself reports, so the checks do not
depend on how the source is written:

- The first run must succeed for the whole crate with the number of verified items recorded
  in the script, no error and no warning, and Verus must report the pinned version.
- The crate functions that Verus verified must be exactly the list recorded in the script.
  A function marked `external` is skipped, not trusted: verified code cannot call it, and it
  drops out of this list.
- In the logged VIR (the intermediate representation that Verus verifies), every trusted
  specification of the crate must come from `src/verus_std.rs` and target a `core` or `std`
  function as resolved by the compiler, and the targets must be exactly the list recorded in
  the script. The crate may declare no other trusted item: no type or trait specification and
  no type marked `external_body`. No function and no loop may set `assume_termination`,
  `exec_assume_termination` or `exec_allows_no_decreases_clause`.
- In the second run Verus rejects every `assume`, `admit`, function marked `external_body` and
  `assume_specification` in the crate. The run must exit with status 1, the rejected items
  must be exactly the trusted specifications found in the VIR, the last line must be the
  compiler's abort line, and the run may report nothing else.

The script exits with a nonzero status if the Verus version differs or any of these checks
fails.

Trusted assumptions. These are not proved and are not counted as verified.

Specifications of standard-library methods that `vstd` does not specify, all stated with
`assume_specification` in `src/verus_std.rs`. Each restates the documented behavior of the
method in Rust 1.99.1:

| Method | Specification | Documented behavior |
|---|---|---|
| `u64::carrying_mul(a, b, c)` | returns `(lo, hi)` with `hi=floor((a*b+c)/2^64)` and `lo=(a*b+c) mod 2^64` | "Calculates the 'full multiplication' `self * rhs + carry` without the possibility to overflow. This returns the low-order (wrapping) bits and the high-order (overflow) bits of the result as two separate values, in that order." |
| `u128::carrying_mul(a, b, c)` | the same with `2^128` | The same text for `u128`. |
| `u64::bit_width(x)` | `r<=64`; `r=0` for `x=0`; otherwise `2^(r-1)<=x<2^r` | "Returns the minimum number of bits required to represent `self`. This method returns zero if `self` is zero." |
| `u128::bit_width(x)` | the same with `r<=128` | The same text for `u128`. |
| `<u32 as From<bool>>::from(b)` | `1` for `true`, `0` for `false` | "Converts from `bool` to `u32`, by turning `false` into `0` and `true` into `1`." |
| `i128::unsigned_abs(x)` | `r=abs(x)`, including `r=2^127` for `i128::MIN` | "Computes the absolute value of `self` without any wrapping or panicking." |
| `u128::trailing_zeros(x)` | `r=128` for `x=0`; otherwise `r<128`, `x` is a multiple of `2^r` and `x/2^r` is odd | "Returns the number of trailing zeros in the binary representation of `self`." The documented examples include `0u128.trailing_zeros() == 128`. |

Other trusted assumptions:

- The specifications that Verus and its standard library (`vstd`) give to Rust integer
  operations and to standard-library methods used here, including `is_multiple_of`,
  `min`, `max`, `clamp`, `saturating_sub`, `checked_mul`, integer `From` conversions,
  shifts, bitwise operations, integer casts and `Option` equality.
- Under the verification cfg only, `Rounding` derives Verus's `Structural` marker, so that
  Verus reads `==` on `Rounding` and `Option<Rounding>` as structural equality. This relies
  on Verus's premise that the derived `PartialEq` of a field-less enum compares variants.
  The normal build is unchanged.
- Under the verification cfg only, the format layout items are passed through `verus!`.
  This relies on `verus!` leaving plain Rust items unchanged; the normal build compiles the
  same items without it.
- The Verus verifier, including its interpreter for `by (compute_only)` steps, the Z3
  solver and the Rust compiler. A normal build compiles the same source without the
  annotations; the proof does not cover the generated machine code.

Not covered by Verus: the remaining `kernel` helpers, including `mul`,
`div`, `quantize`, the multiple rounding paths, `cmp_mag`, `eq`, `cmp` and their general
paths; the parsers and formatter; and the public types' methods and conversions, including
the widening `From` implementations. Their evidence is listed elsewhere in this document.

### Obligations without a direct machine proof

The following families have no direct machine proof of the
Rust code over their whole domain. Where a part of the domain is proved by a
harness above, the row says so; the rest of the row stays unproven. The
retained evidence is not a substitute for a proof.

| Family and exact required domain | What is unproven and why it is hard | Retained evidence |
|---|---|---|
| Special-value dispatch and results: all canonical operands, width combinations, modes, quantize targets, multiple steps and parsing/formatting inputs | Public class dispatch, NaN selection/quieting/payload fitting, invalid-operation results, zero-divisor results, directed overflow mapping, total comparison and OrderedDecimal key semantics have no full-domain direct proof. Kani covers special storage, constants and widening/narrowing back only; the finite Verus proofs assume their finite preconditions. | The Infinity and NaN source argument, every test in `src/special_tests.rs`, special branches of the four Decimal32 exhaustive tests, and the independent finite differentials. |
| Caller preconditions of the representation rows: every call from production code that Verus does not verify of `InRange::checked`, `InRange::unchecked`, `unpack_canonical_128`, `canonical`, `is_largest_finite` and `exceeds_max` | Unproven: `checked`, `unchecked`, `is_largest_finite` and `exceeds_max` need `W` to be one of the three formats; `unchecked` also needs the value in range; `unpack_canonical_128` needs a finite canonical encoding; `canonical` needs `c<2^127` and `q<=2^31-39`; `exceeds_max` needs `c<=10^P-1`. The callers (the public types' constants and methods, `mul`, `quantize`, the multiple rounding paths and the parsers `parse_32`, `parse_64`, `parse_chunks` and `parse_general`) are not verified. | `Width` is sealed and implemented only by the three formats; finite storage comes from validated finite bits or `encode`; special storage is classified before finite extraction; the value-range argument recorded at each caller; the Kani harnesses and tests that exercise those callers. |
| Caller preconditions of the Verus-proved helpers: every call from production code that Verus does not verify of `div_rem_pow10`, `div_rem_pow10_64`, `div_rem_scaled`, `strip_trailing_zeros`, `strip_trailing_zeros_64`, `scaled_rem`, `scaled_is_multiple`, `fit` and `add_sub` | Unproven: each caller must establish the helper's precondition (`requires`), and the Verus proofs above assume it; for `fit` this is the format, `c<2^127`, `pref<=q` for nonzero `c` and `c>=10^(P-1)` when sticky; for `add_sub` it is the format and both operands in the Decimal128 range; for `div_rem_scaled` it is `a<10^34`, `0<b<10^34` and `a*10^k<b*10^35`. The hard point is that the callers (the public types' addition and subtraction methods, `mul_general`, `div`, `quantize`, `round_to_multiple`, `round_to_unit_multiple_64`, `round_to_multiple_general`, `parse_general` and `narrow`) are not verified. | The value-range argument recorded at each caller (the `clippy::arithmetic_side_effects` reason), the Kani harnesses and tests that exercise those callers, and the caller arguments below. |
| Bounded `eq`/`cmp`: finite canonical operands, `P=max(source precisions)`, coefficients below `10^P`, `abs(gap)<=38-P` | Numeric correctness of equality and ordering for every precision and gap. Agreement with `cmp_general` is proved for `P=7` over every gap, for `P=16` at gaps -22, 0 and 22, and for `P=34` at gap 0; agreement does not establish numeric correctness, because `cmp_general` itself has no direct proof. The hard point is symbolic wide multiplication of the scaled coefficient; a proof for individual gaps cannot stand for all gaps or widths. | The signed/unsigned alignment bounds below, `comparison_alignment_boundaries_all_width_pairs`, and `comparison_extreme_exponents_all_width_pairs`. |
| `eq_general`/`cmp_general`: finite canonical operands in the complementary dispatch domain `abs(gap)>38-P` | Numeric correctness of equality and ordering in this domain. `cmp_general` is also the reference that the bounded harnesses compare against, and its numeric correctness has no direct proof on any domain. The hard points are symbolic wide multiplication and the exponent-dependent paths. `eq_general` is not specified for gap zero; a standalone harness must preserve its caller's domain. | The general comparison argument below and `comparison_exponent_gap_digit_guard_and_sign_classes`, plus the cross-width alignment/extreme-exponent tests. |
| `parse_64`: all accepted and rejected input bytes reaching its 8..17-byte significand paths, both signs and every direction | The complete two-word parser for significands of 9 or more bytes, results produced by the fallback parsers, and non-ASCII input. Only the fast-path results for ASCII strings of length 8 are proved, in the stubbed table above. The hard point is the combined lane conversion and wide coefficient accumulation; a proof of `parse_eight` alone is insufficient. | The SWAR and bounded-scan arguments below, `parse_short_significands_match_independent_reference`, `parse_short_significands_match_general_parser`, and `parse_invalid_bytes_at_each_chunk_position`. |
| `parse_32`, `parse_chunks` and `parse_general`: all accepted and rejected input strings that reach each parser, both signs and every direction | Every result of `parse_chunks` and `parse_general`, which also receive every input that the fast paths of `parse_32` and `parse_64` pass on, and non-ASCII input. Only the `parse_32` fast-path results for ASCII strings of length 0 to 9 are proved, in the stubbed table above. | The parsing arguments below, `parse_grammar_shapes_and_rejected_classes`, `parse_coefficient_exponent_sign_and_range_classes`, `parse_unbounded_digits_saturation_and_scale_clamps`, `parse_long_nonzero_significands_in_range`, the grammar fuzzers and `decimal32_exhaustive_format_parse_round_trip`. |
| `mul` fast path: all finite canonical operand width pairs, each result width and every direction | The operand swap, the exact product from two `u64` carrying multiplications, and the `high=0` and range checks that choose between returning that product and calling `mul_general`. | The exact-product argument below, `mul_exact_product_capacity_widening_and_exponent_boundaries`, `mul_all_width_pairs_coefficient_exponent_sign_classes`, and independent BigInt products. |
| `mul_general`: all finite canonical operand width pairs, each result width and every direction | Full limb-product reconstruction and its connection to sticky reduction and `fit`. The hard points are symbolic wide multiplication and the following division. | The exact-product argument below, `mul_limb_boundaries_operand_order_and_every_reachable_shift`, `mul_all_width_pairs_coefficient_exponent_sign_classes`, and independent BigInt products. |
| `div`: all finite canonical operand width pairs, each result width and every direction | The scale `k`, the quotient bounds `10^(P-1)<=Q<10^35` that establish the preconditions of `div_rem_scaled` and `fit`, and the preferred exponent passed to `fit` after stripping. The scaled long division itself is proved by Verus above. The hard point is the digit-length argument over arbitrary coefficient pairs and the cohort argument for the preference. | The division argument below, `div_scale_paths_terminating_quotients_and_precision_carry`, `div_all_width_pairs_coefficient_exponent_sign_classes`, and independent BigInt fractions. |
| `quantize`: finite canonical value of each width, every `i32` target exponent and both directions | The target range check, the digit guard and the product when scaling up, the increment after the reciprocal division when scaling down, and the zero path. The division `div_rem_pow10` itself is proved by Verus above. | The quantize argument below, `quantize_all_width_coefficient_exponent_target_sign_classes`, `quantize_zero_all_targets_invalid_targets_and_precision_edges`, `quantize_decimal32_exhaustive_reference`, and the BigInt quantize differentials. |
| `round_to_multiple_general`: finite canonical value and positive finite canonical step of the same width, both directions | General division and remainder, representability, and overflow classification, including agreement of the unit-step path with the general path and the use of the Verus-proved overflow helpers `is_largest_finite` and `exceeds_max`. The scaled remainder and divisibility helpers themselves are proved by Verus above. The hard point is symbolic wide division by an arbitrary step; a unit-step or selected-exponent proof does not cover arbitrary positive steps. | The multiple-rounding argument below, `multiple_coefficient_exponent_sign_and_step_cohort_classes`, boundary/error tests, and independent rational floor/ceiling followed by exact fitting. |
| `display::fmt` (the finite branch of public formatting): every finite canonical value of each width, every width, fill, alignment and flag, and every writer | The digits, decimal point and zeros of the plain and stripped strings, the padding, and the propagation of every writer error. | The plain notation formatting argument below, `format_all_coefficient_exponent_sign_classes_and_extreme_lengths`, `format_flags_precision_ignored_and_debug_equals_display`, `format_writer_errors_propagate_on_direct_and_padded_paths`, the format/parse differentials and `decimal32_exhaustive_format_parse_round_trip`. |
| Negation, `abs`, `copysign`, `is_sign_negative`, `is_sign_positive` of each public type, and `Hash` of `OrderedDecimal`: every canonical value of each width | That negation, `abs` and `copysign` change only the sign bit and the two sign tests read it, and that `Hash` gives equal values, including cohort members, both zero signs and values of different widths, the same hasher input. Parts are proved above: finite `class` fields by the public storage harnesses and finite `canonical` by Verus; their composition is not machine-checked. | The sign operations and canonical hashing arguments below, the sign checks in `decimal32_exhaustive_reference`, `decimal64_structured_splitmix64_reference`, `decimal128_structured_splitmix64_reference` and `all_exponents_signs_and_coefficient_boundaries`, `decimal128_construction_paths_store_small_form`, and `cohort_hashes_cross_width_and_zero_signs`. |

A direct-code result in this document names the actual harness, its exact
input domain and checked property, the pinned tool version, and the
reproduction command. A domain split into parts needs an argument that the
union of the parts is the original domain, and a stubbed helper needs a
separate direct proof of the replaced helper on every input the caller can
pass. Successful samples and model-level lemmas are kept separate from
direct-code results.

## Integer obligations

### Decimal digit count

Domain: every `u128`, including zero (whose digit count is zero). For bit width
`b`, put `t = floor(1233*b/4096)`. For each of the 129 possible widths,
`digits_matches_native_at_every_bit_width_endpoint` checks both endpoints
against integer logarithms, establishing `t <= digits(x) <= t+1` on that whole
interval by monotonicity. Also `t <= 38`. The only possible decimal transition
is therefore `10^t`, so `t + [x >= 10^t]` is exact. The power-table index is
bounded by 38; the product `128*1233` fits `u32`.
`digits_matches_native_at_every_decimal_boundary` checks every power boundary.

### Division by a power of ten

Domain: `0 <= x < 2^127`, every `u32` power `k`. Power zero is identity;
`k >= 39` gives quotient zero and remainder `x`, since `x < 10^39`.
For `1 <= k <= 38`, let `d=10^k`, `L=bit_width(d)`, `s=127+L`,
`m=ceil(2^s/d)`, and `e=m*d-2^s`. Compile-time table construction and verification
establish `m < 2^128`, `0 <= e < d`, and `post=L-1 <= 126`, by reconstructing
`m*d` with exact 64-bit limb products. Since `d` is not a power of two,
`2^(L-1) < d < 2^L`. Then

```text
0 <= x*e/(d*2^s) < 1/d
x*m/2^s = x/d + x*e/(d*2^s)
```

The fractional part of `x/d` is at most `(d-1)/d`, hence the integer quotient
is unchanged. Taking the high half of the exact 256-bit product and shifting
by `L-1` computes this quotient. Thus `q*d <= x`, and the product and remainder
subtraction cannot overflow or underflow.
`div_rem_matches_native_at_boundaries_and_multiples` checks every `k` at
endpoints and the neighbors of the first 100,000 multiples of `10^k` that lie
below `2^127`;
`div_rem_matches_native_for_fixed_seed_random_inputs` adds one million inputs
per power. `div_rem_handles_zero_and_large_exponents` checks the other branches.
The optional `reciprocal_128_k01` through `reciprocal_128_k38` and
`high_half_then_shift_128_*` obligations machine-check the integer model
with production constants; their trust boundary is stated above.

### Removing trailing zeros

Domain: `0 <= c < 2^127`. Zero returns `(0,0)`. A nonzero input has at most
38 trailing zeros. Greedy exact division at powers `32,16,8,4,2,1` recovers the
binary expansion of that count: after each decision, the remaining count is
smaller than the power just considered. Each division decreases the input,
preserving the division premise. The total removed count is at most 38.
`strip_trailing_zeros_matches_naive_for_every_power` checks all possible counts
and coefficients on decimal and integer boundaries.

### 64-bit digit count, division and stripping

Digit count: domain every `u64`. For bit width `b<=64`, `t=floor(1233*b/4096)<=19`.
`digits_64_matches_native_at_every_bit_width_endpoint_and_decimal_boundary`
checks both endpoints of all 65 widths against integer logarithms, so
`t<=digits(x)<=t+1` on each width interval; the only possible transition is
`10^t`, and `t+[x>=10^t]` is exact. The 20-entry power table holds
`10^0..10^19<2^64`; its index is at most 19. The same test checks every power
boundary.

Division: domain `0<=x<2^63`, every `u32` power `k`. Power zero is identity;
`k>=20` gives quotient zero and remainder `x`, since `x<2^63<10^20`. For
`1<=k<=19`, let `d=10^k`, `L=bit_width(d)<=64`, `s=63+L`, `m=ceil(2^s/d)`,
and `e=m*d-2^s`. Compile-time table construction and verification establish
`m<2^64`, `0<=e<d`, and `post=L-1<=63` from the exact 128-bit product `m*d`.
Since `d` is not a power of two, `2^(L-1)<d<2^L`, so `x*e<2^63*2^L=2^s` and

```text
0 <= x*e/(d*2^s) < 1/d
x*m/2^s = x/d + x*e/(d*2^s)
```

The fractional part of `x/d` is at most `(d-1)/d`, hence the integer quotient
is unchanged. The high word of the 128-bit product `x*m` is
`floor(x*m/2^64)`; shifting it by `post` gives `floor(x*m/2^s)`. Thus
`q*d<=x`, and the remainder subtraction cannot underflow.
`div_rem_64_matches_native_at_boundaries_and_multiples` checks every `k` at
endpoints, `2*10^16-1`, and the neighbors of the first 100,000 multiples of
`10^k` that lie below `2^63`;
`div_rem_64_matches_native_for_fixed_seed_random_inputs` adds one million
inputs per power; `div_rem_64_handles_zero_and_large_exponents` checks the
other branches. The optional `reciprocal_64_k01` through
`reciprocal_64_k19` and `high_half_then_shift_64_*` obligations check the
integer model with current production constants under the trust boundary
stated above.

Stripping: domain `0<=c<10^16`. Zero returns `(0,0)`. A nonzero input has at
most 15 trailing zeros. Greedy exact division by the constants `10^8,10^4,10^2,10`
recovers the binary expansion of that count; each accepted division
decreases the input, and native division by a nonzero constant is exact.
`strip_trailing_zeros_64_matches_naive_for_every_count` checks every count
0..15 with boundary, fixed and 1,000 random units against repeated division by
ten.

### Fitting a coefficient

Domain: `c < 2^127`, an `i64` exponent and preferred exponent, a supported
layout, and either no rounding direction or one of the two directed modes.
The represented magnitude is `(c+f)*10^e`, where `f=0` without sticky and
`0 < f < 1` with sticky. Sticky requires `c >= 10^(P-1)` for the result
precision. Width conversion always supplies `f=0`.

Let `n=digits(c)` and `k=max(n-P,0,qmin-e)`. Exponent arithmetic uses `i128`,
so subtraction of any two `i64` values and addition of at most 32 digit shifts
cannot overflow. The new exponent `e+k` is at least `qmin`. Division uses
`min(k,39)`, which preserves quotient and remainder for every larger `k`.
Its input is the original bounded coefficient. The quotient has at most `P`
digits. The discarded part is nonzero exactly when remainder or sticky is
nonzero: the sticky premise prevents a hidden fraction from being an exact
result at a finer representable exponent.

For exact zero, clamp the preferred exponent to the target interval and keep
the sign. For exact nonzero, multiply by at most
`10^min(e'-clamp(pref), P-digits(q))` when this shift is positive. This preserves
the value, keeps the coefficient below `10^P`, and selects the closest
representable exponent to the preference, for the producer premise
`pref <= e` (arbitrary preferences are also admitted for zero). Clamp both
ends before shifting, so an exact subnormal cannot move below `qmin`.
Insufficient digit headroom to reach `qmax` means overflow.

For an inexact input, digit reduction gives a full-precision quotient unless
underflow fixes the exponent at `qmin`. With no direction, `e'>qmax`, or
`e'=qmax` and quotient at least `10^P-1`, means the original magnitude exceeds
the maximum: report `Overflow` before `Inexact`. Otherwise report `Inexact`.
With a direction, increase magnitude exactly for positive ceiling or negative
floor. The increment is at most `10^P`; equality becomes
`10^(P-1)` at exponent `e'+1`. Testing the final exponent against `qmax` is
therefore equivalent to testing the directed, unbounded-exponent rounded
value against the finite maximum. The final coefficient is below `10^P` and
the exponent is in range before encoding. Rounding to zero preserves the sign.

`fit_full_integer_domain_boundaries_and_sticky_rational_oracle` checks all
coefficient digit lengths, the upper integer bound, reduction/underflow
boundaries, and three exact rational tails for each sticky case against the
arbitrary-precision oracle. All tails in `(0,1)` have the same remainder-zero
classification and directed result by the inequalities above.
`fit_preferred_exponent_clamps_and_extreme_i64_exponents` checks exponent
extremes, zero preferences, precision headroom, and exact underflow recovery.

### Bounded-gap exact sum

Domain: canonical finite operands whose exponent gap has magnitude at most
four, and whose minimum exponent is inside the result exponent interval.
Decoded exponents are in `[-6176,6111]`, so their signed difference fits
`i32`. Each decoded coefficient is below `10^34`. Convert each coefficient
exactly to `i128` and apply its effective sign (the second sign is flipped
for subtraction). For a nonzero gap, multiply the coefficient at the larger
exponent by `10^abs(gap)`. The power index is at most four, the factor is at
most `10^4`, and the signed sum has magnitude at most
`(10^34-1)*10001 < 2^127`. A compile-time assertion checks this maximum;
monotonicity covers every smaller coefficient and each gap in `0..=4`.
Thus conversion, negation, multiplication, addition, and unsigned absolute
value are exact without overflow. Gap five is outside this unconditional
bound: `(10^34-1)*100001` exceeds even `u128::MAX`.

If the magnitude is below `10^P`, return it at the minimum exponent.
This is the preferred exponent, and neither precision reduction nor
underflow is needed. A negative sum is negative; a zero retains the common
effective sign for two same-sign zeros, and otherwise is negative exactly
toward negative.

For two nonzero operands, an accepted sum has magnitude below `10^34`.
The unscaled coefficient at the minimum exponent is also below `10^34`;
therefore the scaled coefficient is below `2*10^34 < 10^35`.
The general algorithm's exact alignment guard must succeed, and its
magnitude/sign operations give precisely the same signed sum. Its one-zero
and both-zero branches represent the same integer, sign and preference.
Since the accepted result already fits at its preference, `fit` gives the
same fields. If the gap, result exponent range, or coefficient capacity
guard fails, the general path performs the complete rounding, overflow and
cohort handling. This partition covers every supported input.

`addsub_equal_exponent_exact_path_boundaries` and
`addsub_near_exponent_exact_path_boundaries` cross all 27 width combinations
with both operations, all sign pairs, and all three modes. They check zero,
maximum coefficients, exact cancellation, achievable aligned sums
`10^P-1,10^P,10^P+1`, gaps from zero through five in both orders,
both source exponent endpoints, and the result range endpoints and their
first neighbors where the source ranges permit them. Impossible sums and
exponents are excluded by the source bounds. The gap test also checks the
maximum signed-sum bound and rejection of the first excluded power bound
using checked integer multiplication.

### Exact addition and subtraction

Domain: two canonical finite operands, independently chosen widths, any result
width, and all three modes. Decoding establishes coefficients below `10^34`
and exponents in `[-6176,6111]`. Subtraction first flips the second sign.
The preferred exponent is always the minimum of the original exponents.
Both-zero inputs retain a shared effective sign; differing signs produce
negative zero only toward negative. One-zero inputs preserve the nonzero
operand's effective sign and exact coefficient. All zero exponents and
one-zero preferences pass through `fit`.

For nonzero inputs, order exponents `qa >= qb`, set `d=qa-qb` (`0..=12287`),
and `n=digits(A)` (`1..=34`). The guard `n+d <= 37` implies `d <= 36` and
`A*10^d < 10^37`. Thus addition is below `10^37+10^34 < 2^123 < 2^127`.
Magnitude comparison before subtraction prevents unsigned underflow and
selects the correct sign. Equal magnitudes give negative zero only toward
negative. `Wide` is exact at `qb`, with `pref <= qb`, so every result is
rounded only by `fit`. The guarded power index is at most 36.

When the guard fails, trailing-zero stripping receives `B < 10^34 < 2^127`.
Its count is at most 33. Remove only `z=min(zeros,d)`; use the fully stripped
coefficient when possible, otherwise exact reciprocal division by `10^z`
with the same bounded input. This branch has `z=d<zeros<=33`, so no fraction
is discarded. Replacing `B,qb,d` by `B',qb+z,d-z` preserves both the value
and `pref <= qb'`. Retry the same exact guard. Stripping is outside the
common exact path; no 128-bit native division is required.

### Compressed addition and subtraction

Premises: the retry still has `n+d' >= 38`. Since `n<=34`, `d'>=4`.
Had stripping stopped at `d`, the retry would have `d'=0`, a contradiction;
therefore all trailing zeros were removed and `10` does not divide `B'`.
Set `e=35-n` (`1..=34`) and `g=d'-e >= 3`. Reciprocal division receives
`B'<10^34<2^127` and returns `B'=s_hi*10^g+s_lo`, including `g>=39`.
Then `s_hi<10^31` and `0<s_lo<10^g`. Also
`10^34 <= A*10^e < 10^35`; the power index is at most 34.

```text
same sign:     H = A*10^e + s_hi,     f = s_lo/10^g
opposite sign: H = A*10^e - s_hi - 1, f = 1-s_lo/10^g
magnitude = (H+f)*10^(qb'+g),         0 < f < 1
10^34-10^31 <= H < 10^36 < 2^127
```

The aligned dominant magnitude is at least `10^37`, while `B'<10^34`, so
its sign is the result sign. The exact aligned integer has at least 37
significant digits and a nonzero units digit; it cannot be represented by
any supported precision. The bound `H>=10^33>=10^(P-1)` for `P=7,16,34`
establishes the sticky premise. The `fit` exponent `qb'+g` is at least the
unchanged preference. Thus any positive fractional tail gives the same
directed result, error classification, and cohort, without losing an exact
case. Branch guards and the finite digit/count ranges above establish all
multiplication, subtraction, reciprocal-division, and `fit` premises.

`addsub_exact_compression_boundary_and_trailing_zeros` enumerates `n` in
every source precision and every possible trailing-zero count of `B`, with
`n+d=37/38` both before and after stripping, in both operand orders. The
coefficient/gap tests also cover nonunit leading digits and both signs.

### Exact product

Domain: canonical finite operands of any two widths and a result of any width,
exact mode or either direction. Decoding each operand at its own width
establishes `0<=a,b<10^34<2^113` and source exponents in `[-6176,6111]`. The
preferred exponent `pref=qa+qb` lies in `[-12352,12222]`, so its `i64` sum is
exact. The sign is `sa XOR sb` for every input, including zero products.

Fast path. The kernel first swaps the operands so that `a<2^64` whenever
either coefficient is below `2^64`; multiplication commutes, so the swap does
not change the product. If after the swap `a<2^64`, the cast of `a` to `u64`
is exact, and `b<2^113` splits as `b=b1*2^64+b0` with `b0<2^64` and
`b1<2^49`. Two `u64` carrying multiplications compute
`a*b0=low+carry*2^64` and `a*b1+carry=middle+high*2^64` exactly, so
`a*b=high*2^128+middle*2^64+low`. The path continues only for `high=0`; then
`middle*2^64+low` joins disjoint limbs into the exact `u128` product. The
checked constructor accepts it only when the product is below `10^P` and
`pref` lies in `[qmin,qmax]` of the result width; that value is the exact
product at its preferred exponent, which is the result `fit` returns for an
exact coefficient already at its preferred exponent. Every other input,
including `a>=2^64` after the swap, `high!=0`, a product of more than `P` digits,
and an out-of-range `pref`, reaches the general path, which computes the same
product from the original coefficients.

General path. A zero coefficient returns signed zero at `pref` through `fit`.
Otherwise split each coefficient at `10^17`: `a=a1*10^17+a0`,
`b=b1*10^17+b0`, every limb below `10^17<2^57`. Then
`p00=a0*b0<10^34`, `p11=a1*b1<10^34` and `p01=a0*b1+a1*b0<2*10^34`. Euclidean
division gives `p00=c0*10^17+l0` and `p01+c0=c1*10^17+l1` with
`p01+c0<2*10^34+10^17<2^127`. Put `H=p11+c1` and `R=l1*10^17+l0<10^34`.
Then `a*b=H*10^34+R` exactly, and `H<10^34` because `a*b<10^68`.

- `H=0`: the product is `R<10^34<2^127`, exact at `pref` with no tail.
- `H>0`: let `sh=digits(H)-1`, in `0..=33`. Euclidean division gives
  `R=T*10^sh+S` with `T<10^(34-sh)` and `S<10^sh`. The coefficient
  `H*10^(34-sh)+T` lies in `[10^34,10^35)` because
  `10^sh<=H<10^(sh+1)`, and its exponent is `pref+sh>=pref`. The discarded
  value `S*10^(pref)` is below one unit of that exponent; the sticky flag
  records `S!=0`.

The coefficient is at least `10^34>=10^(P-1)` for every result precision, so
the sticky premise of `fit` holds: any discarded tail lies strictly inside one
grid step of every exponent `fit` can select, and one directed rounding at the
selected exponent is the correctly rounded result. With `S=0` the value is the
exact product, so exact mode returns it or the `Inexact`/`Overflow` error that
`fit` assigns from the exact magnitude. Every power index is at most `34`.

Tests. `mul_exact_product_capacity_widening_and_exponent_boundaries` checks
coefficients at and around `2^64`, at and around the `2^64` carry boundary of
each small factor, at and around `10^34/a` for small factors, high limb zero
and nonzero, and products at and around `2^128`, in both operand orders, for
all nine width pairs, all three result widths and exponent sums at each result
range end. `mul_limb_boundaries_operand_order_and_every_reachable_shift`
crosses limb values `0,1,9,5*10^16,10^17-1` in both limbs of both operands,
each maximum coefficient, `2^64-1/2^64/2^64+1`, and every product digit count
`34..=68`, so every `sh` in `0..=33` and both `H=0` and `H>0` are reached; the
general path is also reached for small products by choosing `pref` outside
the result range. The digit-count and Euclidean proofs above extend these
classes to every coefficient in each interval.

### Division

Domain: canonical finite operands of any two widths and a result of any width,
exact mode or either direction. Decoding each operand at its own width
establishes `0<=a,b<10^34` and source exponents in `[-6176,6111]`. The
preferred exponent `pref=qa-qb` lies in `[-12287,12287]`, so its `i64`
difference is exact. The sign is `sa XOR sb` for every input, including zero
quotients.

Zeros. The public methods test the finite divisor before calling the kernel:
zero divided by zero returns `NAN`, and a nonzero dividend divided by zero
returns the infinity with XOR sign, for either signed zero and every mode.
The finite kernel has its own `InvalidInput`/`DivisionByZero` guards, but
public division does not reach them. A zero dividend with a nonzero divisor
gives zero at `pref` through `fit`, which clamps the exponent to the result
range.

Scale. Otherwise `a,b>0`. Put `da=digits(a)` and `db=digits(b)`, both in
`1..=34`, and `k=max(P+db-da,0)<=P+33<=67`. The exact quotient is
`(a*10^k/b)*10^(pref-k)`; put `Q=floor(a*10^k/b)` and `R=(a*10^k) mod b`.

- `k>0`: `k=P+db-da`. From `10^(da-1)<=a<10^da` and `10^(db-1)<=b<10^db`,
  `10^(P+db-1)<=a*10^k<10^(P+db)`, so `10^(P-1)<a*10^k/b<10^(P+1)`.
- `k=0`: `da>=P+db`, so `a/b>10^(da-1)/10^db>=10^(P-1)` and `a/b<10^34`.

In both cases `10^(P-1)<=Q<10^35<2^127` and `a*10^k<b*10^35`, which with
`a<10^34` and `0<b<10^34` is the precondition of `div_rem_scaled`.

Scaled long division. `div_rem_scaled` starts from `q=floor(a/b)` and
`r=a mod b` and appends `c<=38-db` digits per step, with `4<=38-db<=37`. Each
step keeps `q*b+r=a*10^j` and `r<b`, where `j` counts the digits done:
`r*10^c<10^(db+38-db)=10^38<2^128`, and the new quotient is at most
`floor(a*10^k/b)<10^35`. At `j=k`, Euclidean uniqueness gives `q=Q` and
`r=R`. Verus proves this for every input in the domain of its row above.

Exact quotient. When `R=0`, stripping receives `Q<2^127` and returns
`Q=s*10^z` with `s` not a multiple of ten and `z<=38`, so `s*10^(pref-k+z)`
is the exact quotient, and every exponent stays within `[-12354,12325]`.
Since `s` has no trailing zero, `e=pref-k+z` is the greatest exponent of its
cohort. If `pref>=e`, every cohort exponent is at most `min(e,qmax)`, which is
at most `clamp(e,qmin,qmax)<=clamp(pref,qmin,qmax)`, so the closest exponent to
either clamped preference is the greatest representable one. The kernel
therefore passes `min(pref,e)` as the preference, which satisfies the `fit`
precondition `pref<=e`, and `fit` returns the exact quotient at the exponent
closest to `clamp(pref,qmin,qmax)`, or the `Inexact`/`Overflow` error that it
assigns from the exact magnitude.

Inexact quotient. When `R!=0`, the true magnitude lies strictly between
`Q*10^(pref-k)` and `(Q+1)*10^(pref-k)`, which is the sticky premise of
`fit`; `Q>=10^(P-1)` holds for every result precision. The preference
`min(pref,pref-k)=pref-k` meets the `fit` precondition; an inexact result
takes the least exponent regardless. One directed rounding at the selected
exponent is therefore the correctly rounded quotient, and with no direction
the result is `Inexact`, or `Overflow` when the exact magnitude exceeds the
largest finite value.

Tests. `div_scale_paths_terminating_quotients_and_precision_carry` checks
divisors of every digit length 1..34, so every chunk limit 4..37, with
one-digit dividends for the largest scale `P+33`, 34-digit dividends for
`k=0` at precisions 7 and 16, powers of two through `2^112` and of five
through `5^48` for terminating quotients through and past each precision,
and each precision's maximum coefficient and neighbors for the precision
carry. `div_all_width_pairs_coefficient_exponent_sign_classes` crosses the
coefficient classes of all nine width pairs with exponent differences at the
operand, result, overflow and underflow transitions. The digit-count,
long-division and `fit` proofs extend these classes to every coefficient in
each interval.

### Quantize

Domain: canonical finite input of precision `P=7,16,34`, target `q: i32`,
and an explicit direction. Reject `q` outside `[qmin,qmax]` before arithmetic;
all remaining exponent differences have magnitude at most 12287. Zero keeps
its sign and receives exactly `q`, without computing a power.

For `e>=q` and nonzero `c`, let `s=e-q`. The digit guard
`digits(c)+s<=P` is equivalent to `c*10^s<10^P`. It establishes
`s<=P-1<=33` before the power lookup and multiplication. Failing the guard
returns `InvalidInput`, the IEEE 754-2019 invalid operation, even if another
cohort would represent the value: the quantize result must have exactly
exponent `q`. No branch returns `Overflow`.
For `e<q`, reciprocal division receives `c<10^P<2^127` and `k=q-e>=1`.
It includes gaps above 38, returning `(0,c)` there. A nonzero remainder
increments magnitude only for positive ceiling or negative floor. Since
`floor(c/10^k)<=floor((10^P-1)/10)`, the incremented quotient is strictly
below `10^P`; a precision carry is impossible. The original sign, including
zero, and checked exponent `q` establish the encoder premises directly.
Digit and reciprocal proofs extend these branches to every input; the class
tests enumerate precision headroom, exact/nonzero remainder, both increment
choices, exponent ends, and all out-of-range target categories.

### Sign operations

Negation, `abs`, `copysign` and the sign tests use only the sign bit, the
highest bit of the encoding in every format (IEEE 754-2019 3.5.2). Negation
flips it, `abs` clears it, `copysign` replaces it with the sign bit of the
other operand, and the sign tests read it. The combination field and trailing
significand are copied unchanged; they alone determine finiteness, canonicity,
coefficient and exponent. Every result is therefore the canonical finite
encoding with the operand's coefficient and exponent and the new sign, zeros
included, and a stored Decimal128 keeps the small coefficient form. No
arithmetic is performed.

### Rounding to a multiple

Domain: canonical finite value `x=c_x*10^q_x` and step `s=c_s*10^q_s` of the
same width `P=7,16,34`, and an explicit direction. Test zero before sign, so
either signed zero step returns `DivisionByZero`; any negative nonzero step is
`InvalidInput`. Then `s>0`, and the result is `I*s`, where `I` is the integer
nearest to `x/s` in the requested direction. Because `s>0`, `x/s` has the sign
of `x`; the magnitude of `I` rounds up exactly for positive ceiling and
negative floor and down otherwise. The preferred exponent is `q_s`. A zero
result keeps the sign of `x`. The exact multiple is returned, or `Overflow`
when its magnitude exceeds the largest finite value, or `Inexact` when it is
below that and needs more than `P` significant digits at every exponent.

#### Unit steps for precisions 7 and 16

The kernel takes this path for `P<=16` when the stripped step coefficient is
`u=1` or `u=5`. Decode establishes `c_x,c_s<10^P<=10^16<2^64` and source
exponents within `[-398,369]`; both coefficients convert to `u64` exactly.
64-bit stripping receives `c_s<10^16`; the removed count `z<=P-1<=15` gives
`e_s=q_s+z` in `[-398,384]`.

Put `N=c_x*(2 if u=5 else 1)` and `E=q_x-e_s-[u=5]`. Then
`x/s = N*10^E` exactly. All exponent arithmetic fits `i32`
(`-783<=E<=767`), and `N<2*10^16<2^63`.

- `E>=0`: the quotient is an integer, so the result is `x`;
  keep `(c,e)=(c_x,q_x)`. Here `q_x>=e_s+[u=5]>=q_s`.
- `E<0`: divide `N` by `10^(-E)` with the 64-bit division (`-E>=20` gives
  quotient zero, as `N<10^17`), incrementing the integer quotient `I` only for
  a nonzero remainder and positive ceiling or negative floor. This is the only
  rounding. The result is `u*I*10^e_s`; keep `(c,e)=(u*I,e_s)`. Here
  `e_s>=q_s`.

In the divided branch, `I<=c_x/10+1` for `u=1`; for `u=5`, `5*I<=c_x+4`,
because `I` is a directed endpoint of `2*c_x/10^(-E)` and the largest
remainder at divisor ten is eight. Larger divisors only decrease the upper
bound, and a zero numerator never increments. Thus `c<=c_x+4<10^P+4<2^64`.
The `u=1` branch stays below `10^P`; the only multiple of five in
`[10^P,10^P+4]` is `10^P`. That single carry becomes `(10^(P-1),e+1)`
exactly. Afterwards `c<10^P` and `e>=q_s` in every branch, the result is
exact, and no `Inexact` outcome exists on this path.

- `c=0`: return zero with the sign of `x` at exponent `q_s`, which is in range.
- `c>0` and `e>q_s`: shift by `s=min(e-q_s,P-digits(c))`; `digits(c)<=P`, so
  the saturating difference is exact. Then `c*10^s<10^P` and the exponent
  `max(q_s,e-(P-digits(c)))` is the representable exponent closest to the
  preferred exponent `q_s`.
- If the resulting exponent exceeds `qmax`, the shift was limited by
  precision, so `c>=10^(P-1)` and the magnitude is at least `10^(P+qmax)`,
  above the largest finite value: return `Overflow`.

These are the fields that `fit` produces for an exact coefficient at most
`10^P` with exponent at least the preferred exponent; the kernel computes them
directly in 64-bit arithmetic. The output satisfies the encoding premises
`c<10^P` and `qmin<=e<=qmax`. These bounds use only decoded ranges, the
checked unit, and the sign/exponent guards; the 64-bit digit, division, and
stripping obligations are established above.

#### General steps

Every other step, and every step for precision 34, takes this path. Decode
establishes `c_x,c_s<10^P<=10^34<2^113`, `c_s>0`, and exponents in
`[-6176,6111]`. The path builds an exact coefficient at an exponent no lower
than `q_s`, with no tail, and passes it to `fit` in exact mode with preferred
exponent `q_s`. `fit` then returns the representable value closest to that
exponent, or `Overflow` from the exact magnitude, or `Inexact`.

- `c_x=0`: zero with the sign of `x` at `q_s`.
- `q_x>=q_s`: let `d=q_x-q_s<=12287`. In units of `10^q_s`, `x=M=c_x*10^d`, an
  integer, and `s=c_s`.
  - `d>=113`: `scaled_is_multiple` decides only whether `M` is a multiple of
    `c_s`. Write `c_s=2^i*5^j*o` with `o` coprime to ten. `c_s<2^113` gives
    `i<=112` and `j<=48`, so `2^i*5^j` divides `10^d`, and `M` is a multiple
    of `c_s` exactly when `o` divides `c_x`. A multiple is passed to `fit`
    unchanged, as `c_x` at `q_x`. Otherwise the multiple in either direction is `M+-t` with `t` in
    `(0,c_s)`, whose trailing-zero count `z` is at most 33, so
    `digits(c_x)+d-z>38` and the failed-guard argument below applies without
    computing `t`: the result is `Inexact`, or `Overflow` when rounding up
    and `t>h*10^d`. Since `d>38`, this holds exactly when `h=0` and
    `q_x=qmax`, which `is_largest_finite` tests.
  - `d<113`: `scaled_rem` returns `r=M mod c_s`. If `digits(c_x)+d<=38`,
    `M<10^38<2^128` is computed directly. Otherwise the remainder is reduced
    in steps of at most `10^4`, each product `r*10^k<10^34*10^4` fits
    `u128`; modular reduction commutes with multiplication, so the result is
    exact.

  For `r=0`, `x` is already a multiple and is passed to `fit` unchanged. For
  `r>0`, the magnitude moves by `t=c_s-r` up or `t=r` down, both in
  `(0,c_s)`, so `M+t` or `M-t` is the nearest multiple in the requested
  direction and is nonnegative because `r<=M`. Let `z` be the trailing-zero
  count of `t`, capped at `d`; then the result is
  `(c_x*10^(d-z) +- t/10^z)*10^(q_s+z)` exactly.
  - Guard `digits(c_x)+d-z<=38`: `c_x*10^(d-z)<10^38` and the coefficient is
    below `10^38+10^34<2^127`, an exact `fit` input.
  - Guard fails: then `d-z>38-digits(c_x)>=4`, so `z<d` and `t/10^z` is not a
    multiple of ten while `c_x*10^(d-z)` is. The coefficient has no trailing
    zero and is at least `10^37-10^34>10^34`, so the multiple has more than
    `P` digits at every exponent and is not representable. Rounding down, its
    magnitude is at most `|x|`, so the result is `Inexact`. Rounding up, it
    exceeds the largest finite value `(10^P-1)*10^qmax` only if `q_x=qmax`
    (otherwise `d>=4` keeps `s<10^(P+qmax-4)` and `|x|+s` below it); with
    `h=10^P-1-c_x`, the condition is `t>h*10^d`. `exceeds_max` tests exactly
    this: `h=0` (`is_largest_finite`) exceeds for any `t>0`, and for `h>0`
    the bound `h*10^d>=10^d>t` holds when `d>38` or the product overflows
    `u128`, so only the checked product decides. The result is `Overflow`
    when the test holds and `Inexact` otherwise.
- `q_x<q_s`: let `g=q_s-q_x`. In units of `10^q_x`, `s=S=c_s*10^g`. If
  `digits(c_s)+g<=38`, `S<10^38` and `I=floor(c_x/S)`, incremented for a
  nonzero remainder when rounding up. Otherwise `S>=10^38>c_x`, so the
  quotient is `0` with nonzero remainder and `I=[up]`. In both cases
  `I<=c_x/S+1`, so `I*c_s<=c_x*c_s/S+c_s=c_x/10^g+c_s<2*10^34`, exact at
  exponent `q_s`; `S` may be smaller than `c_x`.

Every result has exponent at least `q_s`, so the `fit` precondition holds; a
zero coefficient keeps the sign of `x` at `q_s`. On a unit step `10^k` or
`5*10^k` the multiple is `x` cut at one digit position, or a carry to a power
of ten, so it never needs more digits than `x`; `Inexact` cannot occur there.

### Numeric comparison, equality and canonical hashing

Comparison and equality read finite Decimal128 fields only after storage
validation and special classification.
The large-coefficient BID form has its implicit bit at position 113, hence
coefficient at least `2^113>10^34-1`. A compile-time assertion checks this
inequality, so no finite canonical Decimal128 can have that form. Its
coefficient is exactly the low 113 bits, exponent the next 14 bits minus bias 6176,
and sign the highest bit. The mask and shift fields are disjoint; the
14-bit exponent narrows exactly to `i32` and bias subtraction fits `i32`.
Finite construction establishes canonical storage before this extraction;
external finite validation uses the general decoder. Special values
are handled before finite extraction. The extraction requires a finite
small-coefficient encoding with `c<10^34`: finite constructors take bits
accepted by the finite decoder or encode fields with `c<10^34` and
`qmin<=q<=qmax`; sign operations change only the sign bit of stored bits.
Decimal32 and Decimal64 comparison handle both finite coefficient forms.
`OrderedDecimal` hashing uses general unpacking for finite values, which
extracts identical fields on this domain.

Domain: two unpacked finite values of any supported widths. Let `P` be
the greater operand precision, one of 7, 16, 34, fixed at each call site.
Both coefficients are below `10^P`. The difference `d=qa-qb` lies in
`[-12287,12287]`, so subtraction and negation fit `i32`. For
`|d|<=38-P` (limits 31, 22, 4 respectively), convert the coefficients
exactly to `i128`, apply each sign, and multiply only the coefficient
with the larger exponent by `10^|d|`. Each signed magnitude is below
`10^P*10^(38-P)=10^38<2^127`; a compile-time assertion checks the
last bound, and the sealed `Width` trait, implemented only by `W32`, `W64`,
and `W128`, restricts P to the three precisions.
Negation and multiplication are exact, zero has integer value zero for
either sign, and the power index is at most 31. Integer comparison is
therefore numeric comparison at the common exponent `min(qa,qb)`.
For finite Decimal32 and Decimal64, identical stored bits give Equal before
unpacking. Identical finite Decimal128 encodings take the zero-gap
integer path, where both signed coefficients are equal.

The complementary domain uses general comparison. Zero is handled
before exponent alignment. Order exponents so `a.exp >= b.exp` and let their
difference be `d`. It fits `i64`. If `digits(a.c)+d > 35`, the scaled nonzero
coefficient is at least `10^35`, larger than `b.c < 10^34`. Otherwise `d <= 34`
and `a.c*10^d < 10^35 < 2^127`, so integer multiplication and comparison are
exact. Swap reversal and negative-sign reversal preserve numeric order.
Signed zeros are equal; every finite pair has a total ordering.

For Decimal32 and Decimal64, identical stored bits compare equal except for
NaNs, which compare unequal and unordered. Special operands are classified
before the finite equality routine, which covers all nine width pairs. It
first uses the same bound `|d|<=38-P`, multiplying unsigned coefficients to a common
exponent. The signed-alignment proof also establishes these unsigned products
below `10^38`. Equality then requires equal aligned coefficients and either
equal signs or a zero aligned coefficient. Both sign tests are boolean
operations without sign-dependent control flow. Positive powers preserve
the zero/nonzero classification. This predicate is necessary and sufficient
for numeric equality throughout the bounded domain.
The complementary domain uses general equality. There `|d|>38-P>=4`, so
the exponents differ, and a zero equals only another zero, independently of
sign and exponent. Nonzero opposite signs cannot be equal.
For same-sign nonzero operands let `d=qa-qb`, in `[-12287,12287]`, so
subtraction and negation fit `i32`. If `1<=d<=33`, equality is exactly
`A*10^d=B`; if `-33<=d<=-1`, it is exactly `B*10^(-d)=A`.
Every power-table index is within `1..=33`. Checked multiplication either
returns the exact product or detects that it exceeds `u128::MAX`, in which
case it cannot equal a coefficient below `10^34`. For `|d|>=34`, the aligned
nonzero coefficient is at least `10^34`, so equality is impossible.
Thus this predicate is equivalent to ordering returning Equal on its entire
domain, for every width combination, cohort and signed zero.

For nonzero finite keys, remove every trailing decimal zero and add its count
to the exponent. The coefficient is below `10^34 < 2^127`; at most 33 additions
to an exponent in `-6176..=6111` fit `i32`. Uniqueness follows because two
integers indivisible by ten cannot differ by a nontrivial power of ten.
A finite zero key is always `(false,0,0)`. All widths of `OrderedDecimal`
hash finite keys as the same tuple of types `(bool,u128,i32)`, so numeric
equality implies identical hash input, including
across widths.

### String parsing

Domain: every UTF-8 string reaching the finite parser. A byte loop accepts
exactly optional sign, one or more ASCII significand digits with at most one point, and optional `e`/`E`
with optional sign and one or more ASCII digits. All bytes must be consumed.
The sign is retained independently of the magnitude, including zero. Public
parsing also accepts the special spellings described in Infinity and NaN.

Skip leading zero digits for accumulation, retaining at most 35 significant
digits. After n retained digits, `0<=c<10^n`; the next multiplication and
addition yield `<10^(n+1)<=10^35<2^127`. Every later digit is discarded;
sticky is true exactly when some discarded digit is nonzero. A discarded
nonzero implies 35 retained significant digits, hence `c>=10^34`, satisfying
`c>=10^(P-1)` for all three precisions. If d digits are discarded, the exact
magnitude is `(c+f)*10^(preferred+d)` with `0<=f<1`; `f>0` iff sticky.
The shared fit proof therefore applies without double rounding.

For P=7 and 1..8 significand bytes after the optional sign, a SWAR word
replaces the byte loop; empty/sign-only input is rejected first. With
L bytes, two overlapping 4-byte little-endian loads (or 1..3 byte loads for
L<4) are ORed; overlapping lanes carry the same byte, so OR is idempotent
and lanes `L..7` are zero. Shifting left by `8*(8-L)` right-aligns the bytes
in the high lanes and the low `8-L` lanes are filled with ASCII `0`; this
prepends leading zeros and changes neither the value nor the fraction count.
Point lanes are found exactly by `zero_lanes(w XOR 0x2e..)`: each masked
lane is at most `0x7f`, adding `0x7f` reaches at most `0xfe`, so no carry
crosses lanes and bit 7 of the result is set exactly for a zero lane.
Compile-time enumeration checks every byte value at every lane against
all-zero and all-one neighbours. Padding lanes are `0`, so only input
points are detected. Two or more points dispatch the original string to the
general parser. A lone point (`L=1`) is rejected. For one point in
lane l (0..7, counted from the least significant lane), the lanes below it
are shifted up one lane, the lanes above are kept and lane 0 becomes `0`.
This deletes the point, prepends one more leading zero, and leaves exactly
`7-l` digits to its right, so the preferred exponent is `l-7` in `[-7,0]`.
Without a point the word is used unchanged with exponent 0. In that case
`trailing_zeros(0)/8` is 8, and the `& 7` mask maps it to lane 0, so every
lane shift stays below 64 bits; the point-deleted word is still computed
but is not selected. All shifts are at most 56 bits and masks are formed
without underflow. The resulting
eight-byte word is validated and converted by the chunk routine below, so
any non-digit byte (ASCII, `e`, sign or UTF-8) dispatches to the general
parser. The path returns directly only for `c<10^7` and an in-range
preferred exponent. Eight digits at or above `10^7` leave this path even
if exactly representable after changing exponent; the complete general path
preserves their required cohort and rounding/error behavior.
`parse_short_significands_match_general_parser` compares this path with the
general parser in every mode for every string over
`0 7 9 . e - / : \u{ae}` with at most eight bytes after each sign; strings
of the unsigned root never start with `-`, so the enumeration contains
61,266,219 distinct inputs. It also covers every string over `9 .` with at
most 22 bytes. The general parser shares `fit` with this path, so it is not
an independent reference. `parse_short_significands_match_independent_reference`
therefore checks a subset against the independent reference in every mode
for every width, and also against the general parser: each sign (none, `+`,
`-`), every length 1..8 (1..17 for P=16), no point or a point at every
position, digit fills `0`, `9` and `1234567890`, and each of `/ : . e E
\u{0} \u{7f} \u{80} \u{ae}` substituted at every position (198,180
inputs). `0xFF` cannot occur in a `&str` and is excluded.
Longer significands use the bounded chunk scan or the general parser.

For P=16 and 8..17 significand bytes after the optional sign, a two-word
SWAR form replaces the byte loop; other lengths use the bounded chunk scan.
With L=17 the first byte becomes the lead byte and the remaining 16 bytes
the body; otherwise the lead is ASCII `0` and the body is all L bytes, so
the body has B=8..16 bytes. Two overlapping 8-byte little-endian loads
take the first and last eight body bytes. The first is shifted left by
`8*(16-B)` (0..64, computed in 128 bits so 64 is defined) and its low
`16-B` lanes are filled with ASCII `0`; this prepends leading zeros and
places the body in the high B lanes of the 16-lane word formed with the last
load as its high half. Point lanes are found exactly by `zero_lanes` on both
halves as for P=7; padding lanes are `0`, so only body points are detected.
Two or more points dispatch to the general parser. For one point in lane l
(0..15), the lanes below it are shifted up one lane, the lanes above are
kept and lane 0 becomes the lead byte. This deletes the point and leaves
exactly `15-l` digits to its right, so the preferred exponent is `l-15`.
Without a body point, a lead `0` (always so for L<17) keeps the word with
exponent 0, a lead point selects the word with exponent -16, and any other
lead byte dispatches to the bounded chunk scan, which returns the same
fields or delegates to the general parser. Then `trailing_zeros(0)/8` is 16,
the `& 15` mask maps it to lane 0 and the point-deleted word is computed but
not selected. All lane shifts are at most 120 bits and masks are formed
without underflow. Every byte of the selected word is validated as an ASCII
digit by the chunk test applied to both halves at once: high nibbles must be
`3` and adding 6 to each low nibble (at most 15) must not reach 16, so no
lane carries into the next. Any other byte, including a lead point moved
into lane 0, `e`, sign or UTF-8, dispatches to the general parser. Each
half is converted by the chunk recurrence below to a value `<10^8`, and the
coefficient `high*10^8+low<10^16=10^P` is exact. Its preferred exponent is
in `[-16,0]`, inside the P=16 range, and `InRange::checked` establishes the
range before the direct return. Every input of this path has at most P
significant digits, so no rounding occurs; the direct fields equal the
general parser's in every mode.

For a significand with at most 35 bytes after its optional sign, a
bounded slice scan accumulates every digit without retaining/discarding
state. After n digit bytes, `0<=c<10^n`, with n<=35. Appending k validated
digits is exactly `c*10^k+v<10^(n+k)<=10^35<2^127`. The compile-time bound
checks this premise and pointer widths <=64. At a point, the remaining
slice length F is at most 34; `-F` fits i32 and equals the preferred exponent
after all following bytes have been validated as digits. Empty input and a
lone point are rejected by `L<=point_count`; a second point, exponent marker,
or other byte dispatches the original string to the general parser.
A direct result requires `c<10^P` and an exponent within the destination
range. Leading zeros do not change the recurrence or the preferred
exponent. The direct-result proof below applies to every field and mode.
Longer inputs and capacity/range failures also take the general parser;
the slice length condition limits an optimization, not accepted syntax.

An eight-byte chunk is read in little-endian order. For each byte b, the
high nibble must be 3. Its low nibble d is initially in `[0,15]`;
adding 6 yields at most 21, so the packed addition cannot carry between
byte lanes or overflow u64. Bit 4 of each sum is zero exactly for d<=9.
Together these tests accept exactly ASCII digits, independently in every
lane, including for non-ASCII bytes. Compile-time enumeration substitutes
all 256 byte values at each of the eight positions and checks acceptance
and the positional value. Lane independence extends this enumeration to
all `256^8` chunks, including multiple invalid bytes.

For accepted digits d0..d7 in textual order, pair lanes are
`10*d(2i)+d(2i+1)<=99` in four 16-bit lanes. Four-digit lanes are
`100*pair(2i)+pair(2i+1)<=9999` in two 32-bit lanes. The final value is
`10000*group0+group1<=99999999`. Lane masks select exactly these components;
all intermediate products and sums stay within their lanes and u64, so
no carry crosses a lane and no value is discarded by a mask. The constant
maximum chunk checks the combined bound and digit order. The returned
value is appended by the bounded recurrence with k=8.

When no full digit chunk remains, fewer than eight bytes remain or the
next eight contain a nondigit. The scalar digit run therefore has length
k<=7 before end or its first nondigit. Its u32 recurrence gives
`tail<10^k` and `factor=10^k<=10^7<u32::MAX`, with that bound checked at
compile time. Combining `c*factor+tail` is the same bounded concatenation.
After a point the scan repeats on the remaining slice; after any other
nondigit it uses the complete general parser. Chunk consumption decreases
the slice by eight; scalar and point consumption use checked slice splits
and decrease it by one. No public-input slice indexing is used.

When no digits were dropped, the retained length is at most P, and
`qmin <= E-F <= qmax`, the parser returns `(sign,c,E-F)` directly.
The recurrence gives `c<10^P`, including c=0. In `fit`, the precision
and underflow shifts are both zero, the remainder and sticky are zero,
and the preferred exponent already equals the working exponent. Thus
neither rounding direction changes any field, and the direct result is
bit-identical to the general result, including the sign and exponent of zero.
The precision and exponent premises are checked at dispatch; the complement
continues through the complete retained-digit, saturation and clamping path.

Let F be the count of all fractional digits, D the count of all discarded
digits, and E the signed written exponent. The original preferred exponent is
`E-F`, never `E-F+D`. The wide exponent is `E-F+D`. Equivalently, if K
fractional digits were retained or skipped as leading zeros and I integer
digits were discarded, the wide exponent is `E-K+I`.
Consequently `preferred<=wide.exp`. An exact result uses the closest attainable
exponent to the original preference; zero clamps that preference to the type
range and preserves its written sign. `fit` establishes all encoding bounds.

Supported targets have pointer width at most 64; a string has length at most
`isize::MAX<=2^63-1`, so each digit count fits i128. The unsigned written
exponent is saturated at `2^64` before applying its sign. Each accumulation
step is at most `10*2^64+9`, and both scale computations fit i128. If exponent
saturation occurs, subtracting F or adding D cannot bring either scale within
`2^63` of zero: the resulting nonzero value is still far above every maximum
or below every minimum. Zero depends only on the same clamped preference.

After combining counts, independently clamp both scales to `[-2^40,2^40]`
and convert to i64. A nonzero retained coefficient has at most 35 digits.
At either clamped extreme it remains outside every representable range even
after the largest precision adjustment (34 digits): positive scale overflows;
negative scale is inexact without a direction, and rounds to signed zero or
one unit at qmin with a direction. These classifications and zero's exponent
are identical before and after clamping. If only the preference is clamped,
the closest attainable exponent is unchanged because all legal exponents lie
strictly inside the clamp. Clamping is monotone, preserving `preferred<=exp`.

`parse_unbounded_digits_saturation_and_scale_clamps` checks both sides of
both thresholds with integer/fractional digit counts crossing them, exponent
strings beyond `2^64`, 10,000 leading/fractional zeros, and million-digit
inputs. `parse_coefficient_exponent_sign_and_range_classes` checks all widths,
signs and modes, 34/35/36/40 significant digits, discarded zero/nonzero tails,
maximum finite neighbors, sub-qmin values, and signed-zero preference.

The independent reference uses a different exponent threshold,
`input length+20000`. After grammar validation it classifies nonzero values
by decimal order outside `[qmin-1,qmax+P]`. The lower class is strictly below
one unit at qmin and the upper strictly exceeds every finite result even
under directed rounding. Inside that interval it removes exact trailing
zeros before constructing a BigUint, rounds with `round_integer`, then restores
the original preference only when independent integer equality proves the
result exact. This preserves long zero-tail tests without constructing huge
powers. Reference expected fields never use production parsing or layout.

### Plain notation formatting

Domain: canonical finite fields, `c<10^P`, `qmin<=q<=qmax`. `NumBuffer`
formats c into 1..=34 ASCII digits. Nonnegative exponents append exactly q
zeros for nonzero c; zero writes one digit. Negative exponents either split
the digits at the checked interior offset or write `0.`, `-q-n` zeros and n
digits. Stripping removes at most `min(trailing coefficient zeros,-q)` digits
and adjusts the exponent equally; zero strips directly to `0`. Integer zeros
are never removed. A negative sign is always retained, including negative zero.

Before padding, the longest output is Decimal128 with c=1, q=-6176 and a
negative sign: `1+2+6175+1=6179` bytes. Nonnegative-exponent output has at most
`1+34+6111=6146` bytes. A point inside the digits needs at most `1+34+1=36`.
Every smaller width is contained in these bounds. Stripping cannot increase
length. The unsigned stack buffer has 6179 bytes, more than the maximum
6178-byte magnitude. All slices/splits are checked; copied slices have equal
length by construction. Zero blocks use lengths at most 64 and subtract at
most the remaining count. No index, arithmetic, or UTF-8 conversion can panic.

The normal path writes pieces directly through `fmt::Write`. Formatting flags
use the bounded unsigned stack text and `pad_integral`; padding is streamed
to the writer and is not limited by the stack capacity. Precision is ignored
on both paths. Display and Debug share the same formatting implementation,
including on the private-field Stripped adapter. Every writer failure is
propagated. No library formatting path allocates.

## Exhaustive and differential evidence

All tests use the optimized test profile with integer overflow checks enabled.
Each test runs on one thread, so the test harness's `--test-threads` setting
bounds the cores a run occupies. Fifteen tests are ignored by default and run
with `cargo test -- --ignored`: the four Decimal32 exhaustive tests (`decimal32_exhaustive_reference`,
`decimal32_exhaustive_widening_reference`, `quantize_decimal32_exhaustive_reference` and
`decimal32_exhaustive_format_parse_round_trip`), the nine tests whose names end in
`_million` or contain `_million_per_`, `mul_decimal128_pair_every_coefficient_class_pair`
and `div_every_coefficient_class_pair`. The other tests run with `cargo test`, including
the ten-million-input structured and widening tests and the million-input kernel division
tests.

| Operation | Test and coverage |
|---|---|
| Decimal32 decoding/encoding | `decimal32_exhaustive_reference`: all 4,294,967,296 patterns; 3,840,000,000 finite, 186,531,840 noncanonical, 134,217,728 infinity, 134,217,728 NaN; accepted fields, bit identity, and rejection errors checked |
| Decimal64/Decimal128 decoding/encoding | `decimal64_structured_splitmix64_reference`, `decimal128_structured_splitmix64_reference`: 10,000,000 structured patterns each; `all_exponents_signs_and_coefficient_boundaries`: every exponent, both signs, both steering forms, binary/decimal boundaries; `structured_in_range_encode_decode`: 100,000 generated field tuples per width |
| Decimal32 widening | `decimal32_exhaustive_widening_reference`: every 32-bit pattern; every accepted finite value widened to both widths retains sign, coefficient, exponent under reference decoding; canonical specials are checked against independently constructed widened bits, quieting NaNs; rejected patterns remain invalid input |
| Decimal64 widening | `decimal64_widening_ten_million_reference`: 10,000,000 independently constructed and decoded values; `widening_keeps_parts_for_all_classes`: all coefficient/exponent classes in both source widths |
| Comparison | `decimal32_all_class_pairs_bigint_comparison`: every ordered pair of Decimal32 coefficient/exponent classes, both signs; `all_nine_width_pairs_bigint_random`: 2,000 pairs for each of nine width combinations; `all_nine_width_pairs_bigint_million`: 1,000,000 per combination |
| Equality/hash/sign | `cohort_hashes_cross_width_and_zero_signs`: every class member, every representable cohort expansion, cross-width equal values, zero at extreme exponents and both signs; all finite pair comparisons also check equality and OrderedDecimal hash agreement when equal |
| Narrowing | `narrowing_all_coefficient_exponent_classes_bigint`: each of three width pairs, all classes and all three modes; `narrowing_three_pairs_bigint_random`: 2,000 values per pair; `narrowing_three_pairs_bigint_million`: 1,000,000 per pair, each in exact mode and both directions |
| Addition/subtraction | `addsub_all_27_coefficient_gap_sign_classes`: all 27 source/source/result combinations, both operations and all modes, every coefficient digit class paired across widths, both exponent orders, all sign pairs, gaps 0..39, 70, 200, 12000 clipped to the source ranges; `addsub_all_27_bigint_random`: 2,000 pairs per combination, each checked in six operation/mode cases; `addsub_all_27_bigint_million`: 1,000,000 pairs per combination with wall-time output |
| Addition/subtraction range and zero | `addsub_largest_finite_underflow_and_cancellation`: both signs around destination maximum and minimum exponents, exact/directed overflow, inexact and rounded zero; `addsub_zero_cohorts_and_self_operations`: both-zero signs and extreme exponents, cancellation, equal-value cohorts, x+x, x-x, x-(-x) |
| Multiplication | `mul_all_width_pairs_coefficient_exponent_sign_classes`: every pair of source coefficient representatives in each of the eight operand width pairs with at most one Decimal128 operand, and every third pair (offset by the first coefficient, so every representative of each side occurs) for two Decimal128 operands, exponent sums targeted at each result range, all four sign pairs, all three result widths and all three modes; `mul_decimal128_pair_every_coefficient_class_pair`: every pair of representatives for two Decimal128 operands with the same exponent sums, signs, result widths and modes; `mul_all_width_pairs_bigint_random`: 2,000 pairs per width pair, each into all result widths and modes; `mul_all_width_pairs_bigint_million`: 300,000 pairs per width pair, each into all three result widths and all three modes, with wall-time output |
| Multiplication fast path | `mul_exact_product_capacity_widening_and_exponent_boundaries`: all nine width pairs in both operand orders, all result widths, signs and modes, zero and maximum coefficients, `10^34/a` neighbors for small factors, coefficients `2^64-1`, `2^64`, `2^64+1` and the low-limb carry boundary for each small factor, the `2^128` widening boundary, every result range end with the neighbor outside it (for Decimal128 also the neighbor inside it), and extreme exponent sums |
| Multiplication general path and range | `mul_limb_boundaries_operand_order_and_every_reachable_shift`: limb values 0, 1, 9, `5*10^16`, `10^17-1` in both limbs of both operands, maximum coefficients, `2^64` neighbors, product digit counts 34..68 so every shift 0..33 and both `H=0` and `H>0`; `mul_max_coefficient_carry_overflow_underflow_and_cohorts`: product `10^35-1`, directed carry through `10^34`, maximum finite neighbors, exact and directed overflow, exact cohort shifts from sums above qmax, underflow and XOR signed zero, zero operands at extreme sums; `mul_source_exponent_classes_and_equal_sum_decompositions`: Cartesian source exponent classes, all sign pairs/modes and equal-sum decompositions |
| Division | `div_all_width_pairs_coefficient_exponent_sign_classes`: for each of the nine operand width pairs, about 400 pairs of source coefficient representatives (offset by the first coefficient, so every representative of each side occurs), exponent differences at each operand range end, result range end, overflow, least full-precision and underflow transition and their neighbors, all four sign pairs, all three result widths and all three modes; `div_every_coefficient_class_pair`: every pair of representatives with the same exponent differences, signs, result widths and modes; `div_all_width_pairs_bigint_random`: 1,000 pairs from each of two exponent distributions (the full source range, and -40..40) per width pair, each into all result widths and modes; `div_all_width_pairs_bigint_million`: 150,000 pairs from each distribution per width pair, with wall-time output |
| Division paths, zeros and range | `div_scale_paths_terminating_quotients_and_precision_carry`: divisors of every digit length 1..34 with their neighbors, powers of two and five, the largest scale, the `k=0` path and precision carry, into all result widths; `div_zero_operands_signs_and_exponent_clamps`: zero and nonzero dividends and divisors at each operand range end and around zero, all sign pairs, result widths and modes, NaN for zero/zero, XOR-signed infinity for nonzero/zero, and the zero quotient sign; `div_range_boundaries_overflow_underflow_and_cohorts`: dividends 1, 5, `10^(P-1)`, the maximum and its predecessor at each operand and result range end and its inner neighbor, over divisors 1, 2, 3, 9, 10, 11 at exponents -2..2, for exact and directed overflow, underflow and cohort clamps; `div_documented_examples`: the documented preferred exponents and directed results |
| Quantize | `quantize_all_width_coefficient_exponent_target_sign_classes`: full coefficient/exponent/sign classes crossed with every target for Decimal32/64; Decimal128 full classes crossed with exponent classes and all local gaps through P+1, plus zero/unit/full-digit power/maximum coefficients at seven source exponents crossed with every target; `quantize_all_widths_bigint_random`: 2,000 per width; `quantize_all_widths_bigint_million`: 1,000,000 per width, both directions and wall time |
| Quantize boundaries | `quantize_zero_all_targets_invalid_targets_and_precision_edges`: both signed zeros at every target, exact scale-up through and past precision, all-nines rounding increment, gaps 38/39, qmin-1/qmax+1/i32 extremes; `quantize_decimal32_exhaustive_reference`: all patterns and fifteen targets, both directions |
| Rounding to a multiple | `multiple_coefficient_exponent_sign_and_step_cohort_classes`: all value classes and signs for Decimal32/Decimal64 and every 13th class for Decimal128 (with every 5th step) crossed with steps of coefficient 1, 5, 50, 100 at every Decimal32/Decimal64 step exponent and at Decimal128 exponents near zero, the range ends and every 997th exponent, unit steps with every trailing-zero count at the maximum exponent, and general coefficients 2, 3, 7, 25, 125, 999, 1024, 4375, `max/3`, `max-1`, `max`; `multiple_general_steps_every_scale_gap_and_unrepresentable_multiples`: step coefficients 2, 3, 7, 25, 125, 999, 1024, 4375, `max/7`, `max-1` and `max` at exponent gaps -40..75, 110..114, 200, 767, 12286 and 12287 where both exponents fit the width (Decimal32 omits 200 and above, Decimal64 omits 12286 and 12287), including the 38-digit guard and `Inexact`/`Overflow` results; `multiple_all_widths_bigint_random`: 2,000 per width; `multiple_all_widths_bigint_million`: 1,000,000 per width, both directions and wall time |
| Rounding to a multiple boundaries/errors | `multiple_exact_multiples_carry_overflow_and_signed_zero`: exact multiples, precision carry, largest finite overflow, zero signs/preference; `multiple_zero_and_negative_steps_are_rejected_in_order`: both signed zero steps and negative steps for all three widths; `multiple_carry_below_and_at_range_end_and_precision_limited_exponents`, `multiple_division_exponent_boundaries_every_zero_count_and_both_units`, `multiple_zero_values_of_both_signs_keep_sign_at_step_exponent` |
| Parsing grammar | `parse_grammar_shapes_and_rejected_classes`: finite significand shapes, optional signs, both exponent markers/signs/leading zeros, invalid numeric syntax and malformed special spellings; FromStr agrees with parse for every width |
| Parsing numeric/range | `parse_coefficient_exponent_sign_and_range_classes`, `parse_unbounded_digits_saturation_and_scale_clamps`: coefficient/exponent/sign/mode classes, accumulation and saturation boundaries, million-digit input; `parse_long_nonzero_significands_in_range`: nonzero significands of 3,002 to 100,000 significant digits, one with 3,000 interior zeros, within the Decimal128 range and including one near its minimum exponent |
| Parsing fuzz | `parse_valid_and_invalid_grammar_fuzzer_ten_thousand_per_width_mode`: 10,000 valid plus 10,000 invalid per width, all modes; `parse_valid_and_invalid_grammar_fuzzer_million_per_width_mode`: 1,000,000 of each per width/mode, wall time |
| Plain/stripped formatting | `format_all_coefficient_exponent_sign_classes_and_extreme_lengths`: full coefficient/exponent/sign classes and 6179-byte limit; `format_flags_precision_ignored_and_debug_equals_display`: width below/above length, space/star/Unicode fills, all alignments, sign/zero flags crossed with widths and precision; `format_writer_errors_propagate_on_direct_and_padded_paths`: writer failures |
| Format/parse differential | `format_parse_random_two_thousand_per_width`: 2,000 independently generated values per width; `format_parse_million_per_width_independent_references`: 1,000,000 per width, plain/stripped strings against independent formatting and parsing in all modes, wall time |
| Decimal32 format/parse | `decimal32_exhaustive_format_parse_round_trip`: every 32-bit pattern; finite plain and stripped via stack-backed fmt::Write; canonical special plain output reparsed for NaN/infinity class and infinity sign; finite fields and rejected patterns checked independently; wall time |
| Range boundaries | `narrowing_largest_finite_and_underflow_boundaries`: both signs, maximum finite neighbors at four finer exponents, and underflow/carry/zero around `qmin`, in all three modes |
| Sign operations | `decimal32_exhaustive_reference`, `decimal64_structured_splitmix64_reference`, `decimal128_structured_splitmix64_reference`, `all_exponents_signs_and_coefficient_boundaries`: at every accepted finite pattern, negation, `abs`, `copysign` with a positive and a negative sign source, and both sign tests, against the reference encoding of the decoded fields with the expected sign; accepted special patterns check negation, `abs`, the negative sign test and classification, without `copysign` or the positive sign test; `decimal128_construction_paths_store_small_form`: the three sign operations keep finite Decimal128 results in small form |
| Constants | `constants_match_reference`: `DIGITS`, `MIN_EXP`, `MAX_EXP` and the stored bits of `ZERO`, `MAX`, `MIN` for every width; each constant classifies to its expected fields and re-encodes to the same bits under the reference model |
| Operand trait | `operand_bound_converts_to_decimal128`: each `Operand` type converts to the same Decimal128 as `From` |
| Errors | `errors_display_and_implement_core_error`: every `Error` variant's Display message, and `core::error::Error` with no source |

### Decimal32 exhaustive format/parse round trip

`decimal32_exhaustive_format_parse_round_trip` visits all `2^32` patterns.
The independent decoder classifies every pattern. Rejected patterns must
return `InvalidInput`. Accepted finite values are formatted both plain and
stripped into a 6179-byte test stack writer and parsed exactly.
Plain output with q<=0 must retain identical bits; with q>0
it must have the same value and the representable exponent closest to zero.
The expected cohort is calculated from original reference fields using bounded
native integer scaling and the independent encoder, without production code.
Stripped output instead uses the exponent after fractional-zero removal
(including exponent zero for zero), then the same positive-exponent rule.
Requiring original bits after stripping would contradict removing fractional
zeros; both finite variants preserve numeric value and written sign.
Canonical specials use only plain formatting in this exhaustive test: parsing
must retain NaN/infinity classification and the infinity sign. A formatted
NaN loses its sign and payload; this branch does not inspect the parsed
NaN signaling bit or payload.

### Decimal32 exhaustive quantize reference

`quantize_decimal32_exhaustive_reference` visits every one of the `2^32` bit
patterns, at all fifteen targets `{-101,-12,-11,...,0,90}`, in both directions.
Accepted finite operands use the independent BID decoder; expected finite
results are encoded by the reference encoder and compared bitwise, including
sign and exponent. Rejected inputs must remain `InvalidInput` through `from_bits`.
Canonical NaNs must return the same sign and payload with the signaling bit
cleared; infinities must return positive quiet NaN with payload zero, checked
bitwise at every listed target and direction. The test reports wall time.

The native finite reference shares no arithmetic or layout code with production.
A decoded coefficient is below `10^7`; exponent differences are at most 191.
For scale-up, zero is handled directly, a gap at least seven is rejected, and
smaller gaps use checked `u64` powers and multiplication with product below
`10^13<2^64`. Products at least `10^7` are rejected. For scale-down, gaps at least
seven give quotient zero and remainder `c`; smaller gaps use native `u64`
Euclidean division by checked powers at most `10^6`. Checked increment gives
at most `10^6`; sign and target are preserved. All other arithmetic is bounded
by these decoded ranges; conversion into `u64` is exact. Thus this reference
is exact over the finite Decimal32 domain without arbitrary-precision cost
inside the exhaustive loop. Before enumeration the same reference is compared
with the BigUint oracle on the full Decimal32 coefficient/exponent/sign class
set, all fifteen targets and both directions. The same comparison is also the
fast test `quantize_decimal32_native_reference_matches_bigint_all_classes`.

## Class coverage argument

The table below partitions finite inputs. Special classification and result
rules are covered separately in Infinity and NaN; the finite integer
arguments do not prove that public dispatch.

Coefficient representatives include zero, powers of ten and their neighbors,
all-nines, `5*10^j`, and two fixed-seed values for each digit length. Exponents
include both range ends and their neighbors, every integer from `-P-1` through
`P+1`, the 34..39 alignment boundaries, and precision-adjusted overflow and
underflow transitions. Narrowing adds destination range transitions to the
source classes. Both signs and all three modes are enumerated.
Representatives alone do not prove an interval: the integer obligations above
establish the result for all members of each following disjoint path class.

| Partition of finite inputs | Why members share the path and result rule | Tests exercising the partition |
|---|---|---|
| Comparison extraction: Decimal128 finite canonical small form; Decimal32/Decimal64 both finite coefficient forms | Decimal128's excluded form has coefficient >=2^113, above its maximum; after special classification, fixed field extraction equals general decoding on every finite canonical value | `comparison_decimal128_canonical_storage_boundary` checks 64/112-bit limb boundaries, the maximum and first invalid coefficient, and both excluded finite combination extremes against independent decoding; `comparison_alignment_boundaries_all_width_pairs`, `comparison_extreme_exponents_all_width_pairs` and `cohort_hashes_cross_width_and_zero_signs` check public comparison against independent decoding and hashing; `decimal128_construction_paths_store_small_form` checks constants, parsing including `FromStr`, decoding, quantize, widening including Decimal32/Decimal64 values rounded to a multiple, addition and subtraction with and without a rounding direction for all nine operand width pairs, and multiplication with and without a rounding direction for all nine operand width pairs into Decimal128, and rounding to a multiple; it filters out special results and requires at least one successful finite result for each path and width pair, and asserts the stored coefficient and exponent of a rounded parse and a rounded addition that carry out of 34 digits; `decimal128_from_bits_rejects_large_form` checks the finite large-form extremes (combination field 24 to 29, both signs, low 122 bits all zero or all one) and 100,000 patterns with a combination field drawn from 24 to 29 and random sign and low 122 bits |
| Comparison: Decimal32/Decimal64 identical bits; gap magnitude <=38-P or >38-P, for greater precision P=7/16/34 | Identical finite bits denote the same value; bounded signed alignment is exact including zeros; the complementary domain uses the complete general ordering rule | `comparison_alignment_boundaries_all_width_pairs` crosses all nine width pairs, both orders and signs, identical operands, gap limits 4/22/31 and both neighbors, maxima and cohort neighbors; `comparison_extreme_exponents_all_width_pairs` crosses each width's range endpoints and adjacent exponents, zeros and maxima |
| General comparison: zero/nonzero, equal/different effective signs | Zero ignores sign; different signs decide order; equal negative signs reverse the magnitude result | `decimal32_all_class_pairs_bigint_comparison`, `cohort_hashes_cross_width_and_zero_signs` |
| General comparison: ordered exponent difference, digit sum `<=35`/`>35`, scaled coefficient `<`/`=`/`>` | Either the bounded exact multiply decides order or the digit lower bound decides it; all nonzero coefficients have a unique digit length | `comparison_exponent_gap_digit_guard_and_sign_classes` checks all width pairs, coefficient classes, gap boundaries and both signs in both operand orders |
| Equality: Decimal32/Decimal64 identical bits; gap magnitude <=38-P or >38-P; both/one/no zero; equal/different signs; gap magnitude 1..33 or >=34; aligned product fits/overflows u128 | Exact alignment is necessary and sufficient; overflow and gaps >=34 cannot match a supported coefficient. These disjoint decisions exhaust all finite pairs and agree with OrderedDecimal Ord and decimal PartialOrd | `comparison_alignment_boundaries_all_width_pairs` covers all nine pairs and both orders, all gaps 0..39, coefficient maxima, exact cohort neighbors and checked-product overflow neighbors; `comparison_extreme_exponents_all_width_pairs` covers maximal cross-width gaps and zero signs; the other comparison and cohort tests in `src/compare_tests.rs` also apply |
| No digit reduction/digit reduction; precision shift smaller/equal/larger than underflow shift | The maximum defining `k` chooses exactly one division grid; quotient/remainder theorem applies for any coefficient within that class | `narrowing_all_coefficient_exponent_classes_bigint`, `fit_full_integer_domain_boundaries_and_sticky_rational_oracle` |
| Exact/inexact discard; zero/nonzero quotient; preferred shift limited by precision/preference | Euclidean remainder zero is exact; nonzero remainder or sticky is inexact; multiplication headroom decides the unique closest exponent | `narrowing_all_coefficient_exponent_classes_bigint`, `fit_preferred_exponent_clamps_and_extreme_i64_exponents` |
| Inexact, sign x direction; increment/no increment; carry/no carry | Every strictly positive discarded fraction gives the same directed integer endpoint; only quotient `10^P-1` can carry | `narrowing_largest_finite_and_underflow_boundaries`, `fit_full_integer_domain_boundaries_and_sticky_rational_oracle` |
| Final exponent below/at/above `qmax`; exact magnitude below/at/above maximum | Range comparison and the quotient/remainder inequalities decide success, `Overflow`, or `Inexact`; direction is applied before the rounded range test | `narrowing_largest_finite_and_underflow_boundaries`, `narrowing_all_coefficient_exponent_classes_bigint` |
| Addition/subtraction dispatch: gap magnitude `0..=4`, preferred exponent in result range and magnitude below `10^P`, or complementary general path | The signed scaled-integer bound covers every coefficient and sign; the accepted aligned result is exactly representable at its preference. Gap/range/capacity failure uses the full general algorithm | `addsub_equal_exponent_exact_path_boundaries`, `addsub_near_exponent_exact_path_boundaries`, `addsub_all_27_coefficient_gap_sign_classes` |
| Addition/subtraction: both zero/one zero/nonzero; shared/differing effective signs; cancellation | Effective signs and exact integer arithmetic decide the sign independently of width; zero uses the original preferred exponent clamped by `fit` | `addsub_zero_cohorts_and_self_operations`, `addsub_all_27_coefficient_gap_sign_classes` |
| Addition/subtraction: `digits(A)+d <=37` or `>37`; retry succeeds/fails; stripped count smaller/equal/larger than gap | Exact bound covers every aligned integer; bounded stripping preserves value and preference; failed retry implies a nonzero remainder and dominance | `addsub_exact_compression_boundary_and_trailing_zeros`, `addsub_all_27_coefficient_gap_sign_classes` |
| Addition/subtraction compression: same/opposite effective signs; `g<39`/`g>=39`; all destination precisions | The positive fractional tail and lower coefficient bound establish the same directed grid endpoint for every tail in `(0,1)` | `addsub_all_27_coefficient_gap_sign_classes`, `addsub_exact_compression_boundary_and_trailing_zeros` |
| Addition/subtraction output: exact/inexact, carry/no carry, maximum neighbors, underflow/rounded signed zero | Shared integer oracle classifies exact magnitude overflow before inexact; directed overflow uses unbounded-exponent rounding, with sign retained on rounded zero | `addsub_largest_finite_underflow_and_cancellation`, `addsub_all_27_bigint_random` |
| Multiplication: zero/nonzero; XOR sign; original exponent sum | Zero keeps XOR sign independently of mode; nonzero magnitude depends only on coefficients and exponent sum, including cohort preference | `mul_source_exponent_classes_and_equal_sum_decompositions`, `mul_max_coefficient_carry_overflow_underflow_and_cohorts` |
| Multiplication fast path: either coefficient below/at/above `2^64`, swapped or not; low carry zero/nonzero; high product limb zero/nonzero; product below/at/above `10^P`; preference inside/outside the result range | The two-limb recurrence is exact throughout each integer interval; the checked constructor accepts exactly the products already representable at their preference. Its complement uses the complete general kernel | `mul_exact_product_capacity_widening_and_exponent_boundaries` |
| Multiplication general path: zero/nonzero; H=0/H>0; each sh=0..33; discarded zero/nonzero | Out-of-range preferences keep the small-product classes reachable. Euclidean decomposition preserves the exact product; the shift digit bounds and the sticky premise decide all tails with a single rounding | `mul_limb_boundaries_operand_order_and_every_reachable_shift`, `mul_all_width_pairs_coefficient_exponent_sign_classes` |
| Multiplication: product precision reduction versus underflow; exact/inexact; carry/no carry; maximum finite neighbors | The shared fit proof applies to the bounded split product; overflow uses exact magnitude without a direction and unbounded-exponent rounding with a direction | `mul_max_coefficient_carry_overflow_underflow_and_cohorts`, `mul_all_width_pairs_coefficient_exponent_sign_classes` |
| Division: zero/nonzero divisor and dividend; XOR sign; preferred exponent `qa-qb` | Public division returns NaN for zero over zero and XOR-signed infinity for every other finite zero divisor; a zero dividend with a nonzero divisor depends only on the sign pair and the clamped preference | `div_zero_operands_signs_and_exponent_clamps`, `div_all_width_pairs_coefficient_exponent_sign_classes` |
| Division: `k>0`/`k=0`; each divisor digit length 1..34, hence each chunk limit 4..37; remainder zero/nonzero; stripped zero count below/at/above `k` | The digit-length bounds put every quotient of a class in `[10^(P-1),10^35)`; the long division is proved for every chunk; stripping preserves the value, and the cohort argument gives the same exponent for the passed and the original preference | `div_scale_paths_terminating_quotients_and_precision_carry`, `div_all_width_pairs_coefficient_exponent_sign_classes` |
| Division output: exact/inexact; carry/no carry; maximum finite neighbors; underflow and cohort clamps | The fit proof applies to the scaled quotient and its sticky flag; overflow uses the exact magnitude without a direction | `div_range_boundaries_overflow_underflow_and_cohorts`, `div_all_width_pairs_bigint_random` |
| Quantize: target outside/inside range; zero/nonzero; e>=q/e<q; scale-up digit guard pass/fail | Target validation bounds every exponent difference; the guarded power product is exact or cannot fit at q; zero bypasses powers | `quantize_all_width_coefficient_exponent_target_sign_classes`, `quantize_zero_all_targets_invalid_targets_and_precision_edges` |
| Quantize: exact/nonzero remainder; k<=38/k>=39; signed increment/no increment | Euclidean division selects one directed endpoint, always below 10^P; sign and q are preserved including rounded zero | `quantize_all_width_coefficient_exponent_target_sign_classes`, `quantize_decimal32_exhaustive_reference`, `quantize_all_widths_bigint_million` |
| Sign operations: each width; operand sign; `copysign` source sign; both steering forms; every exponent; zero/nonzero | Only the sign bit is read or written; the copied fields alone determine canonicity, coefficient and exponent | `decimal32_exhaustive_reference` (every Decimal32 encoding), `all_exponents_signs_and_coefficient_boundaries`, `decimal64_structured_splitmix64_reference`, `decimal128_structured_splitmix64_reference` |
| Rounding to a multiple: zero/negative/positive step; unit or general step; each trailing-zero count | Zero has priority over sign; decimal stripping selects the unit path exactly for 1 and 5 at precisions 7 and 16, and every other step takes the general path | `multiple_zero_and_negative_steps_are_rejected_in_order`, `multiple_coefficient_exponent_sign_and_step_cohort_classes`, `multiple_all_widths_bigint_million` |
| Rounding to a multiple, unit path: E>=0/E<0; -E<=19/-E>=20; quotient remainder zero/nonzero; sign x direction; zero result | Exact rational quotient `N*10^E` is already an integer or rounds once at the integer endpoint; 64-bit division is exact on `N<2^63`; the returned zero keeps the sign of `x` at the step exponent | `multiple_division_exponent_boundaries_every_zero_count_and_both_units` crosses -E of 1, 2, P-1, P, P+1, 16..21, 34..39 and 767 where they do not exceed the width maximum (767 exceeds it for Decimal32), and the width maximum and its predecessor, with units 1 and 5 where the exponent range admits them, asserting that each of these values is reached with at least one unit, every step zero count, coefficient remainders and both signs; `multiple_zero_values_of_both_signs_keep_sign_at_step_exponent`, `multiple_coefficient_exponent_sign_and_step_cohort_classes`, `multiple_exact_multiples_carry_overflow_and_signed_zero` |
| Rounding to a multiple, general path: zero value; q_x>=q_s/q_x<q_s; remainder computed directly or by reduction, or multiple decided by the coprime factor; remainder zero/nonzero; guard pass/fail; up/down; `exceeds_max` true/false; S below/above 10^38 | The exact multiple `M+-t` is built at an exponent no lower than `q_s`; a failed guard leaves a coefficient with no trailing zero and more than `P` digits, so the result is `Inexact` or the exact-magnitude `Overflow` | `multiple_general_steps_every_scale_gap_and_unrepresentable_multiples`, `multiple_coefficient_exponent_sign_and_step_cohort_classes`, `multiple_all_widths_bigint_million` |
| Rounding to a multiple, unit path: no precision carry/carry to 10^P below or at qmax; e=q_s or e>q_s with shift limited by preference or precision; range overflow | u*I<=c_x+4 and the multiple-of-five bound leave only the exact carry; the shift selects `max(q_s,e-(P-digits(c)))`; an exponent above qmax then implies a magnitude above the maximum | `multiple_carry_below_and_at_range_end_and_precision_limited_exponents`, `multiple_exact_multiples_carry_overflow_and_signed_zero`, `multiple_coefficient_exponent_sign_and_step_cohort_classes` |
| Parsing SWAR P=7 dispatch: signed significand byte length 0/1..8/>8; coefficient below/at/above 10^7; zero/one/several points at every lane; any nondigit at every lane | Exact zero-lane detection, point-lane deletion and chunk validation give the same fields as fit; all complementary cases use complete parsing | `parse_short_significands_match_general_parser` enumerates every distinct short string over digit, point, exponent, sign, ASCII-boundary and UTF-8 tokens against the general parser in all modes; `parse_short_significands_match_independent_reference` checks every sign, length 1..8 and point position with `/ : . e E \u{0} \u{7f} \u{80} \u{ae}` at every position against the independent reference, because the general parser shares `fit` with this path; `parse_accumulator_chunk_and_exact_dispatch_boundaries`, `parse_invalid_bytes_at_each_chunk_position` cover 7/8/9 digits, 10^7-1/10^7, signs, zeros, points, exponent syntax and invalid bytes |
| Parsing SWAR P=16 dispatch: signed significand byte length <8/8..16/17/>17; lead digit `0`, other digit or point; zero/one/several points at every lane; any nondigit at every lane | Exact zero-lane detection over both halves, lead/point-lane deletion and two-half chunk validation give the same fields as fit; all complementary cases use the bounded scan or complete parsing | `parse_short_significands_match_general_parser` (P=16 strings up to 10 bytes over digits, point, exponent and nondigit; digit/point strings up to 22 bytes) in all modes; `parse_short_significands_match_independent_reference` checks every sign, length 1..17, point position and `/ : . e E \u{0} \u{7f} \u{80} \u{ae}` at every position against the independent reference; `parse_invalid_bytes_at_each_chunk_position` covers every invalid ASCII byte and UTF-8 class as the last byte of lengths 8..17 |
| Parsing bounded scan: signed byte length 0..35/>35; digits with zero/one point or any other byte; coefficient below/at/above capacity; chunk/scalar runs and invalid bytes at every lane | The bounded recurrence and point-offset identity establish the exact fields; every other input reaches the general grammar and fit | `parse_short_significands_match_general_parser` (P=16/34 strings up to 10 bytes over digits, point, exponent and nondigit; digit/point strings up to 22 bytes), `parse_accumulator_chunk_and_exact_dispatch_boundaries`, `parse_invalid_bytes_at_each_chunk_position` cover digit and byte-length boundaries, every point position, exponent markers, ASCII nondigits and UTF-8 classes |
| Parsing exact dispatch: no dropped digits, retained length <=P and preference in range; or the complement | The direct-result proof establishes equality of every field in all modes; the complementary path uses the complete fit contract | `parse_accumulator_chunk_and_exact_dispatch_boundaries` crosses coefficient capacity, all point positions at digit boundaries, both signs, exponent syntax and qmin/qmax neighbors |
| Parsing: accepted/rejected grammar; zero/nonzero; retained length 0..35; dropped zero/nonzero; both exponent saturation signs and clamp boundaries | Finite byte grammar is exhaustive; accumulation recurrence and sticky lower bound cover every length and tail; monotone scale clamping preserves range, sign and preference rules | `parse_grammar_shapes_and_rejected_classes`, `parse_coefficient_exponent_sign_and_range_classes`, `parse_unbounded_digits_saturation_and_scale_clamps`, `parse_long_nonzero_significands_in_range`, parsing fuzz tests |
| Formatting: q>=0/q<0; zero/nonzero; point inside/before digits; strip removes none/some/all fractional zeros | Each finite coefficient has a unique digit string and point position; bounded concatenation preserves exact value and exponent, with signed zero handled explicitly | `format_all_coefficient_exponent_sign_classes_and_extreme_lengths`, `format_parse_random_two_thousand_per_width`, `decimal32_exhaustive_format_parse_round_trip` |
| Formatting flags absent/present; precision; writer success/failure | Direct writes and bounded stack construction produce identical magnitude; integral padding applies sign/width/alignment without rounding or allocation; errors propagate | `format_flags_precision_ignored_and_debug_equals_display`, `format_writer_errors_propagate_on_direct_and_padded_paths` |
| Trailing-zero count 0..33; all zero representations | Exact power division gives a unique nonzero canonical tuple; zeros use a single tuple across widths | `cohort_hashes_cross_width_and_zero_signs`, `strip_trailing_zeros_matches_naive_for_every_power` |

These finite partitions are exhaustive because every finite input has a sign,
a zero classification, a digit length, an integer exponent, and a unique
quotient and remainder at its selected grid. Boundary tests exercise the decisions;
the bounded integer proofs extend their rules to the interior members.
There is no exhaustive enumeration of Decimal64/Decimal128 encodings or of
all comparison pairs, narrowing inputs, addition/subtraction, multiplication,
division, or value/step pairs for rounding to a multiple, or all quantize targets over Decimal64/Decimal128 inputs.
Their evidence is the path-class argument plus independent differential tests, rather than random sampling alone.
The addition/subtraction oracle decodes both operands with the reference BID
implementation, scales signed `BigInt` coefficients to their original minimum
exponent, and forms the exact sum or difference. It then uses the same
arbitrary-precision rounding routine as narrowing. Zero sign selection is
independent of production arithmetic; a nonzero integer rounded to zero keeps
its integer sign. No production layout or kernel is used for expectations.
Multiplication uses the reference-decoded signs and coefficients to construct
an exact `BigInt` product at the sum of the original exponents, then calls the
independent integer rounding oracle. Expectations use neither the production
split nor production decoding. Every coefficient-class pair is crossed with
exponent-sum classes (for two Decimal128 operands the default test takes every
third pair and the ignored test every pair): all integers from `-N-1` through
`N+1` for product digit length `N`, and neighbors within three of sum range
ends, qmin, qmin-N, qmax, and qmax+P-N for result precision P, restricted to
achievable sums. Each sum has a valid source
exponent decomposition. This quotient of the source exponent space is exact:
for fixed coefficients and signs the product, preference, and all rounding
decisions depend only on the sum. The source-exponent test additionally crosses
each source's range endpoints and three neighbors and every integer from
`-P-1` through `P+1`, with zero/unit/maximum coefficients and alternate
equal-sum decompositions.
Quantize independently scales BigUint coefficients and divides by arbitrary-
precision powers to obtain the directed integer at exactly the target exponent.
Rounding-to-a-multiple expectations form a signed BigInt numerator and
positive denominator at the minimum operand exponent, floor/ceil their
rational quotient, multiply by the original step coefficient, and fit that
exact integer with no direction and preferred exponent equal to the original
step exponent. Neither expectation
uses the production transformations. There is no exhaustive enumeration of
all strings or Decimal64/Decimal128 format/parse values; parsing and formatting
use the bounded recurrence and path-class proofs plus independent differential
evidence. Performance remains uncovered.
