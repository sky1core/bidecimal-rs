#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use core::cmp::Ordering;
use core::hash::Hasher;
use core::num::FpCategory;
use core::ops::Neg;
use core::{fmt, str::FromStr};

use crate::kernel::{self, Wide};
use crate::layout::{self, InRange, Unpacked, W32, W64, W128, Width};
use crate::operand::sealed::{Class, Sealed};
use crate::ordered::sealed::Keyed;
use crate::{Error, Operand, Rounding};

#[inline]
fn add_sub_general<W: Width, A: Operand, B: Operand, const SUBTRACT: bool>(
    x: Unpacked,
    y: Unpacked,
    dir: Option<Rounding>,
) -> Result<InRange<W>, Error> {
    if W::LAYOUT.digits < 34 && A::IS_NARROW && B::IS_NARROW {
        crate::narrow::add_sub_general::<W>(x, y, SUBTRACT, dir)
    } else {
        kernel::add_sub_general::<W>(x, y, SUBTRACT, dir)
    }
}

#[cfg(not(verus_keep_ghost))]
const _: () = {
    assert!(<Decimal32 as Sealed>::IS_NARROW);
    assert!(<Decimal64 as Sealed>::IS_NARROW);
    assert!(!<Decimal128 as Sealed>::IS_NARROW);
    assert!(W32::LAYOUT.digits < 34);
    assert!(W64::LAYOUT.digits < 34);
    assert!(W128::LAYOUT.digits >= 34);
};

/// Formats a value with trailing fractional zeros removed.
///
/// Returned by `stripped` on each decimal type. `Display` and `Debug` write
/// the same output as the value's own `Display`, except that trailing zeros
/// after the decimal point are removed: `1.5000` is written as `1.5`, `2.000`
/// as `2`, and `-0.00` as `-0`. Zeros before the decimal point are kept.
pub struct Stripped<T>(T);

/// Whether the exact sum `x + y`, or the difference `x − y` with `subtract`,
/// is negative, for a nonzero result.
#[cold]
fn sum_is_negative(x: Unpacked, y: Unpacked, subtract: bool) -> bool {
    let y_neg = y.neg != subtract;
    if x.neg == y_neg {
        return x.neg;
    }
    let magnitude = |v: Unpacked| Unpacked { neg: false, ..v };
    if kernel::cmp::<W128>(magnitude(x), magnitude(y)) == Ordering::Less {
        y_neg
    } else {
        x.neg
    }
}

macro_rules! decimal_type {
    ($name:ident, $bits:ty, $int:ty, $bytes:literal, $width:ty, $decode:ident, $unpack:ident, $compare_unpack:ident, $encode:ident, parse: $parse:ident, identical_bits_shortcut: $identical_bits_shortcut:literal) => {
        #[doc = concat!("A decimal value in the ", $bytes, "-byte IEEE 754-2019 BID interchange encoding: a finite number, an infinity, or NaN.")]
        ///
        /// Precision and exponent range are given by [`DIGITS`](Self::DIGITS),
        /// [`MIN_EXP`](Self::MIN_EXP), and [`MAX_EXP`](Self::MAX_EXP). Every
        /// representation of every finite value is supported, including zeros
        /// of either sign, and so are both infinities and quiet and signaling
        /// NaNs with payloads.
        ///
        /// # Infinity and NaN
        ///
        /// Operations give the IEEE 754-2019 results for infinity and NaN
        /// operands; status flags are not provided. Apart from the sign
        /// operations, which change only the sign bit, a NaN operand gives a
        /// quiet NaN with the sign and payload of the first NaN operand, and a
        /// payload that does not fit the result type becomes zero. Only the
        /// sign operations and [`from_bits`](Self::from_bits) keep a signaling
        /// NaN. Invalid operations such as `∞ − ∞`, `0 × ∞`, `0 ÷ 0`, and
        /// `∞ ÷ ∞` give [`NAN`](Self::NAN). An exact operation whose result is
        /// not representable still returns an error, never NaN.
        ///
        /// # Comparison
        ///
        /// Equality and ordering are numeric, within and across widths:
        /// `1.0 == 1.00` and `-0 == +0`, and `-∞` and `+∞` are below and above
        /// every finite value. A NaN is unordered and equal to no value, itself
        /// included, so the types implement neither `Eq`, `Ord`, nor `Hash`.
        /// [`OrderedDecimal`](crate::OrderedDecimal) provides them for use as a
        /// key, and [`total_cmp`](Self::total_cmp) is the IEEE 754-2019
        /// totalOrder.
        ///
        /// # Sign
        ///
        /// Negation (`-x`), [`abs`](Self::abs), and [`copysign`](Self::copysign)
        /// change only the sign bit, so they are exact and never fail, also for
        /// infinities and NaNs: negating `0.00` gives `-0.00`, and a NaN keeps
        /// its payload and stays quiet or signaling.
        ///
        /// # Formatting
        ///
        /// `Display` writes the exact value in plain notation without an
        /// exponent, using the value's own exponent: coefficient 15000 with
        /// exponent −4 is `1.5000`, and coefficient 15 with exponent 2 is
        /// `1500`. A negative zero keeps its sign: `-0`, `-0.00`. Width, fill,
        /// alignment, and sign flags apply; the precision flag is ignored,
        /// because formatting never rounds. Formatting does not allocate.
        /// `Debug` writes the same output as `Display`. An infinity is
        /// written `inf` or `-inf`, and a NaN `NaN` without its sign and
        /// payload. Parsing the output written without formatter flags gives
        /// back an equal value, or a quiet NaN for a NaN.
        /// [`stripped`](Self::stripped) removes
        /// trailing fractional zeros.
        #[repr(transparent)]
        #[derive(Clone, Copy)]
        pub struct $name($bits);

        const _: () = assert!(core::mem::size_of::<$name>() == $bytes);

        impl $name {
            const SIGN_BIT: $bits = 1 << (<$bits>::BITS - 1);

            /// Number of significant decimal digits of the coefficient.
            pub const DIGITS: u32 = <$width as Width>::LAYOUT.digits;
            /// Least exponent q of a value written as coefficient × 10^q.
            pub const MIN_EXP: i32 = <$width as Width>::LAYOUT.qmin;
            /// Greatest exponent q of a value written as coefficient × 10^q.
            pub const MAX_EXP: i32 = <$width as Width>::LAYOUT.qmax;
            /// Positive zero with exponent 0.
            pub const ZERO: Self = Self(layout::$encode(
                InRange::checked(false, 0, 0).expect("in range"),
            ));
            /// The largest finite value: (10^[`DIGITS`](Self::DIGITS) − 1) ×
            /// 10^[`MAX_EXP`](Self::MAX_EXP).
            pub const MAX: Self = Self(layout::$encode(
                InRange::checked(false, 10_u128.pow(Self::DIGITS) - 1, Self::MAX_EXP as i128)
                    .expect("in range"),
            ));
            /// The least finite value: the negation of [`MAX`](Self::MAX).
            pub const MIN: Self = Self(layout::$encode(
                InRange::checked(true, 10_u128.pow(Self::DIGITS) - 1, Self::MAX_EXP as i128)
                    .expect("in range"),
            ));
            /// A quiet NaN with a positive sign and a zero payload.
            pub const NAN: Self = Self(Self::NAN_BITS);
            /// Positive infinity.
            pub const INFINITY: Self = Self(Self::INF_BITS);
            /// Negative infinity.
            pub const NEG_INFINITY: Self = Self(Self::SIGN_BIT | Self::INF_BITS);

            const INF_BITS: $bits = 0b11110 << (<$bits>::BITS - 6);
            const NAN_BITS: $bits = 0b11111 << (<$bits>::BITS - 6);
            const SIGNALING_BIT: $bits = 1 << (<$bits>::BITS - 7);
            const PAYLOAD_MASK: $bits = (1 << <$width as Width>::LAYOUT.trail_bits) - 1;
            const COEFFICIENT_MASK: $bits = (1 << (<$width as Width>::LAYOUT.trail_bits + 3)) - 1;
            const MAX_PAYLOAD: $bits = (10_u128.pow(Self::DIGITS - 1) - 1) as $bits;
            const EMIN: i32 = Self::MIN_EXP + Self::DIGITS as i32 - 1;

            /// Returns the canonical BID encoding of this value.
            pub const fn to_bits(self) -> $bits {
                self.0
            }

            fn comparison_parts(self) -> Unpacked {
                layout::$compare_unpack(self.0)
            }

            const fn is_special(self) -> bool {
                self.0 & Self::INF_BITS == Self::INF_BITS
            }

            /// The fields of a finite value; meaningless for an infinity or NaN.
            #[inline]
            fn unpack(self) -> Unpacked {
                layout::$unpack(self.0)
            }

            #[inline]
            fn class(self) -> Class {
                if !self.is_special() {
                    Class::Finite(self.unpack())
                } else if self.0 & Self::NAN_BITS == Self::NAN_BITS {
                    Class::Nan {
                        neg: self.is_sign_negative(),
                        payload: self.payload(),
                    }
                } else {
                    Class::Infinite(self.is_sign_negative())
                }
            }

            /// A quiet NaN with `payload` if the payload fits this type, and
            /// with a zero payload otherwise.
            fn nan(neg: bool, payload: u128) -> Self {
                let payload = <$bits>::try_from(payload)
                    .ok()
                    .filter(|&p| p <= Self::MAX_PAYLOAD)
                    .unwrap_or(0);
                let sign = if neg { Self::SIGN_BIT } else { 0 };
                Self(sign | Self::NAN_BITS | payload)
            }

            /// The quiet NaN with `payload`, kept if it fits, or the infinity
            /// of this type, with sign `neg`.
            #[cold]
            #[inline(never)]
            fn special_of(nan: bool, neg: bool, payload: u128) -> Self {
                if nan { Self::nan(neg, payload) } else { Self::infinite(neg) }
            }

            fn payload(self) -> u128 {
                (self.0 & Self::PAYLOAD_MASK) as u128
            }

            fn infinite(neg: bool) -> Self {
                if neg { Self::NEG_INFINITY } else { Self::INFINITY }
            }

            /// A zero at the least exponent, the IEEE 754-2019 result of a
            /// finite value divided by an infinity.
            fn zero_at_min_exp(neg: bool) -> Self {
                Self(layout::$encode(InRange::unchecked(Unpacked {
                    neg,
                    coeff: 0,
                    exp: Self::MIN_EXP,
                })))
            }

            /// The result of a finite operation whose sign would be `neg`:
            /// an overflow with a direction becomes the IEEE 754-2019
            /// infinity or largest finite value for that direction and sign.
            fn finish(
                result: Result<InRange<$width>, Error>,
                neg: bool,
                dir: Option<Rounding>,
            ) -> Result<Self, Error> {
                match result {
                    Ok(v) => Ok(Self(layout::$encode(v))),
                    Err(Error::Overflow) => Self::overflow(neg, dir),
                    Err(e) => Err(e),
                }
            }

            /// An overflowed result with sign `neg`: [`Error::Overflow`]
            /// without a direction, and the IEEE 754-2019 overflow result for
            /// a direction.
            fn overflow(neg: bool, dir: Option<Rounding>) -> Result<Self, Error> {
                match dir {
                    None => Err(Error::Overflow),
                    Some(Rounding::TowardPositive) => Ok(if neg { Self::MIN } else { Self::INFINITY }),
                    Some(Rounding::TowardNegative) => {
                        Ok(if neg { Self::NEG_INFINITY } else { Self::MAX })
                    }
                }
            }

            fn fmt_value(self, strip: bool, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self.class() {
                    Class::Finite(v) => crate::display::fmt(v, strip, f),
                    Class::Nan { .. } => crate::display::fmt_special("", "NaN", f),
                    Class::Infinite(neg) => crate::display::fmt_special(
                        if neg {
                            "-"
                        } else if f.sign_plus() {
                            "+"
                        } else {
                            ""
                        },
                        "inf",
                        f,
                    ),
                }
            }

            /// Returns `true` if this value is NaN.
            pub const fn is_nan(self) -> bool {
                self.0 & Self::NAN_BITS == Self::NAN_BITS
            }

            /// Returns `true` if this value is positive or negative infinity.
            pub const fn is_infinite(self) -> bool {
                self.0 & Self::NAN_BITS == Self::INF_BITS
            }

            /// Returns `true` if this value is neither infinite nor NaN.
            pub const fn is_finite(self) -> bool {
                self.0 & Self::INF_BITS != Self::INF_BITS
            }

            /// Returns the floating-point category of this value. A nonzero
            /// finite value is subnormal when its magnitude is below
            /// 10^emin, with emin = [`MIN_EXP`](Self::MIN_EXP) +
            /// [`DIGITS`](Self::DIGITS) − 1 as in IEEE 754-2019.
            pub fn classify(self) -> FpCategory {
                match self.class() {
                    Class::Nan { .. } => FpCategory::Nan,
                    Class::Infinite(_) => FpCategory::Infinite,
                    Class::Finite(v) if v.coeff == 0 => FpCategory::Zero,
                    Class::Finite(v) => {
                        let adjusted = (kernel::digits(v.coeff) as i32)
                            .saturating_sub(1)
                            .saturating_add(v.exp);
                        if adjusted < Self::EMIN {
                            FpCategory::Subnormal
                        } else {
                            FpCategory::Normal
                        }
                    }
                }
            }

            /// The IEEE 754-2019 totalOrder predicate as an ordering:
            /// `-NaN < -∞ < negative finite values < -0 < +0 < positive finite
            /// values < +∞ < +NaN`; equal finite values are ordered by
            /// exponent, with the smaller exponent first for positive values;
            /// a positive signaling NaN is ordered before a positive quiet NaN
            /// and a negative quiet NaN before a negative signaling NaN, and
            /// NaNs of the same kind and sign by payload, with the smaller
            /// payload first for positive NaNs.
            pub fn total_cmp(&self, other: &Self) -> Ordering {
                let (a_neg, b_neg) = (self.is_sign_negative(), other.is_sign_negative());
                if a_neg != b_neg {
                    return if a_neg { Ordering::Less } else { Ordering::Greater };
                }
                let magnitude = match (self.class(), other.class()) {
                    (Class::Finite(x), Class::Finite(y)) => kernel::cmp::<$width>(
                        Unpacked { neg: false, ..x },
                        Unpacked { neg: false, ..y },
                    )
                    .then(x.exp.cmp(&y.exp)),
                    (Class::Finite(_), _) => Ordering::Less,
                    (_, Class::Finite(_)) => Ordering::Greater,
                    (Class::Infinite(_), Class::Infinite(_)) => Ordering::Equal,
                    (Class::Infinite(_), Class::Nan { .. }) => Ordering::Less,
                    (Class::Nan { .. }, Class::Infinite(_)) => Ordering::Greater,
                    (Class::Nan { payload: p, .. }, Class::Nan { payload: q, .. }) => {
                        let quiet = |v: &Self| v.0 & Self::SIGNALING_BIT == 0;
                        quiet(self).cmp(&quiet(other)).then(p.cmp(&q))
                    }
                };
                if a_neg { magnitude.reverse() } else { magnitude }
            }

            /// Returns `true` for a zero of either sign.
            pub const fn is_zero(self) -> bool {
                let form_mask = (Self::SIGN_BIT >> 1) | (Self::SIGN_BIT >> 2);
                (self.0 & form_mask != form_mask) & (self.0 & Self::COEFFICIENT_MASK == 0)
            }

            /// Returns `true` if the value is greater than zero. Zeros of
            /// either sign are not positive.
            pub fn is_positive(self) -> bool {
                match self.class() {
                    Class::Finite(v) => !v.neg && v.coeff != 0,
                    Class::Infinite(neg) => !neg,
                    Class::Nan { .. } => false,
                }
            }

            /// Returns `true` if the value is less than zero. Zeros of
            /// either sign, including `-0`, are not negative.
            pub fn is_negative(self) -> bool {
                match self.class() {
                    Class::Finite(v) => v.neg && v.coeff != 0,
                    Class::Infinite(neg) => neg,
                    Class::Nan { .. } => false,
                }
            }

            /// Returns `true` if the sign is negative, also for `-0`. Unlike
            /// [`is_negative`](Self::is_negative), this reports the sign of a
            /// zero.
            pub const fn is_sign_negative(self) -> bool {
                self.0 & Self::SIGN_BIT != 0
            }

            /// Returns `true` if the sign is positive, also for `+0`. Unlike
            /// [`is_positive`](Self::is_positive), this reports the sign of a
            /// zero.
            pub const fn is_sign_positive(self) -> bool {
                !self.is_sign_negative()
            }

            /// Returns this value with a positive sign and the same
            /// coefficient and exponent.
            pub const fn abs(self) -> Self {
                Self(self.0 & !Self::SIGN_BIT)
            }

            /// Returns this value with the sign of `sign` and the same
            /// coefficient and exponent.
            pub const fn copysign(self, sign: Self) -> Self {
                Self((self.0 & !Self::SIGN_BIT) | (sign.0 & Self::SIGN_BIT))
            }

            #[doc = concat!("Returns the exact sum `a + b` as a `", stringify!($name), "`.")]
            ///
            /// The operands may have any width, the same as or different from
            /// each other and from the result type.
            ///
            /// The result uses the preferred exponent, the smaller of the
            /// operand exponents, or the representable exponent closest to it.
            /// An exact zero sum is −0 if both operands are zeros with a
            /// negative sign, and +0 otherwise. An infinite operand gives that
            /// infinity, and `+∞ + −∞` gives [`NAN`](Self::NAN).
            ///
            /// # Errors
            ///
            /// - [`Error::Overflow`] if the magnitude of the exact sum exceeds
            ///   [`MAX`](Self::MAX).
            /// - Otherwise [`Error::Inexact`] if the exact sum is not
            ///   representable.
            pub fn add(a: impl Operand, b: impl Operand) -> Result<Self, Error> {
                Self::add_sub::<false, _, _>(a, b, None)
            }

            #[doc = concat!("Returns the sum `a + b` as a `", stringify!($name), "`, rounded in direction `dir` if the exact sum is not representable.")]
            ///
            /// The operands may have any width, the same as or different from
            /// each other and from the result type.
            ///
            /// An exact result uses the preferred exponent, the smaller of the
            /// operand exponents, or the representable exponent closest to it;
            /// a rounded result uses the least possible exponent. An exact zero
            /// sum of two zeros with the same sign keeps that sign; any other
            /// exact zero sum is −0 when `dir` is
            /// [`Rounding::TowardNegative`] and +0 when it is
            /// [`Rounding::TowardPositive`]. A nonzero sum rounded to zero
            /// keeps the sign of the sum. Infinite operands give the results
            /// of [`add`](Self::add).
            ///
            /// A sum, rounded in direction `dir` as if the exponent range
            /// were unbounded, that exceeds [`MAX`](Self::MAX) in magnitude
            /// gives the IEEE 754-2019 overflow result: +∞, or
            /// [`MIN`](Self::MIN) when negative, for
            /// [`Rounding::TowardPositive`], and [`MAX`](Self::MAX), or −∞
            /// when negative, for [`Rounding::TowardNegative`].
            pub fn add_rounded(
                a: impl Operand,
                b: impl Operand,
                dir: Rounding,
            ) -> Result<Self, Error> {
                Self::add_sub::<false, _, _>(a, b, Some(dir))
            }

            #[doc = concat!("Returns the exact difference `a - b` as a `", stringify!($name), "`.")]
            ///
            /// The operands may have any width, the same as or different from
            /// each other and from the result type.
            ///
            /// The result uses the preferred exponent, the smaller of the
            /// operand exponents, or the representable exponent closest to it.
            /// An exact zero difference is −0 if `a` is −0 and `b` is +0, and
            /// +0 otherwise. An infinite `a` gives that infinity, an infinite
            /// `b` with a finite `a` gives the infinity of the opposite sign,
            /// and the difference of infinities with the same sign gives
            /// [`NAN`](Self::NAN).
            ///
            /// # Errors
            ///
            /// - [`Error::Overflow`] if the magnitude of the exact difference
            ///   exceeds [`MAX`](Self::MAX).
            /// - Otherwise [`Error::Inexact`] if the exact difference is not
            ///   representable.
            pub fn sub(a: impl Operand, b: impl Operand) -> Result<Self, Error> {
                Self::add_sub::<true, _, _>(a, b, None)
            }

            #[doc = concat!("Returns the difference `a - b` as a `", stringify!($name), "`, rounded in direction `dir` if the exact difference is not representable.")]
            ///
            /// The operands may have any width, the same as or different from
            /// each other and from the result type.
            ///
            /// An exact result uses the preferred exponent, the smaller of the
            /// operand exponents, or the representable exponent closest to it;
            /// a rounded result uses the least possible exponent. An exact zero
            /// difference of zeros with opposite signs has the sign of `a`; any
            /// other exact zero difference is −0 when
            /// `dir` is [`Rounding::TowardNegative`] and +0 when it is
            /// [`Rounding::TowardPositive`]. A nonzero difference rounded to
            /// zero keeps the sign of the difference. Infinite operands give
            /// the results of [`sub`](Self::sub).
            ///
            /// A difference, rounded in direction `dir` as if the exponent range
            /// were unbounded, that exceeds [`MAX`](Self::MAX) in magnitude
            /// gives the IEEE 754-2019 overflow result: +∞, or
            /// [`MIN`](Self::MIN) when negative, for
            /// [`Rounding::TowardPositive`], and [`MAX`](Self::MAX), or −∞
            /// when negative, for [`Rounding::TowardNegative`].
            pub fn sub_rounded(
                a: impl Operand,
                b: impl Operand,
                dir: Rounding,
            ) -> Result<Self, Error> {
                Self::add_sub::<true, _, _>(a, b, Some(dir))
            }

            #[inline(always)]
            fn add_sub<const SUBTRACT: bool, A: Operand, B: Operand>(
                a: A,
                b: B,
                dir: Option<Rounding>,
            ) -> Result<Self, Error> {
                if (a.is_special() | b.is_special())
                    && let Some(special) = Self::add_sub_special(a, b, SUBTRACT)
                {
                    return Ok(special);
                }
                let x = a.unpack();
                let y = b.unpack();
                if let Some(v) = kernel::add_sub::<$width>(x, y, SUBTRACT, dir) {
                    return Ok(Self(layout::$encode(v)));
                }
                match add_sub_general::<$width, A, B, SUBTRACT>(x, y, dir) {
                    Ok(v) => Ok(Self(layout::$encode(v))),
                    Err(Error::Overflow) => Self::add_sub_overflow::<SUBTRACT>(a, b, dir),
                    Err(e) => Err(e),
                }
            }

            #[cold]
            #[inline(never)]
            fn add_sub_overflow<const SUBTRACT: bool>(
                a: impl Operand,
                b: impl Operand,
                dir: Option<Rounding>,
            ) -> Result<Self, Error> {
                Self::overflow(sum_is_negative(a.unpack(), b.unpack(), SUBTRACT), dir)
            }

            /// The result when an operand is an infinity or NaN; `None` for finite operands.
            #[cold]
            #[inline(never)]
            fn add_sub_special(a: impl Operand, b: impl Operand, subtract: bool) -> Option<Self> {
                match (a.class(), b.class()) {
                    (Class::Finite(_), Class::Finite(_)) => None,
                    (Class::Nan { neg, payload }, _) | (_, Class::Nan { neg, payload }) => {
                        Some(Self::nan(neg, payload))
                    }
                    (Class::Infinite(x), Class::Infinite(y)) => Some(if x == (y != subtract) {
                        Self::infinite(x)
                    } else {
                        Self::NAN
                    }),
                    (Class::Infinite(x), Class::Finite(_)) => Some(Self::infinite(x)),
                    (Class::Finite(_), Class::Infinite(y)) => Some(Self::infinite(y != subtract)),
                }
            }

            #[doc = concat!("Returns the exact product `a × b` as a `", stringify!($name), "`.")]
            ///
            /// The operands may have any width, the same as or different from
            /// each other and from the result type.
            ///
            /// The product is computed without intermediate error. The result
            /// uses the preferred exponent, the sum of the operand exponents,
            /// or the representable exponent closest to it. The sign of a zero
            /// result is negative exactly when the operand signs differ. An
            /// infinite operand gives the infinity with that sign, and `∞ × 0`
            /// gives [`NAN`](Self::NAN).
            ///
            /// # Errors
            ///
            /// - [`Error::Overflow`] if the magnitude of the exact product
            ///   exceeds [`MAX`](Self::MAX).
            /// - Otherwise [`Error::Inexact`] if the exact product is not
            ///   representable.
            pub fn mul(a: impl Operand, b: impl Operand) -> Result<Self, Error> {
                Self::mul_impl(a, b, None)
            }

            #[doc = concat!("Returns the product `a × b` as a `", stringify!($name), "`, rounded in direction `dir` if the exact product is not representable.")]
            ///
            /// The operands may have any width, the same as or different from
            /// each other and from the result type.
            ///
            /// The product is computed without intermediate error and rounded
            /// once. An exact result uses the preferred exponent, the sum of
            /// the operand exponents, or the representable exponent closest to
            /// it; a rounded result uses the least possible exponent. The sign
            /// of a zero result, exact or rounded, is negative exactly when the
            /// operand signs differ. Infinite operands give the results of
            /// [`mul`](Self::mul).
            ///
            /// A product, rounded in direction `dir` as if the exponent range
            /// were unbounded, that exceeds [`MAX`](Self::MAX) in magnitude
            /// gives the IEEE 754-2019 overflow result: +∞, or
            /// [`MIN`](Self::MIN) when negative, for
            /// [`Rounding::TowardPositive`], and [`MAX`](Self::MAX), or −∞
            /// when negative, for [`Rounding::TowardNegative`].
            pub fn mul_rounded(
                a: impl Operand,
                b: impl Operand,
                dir: Rounding,
            ) -> Result<Self, Error> {
                Self::mul_impl(a, b, Some(dir))
            }

            #[inline(always)]
            fn mul_impl(
                a: impl Operand,
                b: impl Operand,
                dir: Option<Rounding>,
            ) -> Result<Self, Error> {
                if (a.is_special() | b.is_special())
                    && let Some(special) = Self::mul_special(a, b)
                {
                    return Ok(special);
                }
                let (x, y) = (a.unpack(), b.unpack());
                Self::finish(kernel::mul::<$width>(x, y, dir), x.neg != y.neg, dir)
            }

            /// The result when an operand is an infinity or NaN; `None` for finite operands.
            #[cold]
            #[inline(never)]
            fn mul_special(a: impl Operand, b: impl Operand) -> Option<Self> {
                match (a.class(), b.class()) {
                    (Class::Finite(_), Class::Finite(_)) => None,
                    (Class::Nan { neg, payload }, _) | (_, Class::Nan { neg, payload }) => {
                        Some(Self::nan(neg, payload))
                    }
                    (Class::Infinite(x), Class::Infinite(y)) => Some(Self::infinite(x != y)),
                    (Class::Infinite(x), Class::Finite(y)) | (Class::Finite(y), Class::Infinite(x)) => {
                        Some(if y.coeff == 0 { Self::NAN } else { Self::infinite(x != y.neg) })
                    }
                }
            }

            #[doc = concat!("Returns the exact quotient `a ÷ b` as a `", stringify!($name), "`.")]
            ///
            /// The operands may have any width, the same as or different from
            /// each other and from the result type.
            ///
            /// The quotient is computed without intermediate error. The result
            /// uses the preferred exponent, the exponent of `a` minus the
            /// exponent of `b`, or the representable exponent closest to it:
            /// `1.00 ÷ 4` is `0.25`, and `1 ÷ 8` is `0.125`. The sign of a zero
            /// or infinite result is negative exactly when the operand signs
            /// differ. A nonzero finite `a` divided by zero and an infinite `a`
            /// divided by a finite `b` give an infinity, a finite `a` divided
            /// by an infinity gives zero at exponent
            /// [`MIN_EXP`](Self::MIN_EXP), and `0 ÷ 0` and `∞ ÷ ∞` give
            /// [`NAN`](Self::NAN).
            ///
            /// # Errors
            ///
            /// - [`Error::Overflow`] if the magnitude of the exact quotient
            ///   exceeds [`MAX`](Self::MAX).
            /// - Otherwise [`Error::Inexact`] if the exact quotient is not
            ///   representable, as for `1 ÷ 3`.
            pub fn div(a: impl Operand, b: impl Operand) -> Result<Self, Error> {
                Self::div_impl(a, b, None)
            }

            #[doc = concat!("Returns the quotient `a ÷ b` as a `", stringify!($name), "`, rounded in direction `dir` if the exact quotient is not representable.")]
            ///
            /// The operands may have any width, the same as or different from
            /// each other and from the result type.
            ///
            /// The quotient is computed without intermediate error and rounded
            /// once. An exact result uses the preferred exponent, the exponent
            /// of `a` minus the exponent of `b`, or the representable exponent
            /// closest to it; a rounded result uses the least possible
            /// exponent. The sign of a zero or infinite result, exact or
            /// rounded, is negative exactly when the operand signs differ.
            /// Zero and infinite operands give the results of
            /// [`div`](Self::div).
            ///
            /// A quotient, rounded in direction `dir` as if the exponent range
            /// were unbounded, that exceeds [`MAX`](Self::MAX) in magnitude
            /// gives the IEEE 754-2019 overflow result: +∞, or
            /// [`MIN`](Self::MIN) when negative, for
            /// [`Rounding::TowardPositive`], and [`MAX`](Self::MAX), or −∞
            /// when negative, for [`Rounding::TowardNegative`].
            pub fn div_rounded(
                a: impl Operand,
                b: impl Operand,
                dir: Rounding,
            ) -> Result<Self, Error> {
                Self::div_impl(a, b, Some(dir))
            }

            #[inline(always)]
            fn div_impl(
                a: impl Operand,
                b: impl Operand,
                dir: Option<Rounding>,
            ) -> Result<Self, Error> {
                if (a.is_special() | b.is_special())
                    && let Some(special) = Self::div_special(a, b)
                {
                    return Ok(special);
                }
                let (x, y) = (a.unpack(), b.unpack());
                let neg = x.neg != y.neg;
                if y.coeff == 0 {
                    Ok(if x.coeff == 0 { Self::NAN } else { Self::infinite(neg) })
                } else {
                    Self::finish(kernel::div::<$width>(x, y, dir), neg, dir)
                }
            }

            /// The result when an operand is an infinity or NaN; `None` for finite operands.
            #[cold]
            #[inline(never)]
            fn div_special(a: impl Operand, b: impl Operand) -> Option<Self> {
                match (a.class(), b.class()) {
                    (Class::Finite(_), Class::Finite(_)) => None,
                    (Class::Nan { neg, payload }, _) | (_, Class::Nan { neg, payload }) => {
                        Some(Self::nan(neg, payload))
                    }
                    (Class::Infinite(_), Class::Infinite(_)) => Some(Self::NAN),
                    (Class::Infinite(x), Class::Finite(y)) => Some(Self::infinite(x != y.neg)),
                    (Class::Finite(x), Class::Infinite(y)) => Some(Self::zero_at_min_exp(x.neg != y)),
                }
            }

            /// Constructs the exact value `mantissa × 10^(-scale)`.
            ///
            /// Scale is the negation of the exponent passed to
            /// [`quantize`](Self::quantize): `(1, -5)` represents `1E+5`.
            /// The preferred exponent is `-scale`. The result has the same
            /// bits as parsing the integer followed by `E` and the negated
            /// scale, without allocating or formatting a string.
            ///
            /// # Errors
            ///
            /// [`Error::Overflow`] if the magnitude exceeds [`MAX`](Self::MAX),
            /// otherwise [`Error::Inexact`] if the value is not representable.
            #[expect(clippy::arithmetic_side_effects, reason = "negating an i32 widened to i64 cannot overflow")]
            pub fn from_mantissa_and_scale(mantissa: $int, scale: i32) -> Result<Self, Error> {
                Self::from_coefficient_exponent_impl(mantissa, -i64::from(scale), None)
            }

            /// Constructs `mantissa × 10^(-scale)`, rounding once in direction
            /// `dir` if the value is not representable.
            ///
            /// Scale is the negation of the exponent passed to
            /// [`quantize`](Self::quantize). The preferred exponent is
            /// `-scale`; the result has the same bits as
            /// [`parse_rounded`](Self::parse_rounded) of the integer followed
            /// by `E` and the negated scale, including directed overflow.
            /// No string is allocated or formatted.
            #[expect(clippy::arithmetic_side_effects, reason = "negating an i32 widened to i64 cannot overflow")]
            pub fn from_mantissa_and_scale_rounded(
                mantissa: $int,
                scale: i32,
                dir: Rounding,
            ) -> Result<Self, Error> {
                Self::from_coefficient_exponent_impl(mantissa, -i64::from(scale), Some(dir))
            }

            /// Constructs the exact value `coefficient × 10^exponent`.
            /// The coefficient is signed. This is the exponent-sign counterpart
            /// of [`from_mantissa_and_scale`](Self::from_mantissa_and_scale).
            /// All `i32` exponents are accepted as inputs without negation overflow.
            /// Returns the same bits or error as parsing `{coefficient}E{exponent}`
            /// with [`parse`](Self::parse); the preferred exponent is `exponent`.
            ///
            /// # Errors
            ///
            /// [`Error::Overflow`] if the magnitude exceeds [`MAX`](Self::MAX),
            /// otherwise [`Error::Inexact`] if the value is not representable.
            pub fn from_coefficient_and_exponent(coefficient: $int, exponent: i32) -> Result<Self, Error> {
                Self::from_coefficient_exponent_impl(coefficient, i64::from(exponent), None)
            }

            /// Constructs `coefficient × 10^exponent`, rounding once in direction
            /// `dir` if necessary, including directed overflow.
            /// Equivalent to parsing the signed coefficient followed by `E`
            /// and the exponent with [`parse_rounded`](Self::parse_rounded).
            pub fn from_coefficient_and_exponent_rounded(
                coefficient: $int,
                exponent: i32,
                dir: Rounding,
            ) -> Result<Self, Error> {
                Self::from_coefficient_exponent_impl(coefficient, i64::from(exponent), Some(dir))
            }

            #[expect(
                clippy::arithmetic_side_effects,
                reason = "preferred is a widened i32 exponent or negated i32 scale, so the exponent step of one fits i64"
            )]
            fn from_coefficient_exponent_impl(
                coefficient: $int,
                preferred: i64,
                dir: Option<Rounding>,
            ) -> Result<Self, Error> {
                let magnitude = coefficient.unsigned_abs() as u128;
                if let Some(value) = InRange::<$width>::checked(coefficient < 0, magnitude, i128::from(preferred)) {
                    return Ok(Self(layout::$encode(value)));
                }
                let mut wide = Wide {
                    neg: coefficient < 0,
                    coeff: magnitude,
                    exp: preferred,
                    sticky: false,
                };
                if wide.coeff > i128::MAX as u128 {
                    wide.sticky = wide.coeff % 10 != 0;
                    wide.coeff /= 10;
                    wide.exp += 1;
                }
                Self::finish(kernel::fit(wide, preferred, dir), coefficient < 0, dir)
            }

            /// Returns the stored signed coefficient without removing trailing zeros.
            /// For `-123.450`, this is `-123450`. Negative zero returns zero;
            /// use [`is_sign_negative`](Self::is_sign_negative) for its sign.
            ///
            /// # Errors
            ///
            /// [`Error::InvalidInput`] for infinity or NaN.
            pub fn coefficient(self) -> Result<$int, Error> {
                self.to_coefficient_and_exponent().map(|(coefficient, _)| coefficient)
            }

            /// Alias for [`coefficient`](Self::coefficient), returning the
            /// same stored signed integer, not a normalized fraction.
            ///
            /// # Errors
            ///
            /// [`Error::InvalidInput`] for infinity or NaN.
            pub fn mantissa(self) -> Result<$int, Error> {
                self.coefficient()
            }

            /// Returns the stored decimal exponent: value = coefficient × 10^exponent.
            /// For `123.450`, this is `-3`; for `1E+5`, it is `5`.
            /// Zero retains its stored exponent.
            ///
            /// # Errors
            ///
            /// [`Error::InvalidInput`] for infinity or NaN.
            pub fn exponent(self) -> Result<i32, Error> {
                if self.is_special() {
                    return Err(Error::InvalidInput);
                }
                Ok(self.unpack().exp)
            }

            /// Returns the scale, the negation of [`exponent`](Self::exponent).
            /// For `123.450`, this is `3`; for `1E+5`, it is `-5`.
            /// Zero retains its stored scale.
            ///
            /// # Errors
            ///
            /// [`Error::InvalidInput`] for infinity or NaN.
            pub fn scale(self) -> Result<i32, Error> {
                self.to_mantissa_and_scale().map(|(_, scale)| scale)
            }

            /// Returns the stored signed coefficient and scale, without
            /// removing trailing zeros: `1.00` gives `(100, 2)` and `1E+5`
            /// gives `(1, -5)`. The value is `mantissa × 10^(-scale)`; scale is
            /// the negation of the exponent passed to [`quantize`](Self::quantize).
            ///
            /// Negative zero gives `(0, scale)`. Check its original sign with
            /// [`is_sign_negative`](Self::is_sign_negative); reconstructing
            /// with [`from_mantissa_and_scale`](Self::from_mantissa_and_scale) gives positive
            /// zero. Every other finite value round-trips with identical bits.
            ///
            /// # Errors
            ///
            /// [`Error::InvalidInput`] for infinity or NaN. Finite values
            /// always fit the returned integer and scale.
            #[expect(
                clippy::arithmetic_side_effects,
                reason = "the assertions bound the coefficient by the integer's MAX and the exponent above i32::MIN, so both negations fit"
            )]
            pub fn to_mantissa_and_scale(self) -> Result<($int, i32), Error> {
                self.to_coefficient_and_exponent().map(|(mantissa, exponent)| (mantissa, -exponent))
            }

            /// Returns the stored signed coefficient and decimal exponent.
            /// For `-123.450`, this is `(-123450, -3)`. Trailing zeros and
            /// the exponent of zero are preserved. Negative zero returns a
            /// zero coefficient; use [`is_sign_negative`](Self::is_sign_negative)
            /// to distinguish its sign.
            ///
            /// # Errors
            ///
            /// [`Error::InvalidInput`] for infinity or NaN.
            #[expect(
                clippy::arithmetic_side_effects,
                reason = "the static coefficient bound fits the signed integer, so negating its magnitude is safe"
            )]
            pub fn to_coefficient_and_exponent(self) -> Result<($int, i32), Error> {
                const {
                    assert!(10_u128.pow(Self::DIGITS) - 1 <= <$int>::MAX as u128);
                    assert!(Self::MIN_EXP > i32::MIN && Self::MAX_EXP > i32::MIN);
                }
                if self.is_special() {
                    return Err(Error::InvalidInput);
                }
                let value = self.unpack();
                let magnitude = value.coeff as $int;
                let int = if value.neg { -magnitude } else { magnitude };
                Ok((int, value.exp))
            }

            /// Parses a decimal string exactly.
            ///
            /// The accepted syntax is an optional sign (`+` or `-`); a
            /// significand of ASCII decimal digits with at least one digit
            /// and an optional decimal point before, between, or after them;
            /// and an optional exponent of `e` or `E`, an optional sign, and
            /// at least one digit. Any number of digits and any exponent
            /// magnitude are accepted.
            ///
            /// The result uses the preferred exponent, the written exponent
            /// minus the number of digits after the decimal point, or the
            /// representable exponent closest to it: `1.50` has coefficient
            /// 150 and exponent −2. A zero keeps the written sign.
            ///
            /// # Errors
            ///
            /// - [`Error::InvalidInput`] if `s` does not match the syntax,
            ///   including whitespace and digit separators. The special
            ///   values are spelled `inf`, `infinity`, and `nan` in any
            ///   letter case, with an optional sign. This is checked before
            ///   the value.
            /// - [`Error::Overflow`] if the magnitude of the value exceeds
            ///   [`MAX`](Self::MAX).
            /// - Otherwise [`Error::Inexact`] if the value is not
            ///   representable.
            pub fn parse(s: &str) -> Result<Self, Error> {
                Self::parse_impl(s, None)
            }

            /// Parses a decimal string, rounding in direction `dir` if the
            /// value is not representable.
            ///
            /// The syntax is the same as for [`parse`](Self::parse). An exact
            /// result uses the preferred exponent, the written exponent minus
            /// the number of digits after the decimal point, or the
            /// representable exponent closest to it; a rounded result uses the
            /// least possible exponent. A zero result, exact or rounded, has
            /// the written sign.
            ///
            /// A value, rounded in direction `dir` as if the exponent range
            /// were unbounded, that exceeds [`MAX`](Self::MAX) in magnitude
            /// gives the IEEE 754-2019 overflow result: +∞, or
            /// [`MIN`](Self::MIN) when negative, for
            /// [`Rounding::TowardPositive`], and [`MAX`](Self::MAX), or −∞
            /// when negative, for [`Rounding::TowardNegative`].
            ///
            /// # Errors
            ///
            /// [`Error::InvalidInput`] if `s` does not match the syntax.
            pub fn parse_rounded(s: &str, dir: Rounding) -> Result<Self, Error> {
                Self::parse_impl(s, Some(dir))
            }

            fn parse_impl(s: &str, dir: Option<Rounding>) -> Result<Self, Error> {
                match crate::parse::$parse(s, dir) {
                    Ok(v) => Ok(Self(layout::$encode(v))),
                    Err(Error::InvalidInput) => Self::parse_special(s),
                    Err(e) => Self::finish(Err(e), s.starts_with('-'), dir),
                }
            }

            /// Parses the infinity and NaN spellings, which the finite parser
            /// rejects as invalid input.
            #[cold]
            #[inline(never)]
            fn parse_special(s: &str) -> Result<Self, Error> {
                let (neg, body) = match s.as_bytes().first() {
                    Some(b'-') => (true, s.get(1..).unwrap_or_default()),
                    Some(b'+') => (false, s.get(1..).unwrap_or_default()),
                    _ => (false, s),
                };
                if body.eq_ignore_ascii_case("inf") || body.eq_ignore_ascii_case("infinity") {
                    return Ok(Self::infinite(neg));
                }
                if body.eq_ignore_ascii_case("nan") {
                    return Ok(Self::nan(neg, 0));
                }
                Err(Error::InvalidInput)
            }

            /// Returns a formatting adapter that writes this value with
            /// trailing fractional zeros removed: `1.5000` as `1.5`, `2.000`
            /// as `2`.
            pub fn stripped(self) -> Stripped<Self> {
                Stripped(self)
            }

            /// Decodes a BID encoding.
            ///
            /// Every canonical encoding is accepted, and
            /// [`to_bits`](Self::to_bits) returns the same bits. A canonical
            /// infinity has every bit after the combination field clear; a
            /// canonical NaN has every bit between the signaling bit and the
            /// trailing significand clear and a payload below
            /// 10^([`DIGITS`](Self::DIGITS) − 1).
            ///
            /// # Errors
            ///
            /// [`Error::InvalidInput`] if `bits` are a non-canonical encoding.
            pub fn from_bits(bits: $bits) -> Result<Self, Error> {
                if bits & Self::INF_BITS == Self::INF_BITS {
                    let canonical = if bits & Self::NAN_BITS == Self::NAN_BITS {
                        let fixed = Self::SIGN_BIT
                            | Self::NAN_BITS
                            | Self::SIGNALING_BIT
                            | Self::PAYLOAD_MASK;
                        bits & !fixed == 0 && bits & Self::PAYLOAD_MASK <= Self::MAX_PAYLOAD
                    } else {
                        bits & !Self::SIGN_BIT == Self::INF_BITS
                    };
                    return if canonical {
                        Ok(Self(bits))
                    } else {
                        Err(Error::InvalidInput)
                    };
                }
                layout::$decode(bits)?;
                Ok(Self(bits))
            }

            /// Returns this value at exponent `exp`, rounded in direction
            /// `dir` if it is not a multiple of 10^`exp`; `exp = -n` keeps `n`
            /// fractional digits.
            ///
            /// The result has exponent `exp` and the sign of `self`, also when
            /// it is zero. A NaN gives a quiet NaN with its sign and payload,
            /// and an infinity gives [`NAN`](Self::NAN).
            ///
            /// # Errors
            ///
            /// For a finite value, [`Error::InvalidInput`] if `exp` is outside
            /// [`MIN_EXP`](Self::MIN_EXP)`..=`[`MAX_EXP`](Self::MAX_EXP), or if
            /// the coefficient of the result at exponent `exp` has more than
            /// [`DIGITS`](Self::DIGITS) digits.
            pub fn quantize(self, exp: i32, dir: Rounding) -> Result<Self, Error> {
                if self.is_special()
                    && let Some(special) = self.quantize_special()
                {
                    return Ok(special);
                }
                let quantized =
                    kernel::quantize::<$width>(InRange::unchecked(self.unpack()), exp, dir)?;
                Ok(Self(layout::$encode(quantized)))
            }

            /// The result when an operand is an infinity or NaN; `None` for finite operands.
            #[cold]
            #[inline(never)]
            fn quantize_special(self) -> Option<Self> {
                match self.class() {
                    Class::Finite(_) => None,
                    Class::Nan { neg, payload } => Some(Self::nan(neg, payload)),
                    Class::Infinite(_) => Some(Self::NAN),
                }
            }

            /// Rounds this value to a multiple of `step`:
            /// `step × to_integer(self / step)`.
            ///
            /// `to_integer` is floor for [`Rounding::TowardNegative`] and
            /// ceiling for [`Rounding::TowardPositive`]. `step` may be any
            /// positive value. The quotient is computed exactly and rounded
            /// once, when it becomes an integer; the resulting multiple is then
            /// exact or an error, never rounded a second time.
            ///
            /// The result uses the preferred exponent, the exponent of `step`,
            /// or the representable exponent closest to it: `1.53` rounded
            /// toward negative to a multiple of `0.05` is `1.50`. A zero result
            /// has the sign of `self` and the exponent of `step`.
            ///
            /// A NaN `self` or `step` gives a quiet NaN, before any error. An
            /// infinite `self` with a finite `step` gives that infinity, and a
            /// `step` of +∞ gives [`NAN`](Self::NAN).
            ///
            /// # Errors
            ///
            /// In order of precedence:
            ///
            /// - [`Error::DivisionByZero`] if `step` is zero of either sign.
            /// - [`Error::InvalidInput`] if `step` is negative, including −∞.
            /// - [`Error::Overflow`] if the multiple exceeds
            ///   [`MAX`](Self::MAX) in magnitude.
            /// - Otherwise [`Error::Inexact`] if the multiple is not
            ///   representable, that is, it needs more than
            ///   [`DIGITS`](Self::DIGITS) significant digits. This cannot
            ///   happen when `step` is 10^k or 5 × 10^k for an integer k.
            pub fn round_to_multiple(self, step: Self, dir: Rounding) -> Result<Self, Error> {
                if (self.is_special() | step.is_special())
                    && let Some(special) = self.multiple_special(step)
                {
                    return special;
                }
                let rounded = kernel::round_to_multiple::<$width>(
                    InRange::unchecked(self.unpack()),
                    InRange::unchecked(step.unpack()),
                    dir,
                )?;
                Ok(Self(layout::$encode(rounded)))
            }

            /// The result when an operand is an infinity or NaN; `None` for finite operands.
            #[cold]
            #[inline(never)]
            fn multiple_special(self, step: Self) -> Option<Result<Self, Error>> {
                match (self.class(), step.class()) {
                    (Class::Nan { neg, payload }, _) | (_, Class::Nan { neg, payload }) => {
                        Some(Ok(Self::nan(neg, payload)))
                    }
                    (_, Class::Infinite(true)) => Some(Err(Error::InvalidInput)),
                    (_, Class::Finite(s)) if s.coeff == 0 => Some(Err(Error::DivisionByZero)),
                    (_, Class::Finite(s)) if s.neg => Some(Err(Error::InvalidInput)),
                    (Class::Infinite(neg), Class::Finite(_)) => Some(Ok(Self::infinite(neg))),
                    (_, Class::Infinite(false)) => Some(Ok(Self::NAN)),
                    (Class::Finite(_), Class::Finite(_)) => None,
                }
            }
        }

        impl FromStr for $name {
            type Err = Error;

            /// Same as [`parse`](Self::parse).
            fn from_str(s: &str) -> Result<Self, Error> {
                Self::parse(s)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.fmt_value(false, f)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(self, f)
            }
        }

        impl fmt::Display for Stripped<$name> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt_value(true, f)
            }
        }

        impl fmt::Debug for Stripped<$name> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(self, f)
            }
        }

        impl Sealed for $name {
            const IS_NARROW: bool = <$width>::LAYOUT.digits < 34;

            fn is_special(self) -> bool {
                $name::is_special(self)
            }

            #[expect(
                private_interfaces,
                reason = "Sealed cannot be named outside this crate, and type privacy rejects any external call that would produce an Unpacked"
            )]
            fn unpack(self) -> Unpacked {
                $name::unpack(self)
            }

            #[expect(
                private_interfaces,
                reason = "Sealed cannot be named outside this crate, and type privacy rejects any external call that would produce a Class"
            )]
            fn class(self) -> Class {
                $name::class(self)
            }
        }

        impl Operand for $name {}

        impl Keyed for $name {
            #[inline]
            fn key_eq(self, other: Self) -> bool {
                if $identical_bits_shortcut && self.0 == other.0 {
                    return true;
                }
                if self.is_special() | other.is_special() {
                    return crate::ordered::eq_keys::<$width>(self, other);
                }
                self.finite_eq(other)
            }

            #[inline]
            fn key_cmp(self, other: Self) -> Ordering {
                if $identical_bits_shortcut && self.0 == other.0 {
                    return Ordering::Equal;
                }
                if self.is_special() | other.is_special() {
                    return crate::ordered::cmp_keys::<$width>(self, other);
                }
                self.finite_cmp(other)
            }

            #[inline]
            fn key_hash<H: Hasher>(self, state: &mut H) {
                if self.is_zero() {
                    state.write_u128(crate::ordered::finite_hash_key(false, 0, 0));
                    return;
                }
                state.write_u128(self.canonical_hash_key());
            }
        }

        impl Neg for $name {
            type Output = Self;

            fn neg(self) -> Self {
                Self(self.0 ^ Self::SIGN_BIT)
            }
        }

        impl $name {
            #[inline(never)]
            fn canonical_hash_key(self) -> u128 {
                if self.is_special() {
                    crate::ordered::special_hash_key(self.is_nan(), self.is_sign_negative())
                } else {
                    let value = if Self::DIGITS > 16 {
                        self.comparison_parts()
                    } else {
                        self.unpack()
                    };
                    if Self::DIGITS > 16 && value.coeff >= 10_000_000_000_000_000 {
                        return crate::ordered::canonical_wide_hash_key(
                            value.neg,
                            value.coeff,
                            value.exp,
                        );
                    }
                    let (neg, coeff, exp) = kernel::canonical(value);
                    crate::ordered::finite_hash_key(neg, coeff, exp)
                }
            }

            #[inline]
            fn finite_eq(self, other: Self) -> bool {
                kernel::eq::<$width>(self.comparison_parts(), other.comparison_parts())
            }

            #[inline]
            fn finite_cmp(self, other: Self) -> Ordering {
                kernel::cmp::<$width>(self.comparison_parts(), other.comparison_parts())
            }
        }

        impl PartialEq for $name {
            #[inline]
            fn eq(&self, other: &Self) -> bool {
                if $identical_bits_shortcut && self.0 == other.0 {
                    return !self.is_nan();
                }
                if self.is_special() | other.is_special() {
                    return eq_by_class::<$width>(*self, *other);
                }
                self.finite_eq(*other)
            }
        }

        impl PartialOrd for $name {
            #[inline]
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                if $identical_bits_shortcut && self.0 == other.0 {
                    return if self.is_nan() { None } else { Some(Ordering::Equal) };
                }
                if self.is_special() | other.is_special() {
                    return cmp_by_class::<$width>(*self, *other);
                }
                Some(self.finite_cmp(*other))
            }
        }
    };
}

decimal_type!(
    Decimal32,
    u32,
    i32,
    4,
    W32,
    decode_32,
    unpack_32,
    unpack_32,
    encode_32,
    parse: parse_32,
    identical_bits_shortcut: true
);
decimal_type!(
    Decimal64,
    u64,
    i64,
    8,
    W64,
    decode_64,
    unpack_64,
    unpack_64,
    encode_64,
    parse: parse_64,
    identical_bits_shortcut: true
);
decimal_type!(
    Decimal128,
    u128,
    i128,
    16,
    W128,
    decode_128,
    unpack_canonical_128,
    unpack_canonical_128,
    encode_128,
    parse: parse,
    identical_bits_shortcut: false
);

/// IEEE 754-2019 equality, with finite values compared at width `W`: NaN is
/// equal to nothing, and infinities are equal when their signs are. The path
/// for an infinity or NaN operand.
#[cold]
#[inline(never)]
fn eq_by_class<W: Width>(a: impl Sealed, b: impl Sealed) -> bool {
    match (a.class(), b.class()) {
        (Class::Finite(x), Class::Finite(y)) => kernel::eq::<W>(x, y),
        (Class::Infinite(x), Class::Infinite(y)) => x == y,
        _ => false,
    }
}

/// IEEE 754-2019 ordering, with finite values compared at width `W`:
/// unordered with a NaN, and `-∞ < finite values < +∞` otherwise. The path for
/// an infinity or NaN operand.
#[cold]
#[inline(never)]
fn cmp_by_class<W: Width>(a: impl Sealed, b: impl Sealed) -> Option<Ordering> {
    Some(match (a.class(), b.class()) {
        (Class::Finite(x), Class::Finite(y)) => kernel::cmp::<W>(x, y),
        (Class::Nan { .. }, _) | (_, Class::Nan { .. }) => return None,
        (Class::Infinite(x), Class::Infinite(y)) => y.cmp(&x),
        (Class::Infinite(x), Class::Finite(_)) => {
            if x {
                Ordering::Less
            } else {
                Ordering::Greater
            }
        }
        (Class::Finite(_), Class::Infinite(y)) => {
            if y {
                Ordering::Greater
            } else {
                Ordering::Less
            }
        }
    })
}

fn narrow<W: Width>(value: Unpacked, dir: Option<Rounding>) -> Result<InRange<W>, Error> {
    kernel::fit(
        Wide {
            neg: value.neg,
            coeff: value.coeff,
            exp: i64::from(value.exp),
            sticky: false,
        },
        i64::from(value.exp),
        dir,
    )
}

macro_rules! width_pair {
    ($small:ident, $large:ident, $small_width:ty, $large_width:ty, $encode_small:ident, $encode_large:ident, $rounded:ident) => {
        impl From<$small> for $large {
            /// Widens exactly, keeping the sign, coefficient, and exponent. An
            /// infinity stays that infinity, and a NaN gives a quiet NaN with
            /// its sign and payload.
            #[inline]
            fn from(value: $small) -> Self {
                const {
                    assert!(
                        <$small_width as Width>::LAYOUT.digits <= <$large_width as Width>::LAYOUT.digits
                            && <$large_width as Width>::LAYOUT.qmin <= <$small_width as Width>::LAYOUT.qmin
                            && <$small_width as Width>::LAYOUT.qmax <= <$large_width as Width>::LAYOUT.qmax
                    )
                };
                if value.is_special() {
                    return Self::special_of(value.is_nan(), value.is_sign_negative(), value.payload());
                }
                Self(layout::$encode_large(InRange::unchecked(value.unpack())))
            }
        }

        impl TryFrom<$large> for $small {
            type Error = Error;

            /// Narrows exactly.
            ///
            /// The result has the value's sign and uses the value's exponent,
            /// or the representable exponent closest to it. An infinity stays
            /// that infinity, and a NaN gives a quiet NaN with its sign, and
            /// its payload if it fits.
            ///
            /// # Errors
            ///
            /// - [`Error::Overflow`] if the magnitude of the value exceeds the
            ///   largest finite value of the target type.
            /// - Otherwise [`Error::Inexact`] if the value needs more digits or
            ///   a smaller exponent than the target type provides.
            fn try_from(value: $large) -> Result<Self, Error> {
                if value.is_special() {
                    return Ok(Self::special_of(
                        value.is_nan(),
                        value.is_sign_negative(),
                        value.payload(),
                    ));
                }
                let v = value.unpack();
                Self::finish(narrow::<$small_width>(v, None), v.neg, None)
            }
        }

        impl $small {
            #[doc = concat!("Narrows a `", stringify!($large), "`, rounding in direction `dir` if the value is not representable.")]
            ///
            /// An exact result uses the value's exponent, or the representable
            /// exponent closest to it; a rounded result uses the least possible
            /// exponent. The result, including a zero, has the value's sign.
            /// Infinities and NaNs give the results of `try_from`.
            #[doc = concat!("Without a direction, use `", stringify!($small), "::try_from`.")]
            ///
            /// A value, rounded in direction `dir` as if the exponent range
            /// were unbounded, that exceeds [`MAX`](Self::MAX) in magnitude
            /// gives the IEEE 754-2019 overflow result: +∞, or
            /// [`MIN`](Self::MIN) when negative, for
            /// [`Rounding::TowardPositive`], and [`MAX`](Self::MAX), or −∞
            /// when negative, for [`Rounding::TowardNegative`].
            pub fn $rounded(value: $large, dir: Rounding) -> Result<Self, Error> {
                if value.is_special() {
                    return Ok(Self::special_of(
                        value.is_nan(),
                        value.is_sign_negative(),
                        value.payload(),
                    ));
                }
                let v = value.unpack();
                Self::finish(narrow::<$small_width>(v, Some(dir)), v.neg, Some(dir))
            }
        }

        impl PartialEq<$large> for $small {
            #[inline]
            fn eq(&self, other: &$large) -> bool {
                if self.is_special() | other.is_special() {
                    return eq_by_class::<$large_width>(*self, *other);
                }
                kernel::eq::<$large_width>(self.comparison_parts(), other.comparison_parts())
            }
        }

        impl PartialEq<$small> for $large {
            fn eq(&self, other: &$small) -> bool {
                other == self
            }
        }

        impl PartialOrd<$large> for $small {
            fn partial_cmp(&self, other: &$large) -> Option<Ordering> {
                if self.is_special() | other.is_special() {
                    return cmp_by_class::<$large_width>(*self, *other);
                }
                Some(kernel::cmp::<$large_width>(
                    self.comparison_parts(),
                    other.comparison_parts(),
                ))
            }
        }

        impl PartialOrd<$small> for $large {
            fn partial_cmp(&self, other: &$small) -> Option<Ordering> {
                if self.is_special() | other.is_special() {
                    return cmp_by_class::<$large_width>(*self, *other);
                }
                Some(kernel::cmp::<$large_width>(
                    self.comparison_parts(),
                    other.comparison_parts(),
                ))
            }
        }
    };
}

width_pair!(
    Decimal32,
    Decimal64,
    W32,
    W64,
    encode_32,
    encode_64,
    from_d64_rounded
);
width_pair!(
    Decimal32,
    Decimal128,
    W32,
    W128,
    encode_32,
    encode_128,
    from_d128_rounded
);
width_pair!(
    Decimal64,
    Decimal128,
    W64,
    W128,
    encode_64,
    encode_128,
    from_d128_rounded
);
