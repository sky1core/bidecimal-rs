# bidecimal Specification

This file is the contract for the repository. Code and tests follow it; when
they disagree, the code is wrong.

## Goal

bidecimal provides fixed-width IEEE 754-2019 decimal types — `Decimal32`,
`Decimal64`, and `Decimal128` — whose operations return an exact result, a
single rounding in a direction the caller names, an IEEE 754-2019 infinity or
NaN result, or an error. The explicit binary floating-point conversions are
the narrow exception: they use the fixed nearest, ties-to-even rule and
precision limits specified below.

The project succeeds when all of the following hold:

1. **Exact or explicit.** Every supported operation returns the exact result,
   a result rounded in a direction the caller named, the infinity or NaN result
   defined below, or an error. The explicitly named binary floating-point
   conversions instead use the fixed nearest, ties-to-even contract below,
   including its precision limits, overflow and underflow results. No other
   operation silently rounds, truncates, or overflows.
2. **Proven correct.** Correctness is backed by evidence that covers every
   supported input. A count of passing tests is not that evidence, and neither
   is having ported another implementation such as Intel BID or IBM decNumber.
3. **Compact.** Each value occupies exactly 4, 8, or 16 bytes.
4. **Pure Rust.** The library itself uses no C library and no FFI at run time.

## Design Direction

Every feature, including later ones, keeps this direction.

- The types follow IEEE 754-2019 as Rust's `f64` does: infinities and NaNs
  are values, and comparison is the IEEE 754-2019 comparison, under which a
  NaN is unordered, so the types implement neither `Eq`, `Ord`, nor `Hash`.
  This sets value and comparison semantics; it does not mean providing every
  `f64` operation.
- A key order is explicit. `OrderedDecimal` gives `Eq`, `Ord`, and `Hash`
  with numeric keys, as `OrderedFloat` from the `ordered-float` crate does for
  `f64`. `total_cmp`, the IEEE 754-2019 totalOrder predicate, is a separate
  order that keeps representations apart.
- Arithmetic is decimal and rounds only in a direction the caller names
  (Goal 1), never implicitly as `f64` arithmetic does.

## Types

| Type | Storage | Significant digits | Exponent q (coefficient × 10^q) |
|---|---|---|---|
| `Decimal32` | 4 bytes | 7 | −101 … 90 |
| `Decimal64` | 8 bytes | 16 | −398 … 369 |
| `Decimal128` | 16 bytes | 34 | −6176 … 6111 |

- A value is the IEEE 754-2019 BID (binary integer decimal) encoding of the
  interchange format of its width. It carries no other data.
- Supported values are the finite values of each format — zeros of either
  sign and every finite number in every representation (cohort member) — both
  infinities, and quiet and signaling NaNs with their sign and payload.
- Decoding bits accepts exactly the canonical encodings. A canonical infinity
  has every bit after the combination field clear. A canonical NaN has every
  bit between the signaling bit and the trailing significand clear and a
  payload below 10^(P−1), where P is the number of significant digits. A
  non-canonical encoding is invalid input.

## Correctness Contract

Items 1–7 concern finite operands; infinity and NaN operands follow
[Infinity and NaN](#infinity-and-nan).

1. If the exact result is representable in the result type, the operation
   returns it, except that binary-to-decimal conversion applies its explicitly
   limited significant-digit precision even when the full decimal format
   could represent more digits exactly.
2. Except for the explicit binary floating-point conversions below, an
   unrepresentable result is rounded in the direction the caller specified or
   returns an error. Those operations have no default rounding. Addition,
   subtraction, multiplication, division, narrowing conversion, and parsing
   can each be called without a direction (an unrepresentable result is an
   error) or with one (the result is rounded). Quantize and rounding to a
   multiple always take a direction. The binary conversion rule does not
   change any of these operations.
3. The caller-selectable rounding directions are roundTowardNegative and
   roundTowardPositive. Binary floating-point conversion uses fixed
   roundTiesToEven; this adds no variant to `Rounding`.
4. For operations other than the binary floating-point conversions below,
   without a direction, overflow is an error: the exact result exceeds the
   largest finite value of the result type in magnitude. With a direction,
   overflow is determined as in IEEE 754-2019 — the result, rounded in that
   direction as if the exponent range were unbounded, exceeds the largest
   finite value of the result type — and gives the IEEE 754-2019 overflow
   result: toward positive, +∞ for a positive result and the most negative
   finite value for a negative one; toward negative, the largest finite value
   for a positive result and −∞ for a negative one. Rounding to a multiple is
   the exception: its overflow is an error in either direction. Binary
   floating-point conversion uses its specified nearest-even overflow and
   gradual-underflow mapping, returning a value.
5. No operation panics, aborts, or wraps. Failures of fallible operations are
   reported through `Result`, and errors distinguish at least: not exactly
   representable, overflow, division by zero, and invalid input. The binary
   floating-point conversions are total over valid source values and return
   the specified values, without `Result`, `Option`, or status flags.
6. Except for the binary-to-decimal cohort policy below, the exponent of a
   result follows IEEE 754-2019: an exact result uses the preferred exponent
   of the operation, or the representable exponent closest to it; a rounded
   result uses the least possible exponent. The exact trailing-zero value
   transformations retain their separate exponent rules below.
7. The sign of a zero result follows IEEE 754-2019. Where IEEE 754-2019 makes
   that sign depend on the rounding direction, an operation called without a
   direction gives +0. Binary floating-point conversion preserves the source
   sign, also on underflow to zero; a binary-to-decimal zero has exponent zero.

## Infinity and NaN

- Operations give the IEEE 754-2019 results for infinity and NaN operands, as
  listed for each operation below. Binary floating-point conversion has its
  separate sign, quieting and payload mapping below. Only the values are
  provided: the IEEE 754-2019 status flags are not.
- Except for the separate binary floating-point conversion mapping, a NaN
  operand gives a quiet NaN with the sign and payload of the first NaN
  operand. A payload that is not below 10^(P−1) for the result type becomes
  zero. A signaling NaN operand gives a quiet NaN like a quiet one; only the
  sign operations and decoding bits keep a signaling NaN.
- An invalid operation gives the positive quiet NaN with payload zero:
  `∞ − ∞` for infinities of the same sign (and `+∞ + −∞`), `0 × ∞`, `0 ÷ 0`,
  `∞ ÷ ∞`, and the cases listed under each operation.
- Except for the explicitly named binary floating-point conversions, an
  operation without a direction whose finite result is not representable
  returns an error, never an infinity or NaN.

## Operations

### Addition and Subtraction

- Available on every type directly. Nothing forces promotion to `Decimal128`.
- Operands may have the same or different widths. The caller can name any of
  the three types as the result type; same-width operations also have a form
  that returns the operand type.
- An infinite operand gives an infinity: the first operand's when it is
  infinite, and otherwise the second operand's, negated for subtraction. The
  sum of infinities of opposite signs and the difference of infinities of the
  same sign are NaN.

### Multiplication

- Available on every type directly, like addition: operands may have any of
  the three widths, and the caller names any of the three types as the result
  type.
- The product is computed without intermediate error; the correctness contract
  then applies to the result. The preferred exponent is the sum of the operand
  exponents.
- An infinite operand gives an infinity, negative exactly when the operand
  signs differ; an infinity times zero is NaN.

### Division

- Available on every type directly, like multiplication: operands may have
  any of the three widths, and the caller names any of the three types as the
  result type.
- The quotient is computed without intermediate error; the correctness
  contract then applies to it. The preferred exponent is the dividend's
  exponent minus the divisor's: `1.00 / 4` is `0.25`, and `1 / 8` is `0.125`.
- A nonzero finite value divided by zero and an infinity divided by a finite
  value give an infinity. Zero divided by zero and an infinity divided by an
  infinity give NaN. A finite value divided by an infinity gives zero at the
  least exponent of the result type.
- The sign of a quotient, including zero and infinity, is negative exactly
  when the operand signs differ.

### Rounding to a Multiple

Rounds a value to an integer multiple of a step:

```text
result = step × to_integer(x / step), returned in the type of x and step
```

- Available on every type; `x` and `step` have the same type.
- `to_integer` is floor (roundTowardNegative) or ceiling (roundTowardPositive),
  chosen by the caller.
- A NaN `x` or step gives a quiet NaN before any error. Otherwise the step
  may be any positive value: a zero step is a division-by-zero error, and a
  negative step, −∞ included, is invalid input. An infinite `x` with a finite
  step gives that infinity, and a step of +∞ gives NaN.
- Rounding happens once, when the quotient becomes an integer. Returning the
  multiple in the type is exact or an error, never a second rounding: a
  multiple beyond the largest finite value is an overflow error in either
  direction, and any other
  multiple that needs more significant digits than the type has is a
  not-exactly-representable error. For a step of the form 10^k or 5 × 10^k,
  the latter error cannot occur.
- The preferred exponent of the result is the step's exponent: `1.53` rounded
  down to a multiple of `0.05` is `1.50`. A zero result takes the sign of `x`.

### Quantize

- Rounds a value of any type to a target exponent q, toward negative or
  toward positive; q = −n keeps n fractional digits.
- A NaN gives a quiet NaN with its sign and payload, and an infinity gives NaN,
  whatever q is.
- For a finite value, the result has exponent q. A q outside the type's
  exponent range is invalid input, and so is a q at which the coefficient of
  the result does not fit the type, as for the IEEE 754-2019 invalid
  operation: `1` at q = −7 in `Decimal32` needs 8 digits. Quantize never
  overflows.

### Sign Operations

- Negation, absolute value, and copying the sign of another value of the same
  type (IEEE 754-2019 negate, abs, and copySign) are available on every type.
  They change only the sign bit, so they are always exact and never fail, also
  for infinities and NaNs: negating `0.00` gives `-0.00`, and a NaN keeps its
  payload and stays quiet or signaling.
- Sign tests report the sign of every value, zeros, infinities and NaNs
  included (IEEE 754-2019 isSignMinus): `-0` has a negative sign but is not
  negative.

### Classification

- Tests for NaN, infinity, and finiteness, and the IEEE 754-2019 class as
  Rust's `FpCategory`: a nonzero finite value below 10^emin in magnitude, with
  emin = qmin + P − 1, is subnormal.

### Comparison

- Numeric comparison within and across widths, as the IEEE 754-2019
  comparison predicates: `1.0 == 1.00` and `-0 == +0`; −∞ and +∞ are below and
  above every finite value, and infinities of the same sign are equal. A NaN
  is unordered and equal to no value, itself included.
- The types are therefore partially ordered and provide neither total
  equality, total ordering, nor hashing.
- `OrderedDecimal` wraps a value of any type for use as a key, with the size
  and alignment of the value. Its equality, ordering, and hashing order
  −∞ < finite values < +∞ < NaN, treat every NaN as one key and numerically
  equal values, `-0` and `+0` included, as one key, and hash numerically equal
  values alike regardless of width.
- `total_cmp` is the IEEE 754-2019 totalOrder predicate.
- Zero, positive, and negative tests; a NaN is neither positive nor
  negative.

### Width Conversion

- Widening (`Decimal32` → `Decimal64` → `Decimal128`) is always exact and
  keeps the exponent. It keeps an infinity, and a NaN becomes a quiet NaN with
  its sign and payload.
- Narrowing returns the exact value when it is representable; otherwise it
  rounds in the specified direction or returns an error. It keeps an infinity,
  and a NaN becomes a quiet NaN with its sign and, if it fits, its payload.
- Without a direction, a finite value whose magnitude exceeds the target's
  largest finite value is an overflow error; any other value that needs more
  digits or a smaller exponent than the target provides is a
  not-exactly-representable error.

### Binary floating-point conversion

Each of `Decimal32`, `Decimal64`, and `Decimal128` provides four inherent
methods:

```text
to_f64(self) -> f64
to_f32(self) -> f32
from_f64(value: f64) -> Self
from_f32(value: f32) -> Self
```

They return values, not `Result` or `Option`, and expose no status flags or
rounding argument. They add no `From`, `Into`, `TryFrom`, or numeric-trait
implementations. They are manually written safe Rust, require neither
allocation nor a new dependency, and retain `no_std`.

For a finite decimal input, `to_f64` and `to_f32` round the exact value
`sign * coefficient * 10^exponent` once to the destination binary format,
nearest with ties to an even significand. `to_f32` rounds directly to binary32,
not through binary64. For example, decimal
`1.000000059604644775390625000000001` converts directly to f32 bits
`0x3f800001`; first rounding it to f64 and then to f32 would give `0x3f800000`.
Equal finite decimal cohorts have the same binary result.

For a finite binary input, `from_f64` and `from_f32` start from its exact
binary rational value, not its shortest printed decimal spelling. They round
once, nearest with ties to an even retained decimal coefficient, to the
following significant-digit limits:

| Destination | `from_f64`: min(P, 15) | `from_f32`: min(P, 6) |
|---|---|---|
| `Decimal32` | 7 | 6 |
| `Decimal64` | 15 | 6 |
| `Decimal128` | 15 | 6 |

Here P is the destination's full precision, and 15 and 6 are `f64::DIGITS`
and `f32::DIGITS`. The destination's minimum exponent participates in this
same rounding; rounding to the digit limit and then rounding again for
underflow is not allowed. A larger decimal destination does not retain
additional binary expansion digits beyond this limit.

The binary-to-decimal cohort rule is a project rule. For a nonzero finite
input, form its exact terminating decimal expansion `c * 10^q`, using `q=0`
for an integer and otherwise the greatest `q<0` for which `c` is an integer.
If `c` has L digits and the significant-digit limit is K, discard
`max(L-K, qmin-q, 0)` decimal places and round once. A carry beyond K digits
is absorbed by an exact division by ten and an exponent increment. An
exponent above `qmax` is fitted by exact coefficient zero padding if the
full destination precision P permits; otherwise the result overflows.
Finally apply the existing `trim_fraction_zeros` rule to the finite result.
It removes only fractional trailing zeros, not integer zeros at exponent zero
or coefficient zeros at a positive exponent. Every zero result has exponent
zero and retains the input sign, including a nonzero input rounded to zero.
This policy does not change the standalone trailing-zero methods.

Range is decided from the rounded result. Rounded overflow produces infinity
with the source sign. Gradual underflow produces a subnormal or a zero with
the source sign; exactly half the smallest positive destination value rounds
to zero. A finite source just beyond the destination's largest finite value
may still round to that value. Among binary-to-decimal conversions, only
`Decimal32::from_f64` can overflow or underflow the decimal range; Decimal64
and Decimal128 cover the finite binary64 range, and all three decimal widths
cover the finite binary32 range.

Both directions preserve the sign of zero and infinity. Every NaN becomes a
quiet NaN with the source sign. Treat the payload as an unsigned integer,
excluding the quiet/signaling discriminator: preserve it if it fits the
destination, otherwise use zero. The decimal limit is `payload < 10^(P-1)`;
the binary limits are `payload < 2^22` for f32 and `payload < 2^51` for f64.
This is a project mapping, not an IEEE cross-radix payload requirement. The
binary mapping uses the IEEE-2008 interchange quiet-bit convention: fraction
bit 22 for f32 or bit 51 for f64 is set. Rust documents this interpretation
for x86 and ARM and a historical signaling-polarity difference on MIPS;
bit transport does not assert identical hardware signaling behavior on
every target. No floating-point arithmetic is used to transport NaN fields.

Examples below state numerical results, not `Display` notation or a promise
to recover the source's original spelling:

| Binary input | Decimal result |
|---|---|
| `0.1_f64` | Decimal64 and Decimal128: `0.1` |
| `0.1_f64 + 0.2_f64` | Decimal64 and Decimal128: `0.3` |
| `86.296_f64` | Decimal64 and Decimal128: `86.296` |
| `1e20_f64` | Decimal64 and Decimal128: `1E+20` |
| `1.2345678901234568e17_f64` | Decimal64 and Decimal128: `1.23456789012346E+17` |
| `5e-324_f64` (the smallest positive f64 subnormal) | Decimal64 and Decimal128: `4.94065645841247E-324`; Decimal32: positive zero |
| `0.1_f32` | All three decimal widths: `0.1` |

The same digit rule applies to binary subnormals. Their available relative
precision is lower than in the normal range, so expansion noise can remain,
as the smallest f64 example shows. General binary-to-decimal-to-binary
bitwise round trips are not promised. Decimal-to-binary-to-decimal numerical
round trips apply to at most 15 significant digits for Decimal64/Decimal128
through f64, seven for Decimal32 through f64, and six for every width through
f32, within the common normal finite range. This is a numerical guarantee,
not preservation of the original decimal cohort. Zero sign is preserved
separately.

Arithmetic, parsing, quantize, decimal-width conversion, serialization, and
the trailing-zero value transformations retain their existing contracts.
In particular, serde continues to reject floating-point inputs. These methods
are the explicitly named interoperability path, not implicit conversion.

### Coefficient, exponent, mantissa, and scale

- Individual accessors use semantic names: `coefficient()` returns the stored
  signed coefficient; `mantissa()` is its alias, not a normalized fraction.
  `exponent()` returns the stored decimal exponent, and `scale()` its negation.
  All four return `Result`, rejecting infinity and NaN with `InvalidInput`.
  Trailing zeros and the exponent of zero are preserved. A negative-zero
  coefficient is integer zero; its sign is available through `is_sign_negative`.

- Pair APIs are `from_coefficient_and_exponent` / `to_coefficient_and_exponent`
  and `from_mantissa_and_scale` / `to_mantissa_and_scale`. Each constructor has
  a `_rounded` counterpart taking a rounding direction. Coefficient/mantissa
  is signed; value = coefficient × 10^exponent = mantissa × 10^(−scale).
  Exponent construction accepts the full `i32` input domain directly, including
  `i32::MIN`; it does not negate the exponent in `i32` arithmetic.
  Decimal32 uses i32 coefficients, Decimal64 uses i64, and Decimal128 uses
  i128; exponent and scale use i32.
- `from_coefficient_and_exponent` and its `_rounded` counterpart give the
  same bits or error as parsing `{coefficient}E{exponent}`, without or with
  the corresponding direction. The preferred exponent is exponent.
- `from_mantissa_and_scale` and `from_mantissa_and_scale_rounded` give the same bits or error as
  parsing the integer followed by `E` and the negated scale, without or with
  a direction respectively. The preferred exponent is −scale; precision and
  exponent limits follow narrowing, with at most one directed rounding.
  Integer and scale extrema follow the same rules without overflow in
  interpreting the inputs.
- `to_mantissa_and_scale` returns the stored signed coefficient and negated exponent
  of a finite value without removing trailing zeros: `1.00` gives `(100, 2)`.
  It never fails for a finite value; infinity and NaN are invalid input.
  Reconstructing from the pair preserves every finite bit except the sign
  of negative zero: its pair is `(0, scale)`, and reconstruction gives positive
  zero. The original sign remains available through `is_sign_negative`.

### Trailing-zero value transformations

Each decimal width provides `trim_fraction_zeros(self) -> Self` and
`strip_trailing_zeros(self) -> Self`, both marked `#[must_use]`. Both return
the same type, preserve the exact finite numeric value and sign, and do not
round, allocate, or return an error. Unlike `stripped()`, which is a formatting
adapter, these methods return a decimal value whose stored cohort may change.

`trim_fraction_zeros` removes trailing zeros only from the fractional portion
of a finite value. For a nonzero value with stored coefficient magnitude `c`
and exponent `q`, it divides `c` by the largest exact power `10^r` with
`r <= max(-q, 0)`, returning exponent `q+r`. A zero with negative exponent is
returned with exponent zero; a zero with a nonnegative exponent keeps its
exponent. Its sign is retained.

`strip_trailing_zeros` removes as many trailing coefficient zeros as the format
permits, including integer zeros. For a nonzero value it divides `c` by the
largest exact power `10^r` with `r <= qmax-q`, returning exponent `q+r`. It
stops at the format's greatest exponent and leaves any remaining coefficient
zeros, without error. A zero is returned with exponent zero and its original
sign. Here `qmax` is 90, 369, or 6111 for Decimal32, Decimal64, or Decimal128.

Both methods return an infinity unchanged. A NaN returns a quiet NaN with the
same sign and payload. Both methods are bitwise idempotent. Applying
`strip_trailing_zeros` after `trim_fraction_zeros` gives the same bits as
`strip_trailing_zeros` alone; applying `trim_fraction_zeros` after
`strip_trailing_zeros` leaves the latter's bits unchanged.

For example, both methods turn `1.5000` into coefficient 15 and exponent -1.
For `1500` at exponent zero, `trim_fraction_zeros` keeps coefficient 1500 and
exponent zero, while `strip_trailing_zeros` returns coefficient 15 and exponent
2; both results still display as `1500`. For `0E+5`, the first method keeps
exponent 5 and the second returns exponent zero, retaining either zero sign.
A negative signaling NaN with payload 7 becomes a negative quiet NaN with
payload 7 under either method.

The methods can change the stored cohort, so coefficient, exponent, scale,
raw bits, representation distinctions in `total_cmp`, and the preferred
exponents of subsequent arithmetic may change. Numeric comparisons and `OrderedDecimal`
keys and hashes do not change. Default `Display` and serde serialization of
either result have the same text as the existing stripped formatting of the
input. The unmodified input's default formatting and serialization continue
to preserve fractional zeros.

String round trips preserve the existing numeric-value and zero-sign
contract. They do not promise to retain `strip_trailing_zeros`' positive
exponent or NaN sign, payload, or signaling state.

### Parsing

- Converts a string to each type.
- Accepted syntax: an optional sign; a significand of decimal digits with at
  least one digit and an optional decimal point before, between, or after
  them; an optional exponent of `e` or `E`, an optional sign, and at least one
  digit. Infinity and NaN are spelled `inf`, `infinity`, and `nan` in any
  letter case, with an optional sign; there is no syntax for a signaling NaN
  or a payload. Anything else, including whitespace and digit separators, is
  invalid input.
- The written exponent is the preferred exponent: `1.50` has coefficient 150
  and exponent −2. The correctness contract applies to the parsed value, and
  an unrepresentable value is classified as in narrowing.
- Any number of digits and any exponent magnitude are accepted; the result is
  correct for every syntactically valid string.

### Formatting

- Writes the exact decimal value of each finite value in plain notation,
  without an exponent. An infinity is written `inf` or `-inf`, and a NaN
  `NaN`, without its sign and payload, as for `f64`.
- By default the value's own exponent is shown: coefficient 15000 with exponent
  −4 is `1.5000`; coefficient 15 with exponent 2 is `1500`.
- The `stripped()` formatting adapter strips trailing fractional zeros:
  `1.5000` → `1.5`, `2.000` → `2`, without changing the stored value. For
  decimal-value results, see
  [Trailing-zero value transformations](#trailing-zero-value-transformations).
- A negative zero keeps its sign: `-0`, `-0.00`.
- Parsing the output gives back an equal value, or a quiet NaN for a NaN.
  The output does not carry a positive exponent, so the parsed value's
  exponent follows the parsing rules.
- Formatting does not allocate. Before padding, the longest `Decimal128`
  output is 6,179 bytes.
- Width, fill, alignment, and sign flags of the formatter apply, to an
  infinity or NaN as for `f64`. Formatting never rounds, so a precision flag
  is ignored.

### Serialization

- The optional `serde` feature provides `Serialize` and `Deserialize` for
  `Decimal32`, `Decimal64`, `Decimal128`, and `OrderedDecimal<T>`, retaining
  `no_std` without requiring allocation or serde's `std` or `alloc` features.
- Serializers write the existing `Display` output as a string.
  Deserialization accepts strings through exact parsing and signed or unsigned
  integers through 128 bits with the same exactness and range rules as parsing.
  Because it accepts both, it asks the format for any value, so a
  format used for deserialization must support deserialize_any.
  Floating-point inputs are rejected with an instruction to send strings.
  Integer input width also depends on the backend: default serde_json uses
  native i64/u64 integer tokens and may report larger JSON numbers as floats.
  Send decimal strings for exact values outside those integer ranges.
  Parsing failures, including those of integer inputs, report serde errors
  that include the corresponding decimal error.
- String round trips preserve the numeric value, finite fractional cohorts,
  and zero signs. Positive exponents are not retained (`1E+5` is written as
  `"100000"`), and NaN sign, payload, and signaling state are lost, as in
  `Display` and parsing.
- `OrderedDecimal<T>` delegates to its wrapped value without adding a wrapper
  to the representation or normalizing equal keys.
- To serialize without fractional trailing zeros, serialize
  `x.trim_fraction_zeros()` using the existing implementation. The two
  [trailing-zero value transformations](#trailing-zero-value-transformations)
  have the same stripped text; neither adds a serde helper nor a `Serialize`
  implementation for `Stripped<T>`.

### Database integration

Database adapters are optional, versioned features for Decimal32, Decimal64,
and Decimal128. With all database features disabled, the core remains `no_std`
with no runtime dependencies. The adapters do not store BID encodings, use
binary floating-point intermediates, or depend on another decimal crate.

Reads succeed only when the numeric value is exactly representable. A database
fractional scale is preserved when possible; otherwise the same closest exact
cohort policy as parsing applies. The scale of a zero can be clamped to the
destination exponent range.

MySQL adapters (`sqlx-0_9-mysql`, `diesel-2-mysql`, and `mysql_common-0_37`)
write fixed-point decimal text. SQLx and mysql_common bind string-tagged
parameters. Accepted values have a minimum fractional scale s <= 30 and
integer digit count i such that i + s <= 65. The original scale is retained
when it fits; otherwise fractional trailing zeros are removed and the minimum
fractional scale is used. NaN and infinities are rejected. mysql_common writing
is an explicit fallible `TryFrom<DecimalN> for Value`. Integer `Value::Int` and
`Value::UInt` inputs are read exactly. Its read intermediate retains the original
Value for rollback and rejects Float and Double values.

PostgreSQL adapters (`sqlx-0_9-postgres`, `diesel-2-postgres`, and
`postgres-types-0_2`) exchange scalar NUMERIC through its external binary
base-10000 representation. SQLx also accepts NUMERIC text results. The binary
codec validates framing, every digit group, sign and scale. For finite
values it rejects nonzero fractional digits hidden beyond the declared scale.
For special values, valid weight, scale and digit-body fields have no numeric
meaning and are ignored after validation. Canonical positive payload-free quiet
NaN and both infinities are supported; other NaN encodings are rejected on
output. PostgreSQL zero is normalized to positive zero while its representable
scale is preserved. Arrays and domains are outside this adapter contract.

Adapters never round on the client. Database column precision/scale, CASTs,
functions, arithmetic and triggers can change a value under server rules.
SQL DECIMAL/NUMERIC storage normalizes negative zero to positive zero and
cannot preserve the original positive-exponent cohort or input spelling.
Errors travel through the driver's fallible error path rather than panics.

Successful shared conversion performs no temporary heap allocations. Driver
buffers may allocate. The library's Diesel feature does not link a native
client.

## Verification

Correctness evidence for every operation consists of all of the following,
kept in the repository:

1. For each integer routine an operation relies on (for example division by a
   power of ten), a proof that it returns the exact result for every input in
   its stated domain and never overflows, with the premises checked by
   compile-time evaluation or finite enumeration.
2. Exhaustive enumeration of all 2^32 `Decimal32` bit patterns for decoding,
   widening, and the format/parse round trip, and for quantize at every
   target exponent from −12 to 0 and at both ends of the exponent range, in
   both directions, checked against the reference in item 3 or, for the round
   trip, against the original value (a quiet NaN for a NaN).
3. Differential testing against an arbitrary-precision reference that shares
   no code with the implementation under test, including bit decoding.
4. Enumeration of boundary and cohort classes for every width, of the
   infinity and NaN classes of every operand, and of every error path, with a
   written argument that the classes cover every input: each
   input belongs to a class whose members take the same path and whose result
   follows from the tested members.

A reduced sample is a preliminary observation, not evidence.

## Non-goals

- IEEE 754-2019 status flags and alternate exception handling.
- Caller-selectable rounding directions other than roundTowardNegative and
  roundTowardPositive, apart from the fixed nearest-even rule of binary
  floating-point conversion.
- General arbitrary-precision division and remainder.

## Implementation Requirements

- Pure Rust. The library itself contains no FFI and links no C library or
  native code. Optional database features add only Rust crates and link no
  native database client; those crates' own dependencies may call
  operating-system APIs through binding crates such as `libc`.
- Targets current stable Rust and makes full use of recent standard-library
  integer primitives: wide multiplication, bit width, unbounded shifts, and
  allocation-free integer formatting. Each use is justified by generated
  machine code and measurement on the production path.
- A performance comparison keeps operation, operands, rounding direction, and
  build profile identical between the compared builds, and reports the
  platform, sample count, and timing distribution.
