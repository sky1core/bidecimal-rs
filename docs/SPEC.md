# bidecimal Specification

This file is the contract for the repository. Code and tests follow it; when
they disagree, the code is wrong.

## Goal

bidecimal provides fixed-width IEEE 754-2019 decimal types — `Decimal32`,
`Decimal64`, and `Decimal128` — whose operations return an exact result, a
single rounding in a direction the caller names, an IEEE 754-2019 infinity or
NaN result, or an error.

The project succeeds when all of the following hold:

1. **Exact or explicit.** Every supported operation returns the exact result,
   a result rounded in a direction the caller named, the infinity or NaN result
   defined below, or an error. Nothing is rounded, truncated, or overflowed
   silently.
2. **Proven correct.** Correctness is backed by evidence that covers every
   supported input. A count of passing tests is not that evidence, and neither
   is having ported another implementation such as Intel BID or IBM decNumber.
3. **Compact.** Each value occupies exactly 4, 8, or 16 bytes.
4. **Pure Rust.** No C library and no FFI at run time.

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
   returns it.
2. Otherwise the operation rounds in the direction the caller specified or
   returns an error. There is no default rounding. Addition, subtraction,
   multiplication, division, narrowing conversion, and parsing can each be
   called without a direction (an unrepresentable result is an error) or with
   one (the result is rounded). Quantize and rounding to a multiple always
   take a direction.
3. The rounding directions are roundTowardNegative and roundTowardPositive.
4. Without a direction, overflow is an error: the exact result exceeds the
   largest finite value of the result type in magnitude. With a direction,
   overflow is determined as in IEEE 754-2019 — the result, rounded in that
   direction as if the exponent range were unbounded, exceeds the largest
   finite value of the result type — and gives the IEEE 754-2019 overflow
   result: toward positive, +∞ for a positive result and the most negative
   finite value for a negative one; toward negative, the largest finite value
   for a positive result and −∞ for a negative one. Rounding to a multiple is
   the exception: its overflow is an error in either direction.
5. No operation panics, aborts, or wraps. Failures are reported through
   `Result`, and errors distinguish at least: not exactly representable,
   overflow, division by zero, and invalid input.
6. The exponent of a result follows IEEE 754-2019: an exact result uses the
   preferred exponent of the operation, or the representable exponent closest
   to it; a rounded result uses the least possible exponent.
7. The sign of a zero result follows IEEE 754-2019. Where IEEE 754-2019 makes
   that sign depend on the rounding direction, an operation called without a
   direction gives +0.

## Infinity and NaN

- Operations give the IEEE 754-2019 results for infinity and NaN operands, as
  listed for each operation below. Only the values are provided: the IEEE
  754-2019 status flags are not.
- A NaN operand gives a quiet NaN with the sign and payload of the first NaN
  operand. A payload that is not below 10^(P−1) for the result type becomes
  zero. A signaling NaN operand gives a quiet NaN like a quiet one; only the
  sign operations and decoding bits keep a signaling NaN.
- An invalid operation gives the positive quiet NaN with payload zero:
  `∞ − ∞` for infinities of the same sign (and `+∞ + −∞`), `0 × ∞`, `0 ÷ 0`,
  `∞ ÷ ∞`, and the cases listed under each operation.
- An operation without a direction whose finite result is not representable
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
- An option strips trailing fractional zeros: `1.5000` → `1.5`, `2.000` → `2`.
- A negative zero keeps its sign: `-0`, `-0.00`.
- Parsing the output gives back an equal value, or a quiet NaN for a NaN.
  The output does not carry a positive exponent, so the parsed value's
  exponent follows the parsing rules.
- Formatting does not allocate. Before padding, the longest `Decimal128`
  output is 6,179 bytes.
- Width, fill, alignment, and sign flags of the formatter apply, to an
  infinity or NaN as for `f64`. Formatting never rounds, so a precision flag
  is ignored.

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
- Rounding directions other than roundTowardNegative and roundTowardPositive.
- General arbitrary-precision division and remainder.

## Implementation Requirements

- Pure Rust. No C library, FFI, or native code is linked into the library.
- Targets current stable Rust and makes full use of recent standard-library
  integer primitives: wide multiplication, bit width, unbounded shifts, and
  allocation-free integer formatting. Each use is justified by generated
  machine code and measurement on the production path.
- A performance comparison keeps operation, operands, rounding direction, and
  build profile identical between the compared builds, and reports the
  platform, sample count, and timing distribution.
