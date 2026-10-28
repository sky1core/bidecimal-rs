#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use core::cmp::Ordering;
use core::fmt;
use core::hash::{Hash, Hasher};
use core::ops::Deref;

use crate::kernel;
use crate::layout::Width;
use crate::operand::sealed::{Class, Sealed};

pub(crate) mod sealed {
    use core::cmp::Ordering;
    use core::hash::Hasher;

    pub trait Keyed: Copy {
        fn key_eq(self, other: Self) -> bool;
        fn key_cmp(self, other: Self) -> Ordering;
        fn key_hash<H: Hasher>(self, state: &mut H);
    }
}

use sealed::Keyed;

/// A decimal value as a collection key: [`Eq`], [`Ord`], and [`Hash`] over the
/// order `-∞ < finite values < +∞ < NaN`, where every NaN is one key and
/// numerically equal values, `-0` and `+0` included, are one key.
///
/// This is the role `OrderedFloat` from the `ordered-float` crate plays for
/// `f64`: the decimal types keep the IEEE 754-2019 comparison, under which a
/// NaN is unordered, and the wrapper fixes one equality, order, and hash for
/// keys. Equal keys can still give different results: `1 / -0` is `-∞` and
/// `1 / +0` is `+∞`, and `1.0` and `1.00` keep their exponents. Where equal
/// inputs must give equal results, as in memoization or interning, key on
/// `to_bits` instead.
///
/// The wrapped type is [`Decimal32`](crate::Decimal32),
/// [`Decimal64`](crate::Decimal64), or [`Decimal128`](crate::Decimal128).
/// The wrapper has the size and alignment of the wrapped value.
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct OrderedDecimal<T>(pub T);

impl<T> OrderedDecimal<T> {
    /// Returns the wrapped value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> From<T> for OrderedDecimal<T> {
    fn from(value: T) -> Self {
        Self(value)
    }
}

impl<T> Deref for OrderedDecimal<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T: Keyed> PartialEq for OrderedDecimal<T> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.0.key_eq(other.0)
    }
}

impl<T: Keyed> Eq for OrderedDecimal<T> {}

impl<T: Keyed> PartialOrd for OrderedDecimal<T> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T: Keyed> Ord for OrderedDecimal<T> {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.key_cmp(other.0)
    }
}

impl<T: Keyed> Hash for OrderedDecimal<T> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.key_hash(state);
    }
}

impl<T: fmt::Display> fmt::Display for OrderedDecimal<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl<T: fmt::Debug> fmt::Debug for OrderedDecimal<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

fn rank(class: &Class) -> u8 {
    match class {
        Class::Infinite(true) => 0,
        Class::Finite(_) => 1,
        Class::Infinite(false) => 2,
        Class::Nan { .. } => 3,
    }
}

/// Key equality, with finite values compared at width `W`. The path for an
/// infinity or NaN operand.
#[cold]
#[inline(never)]
pub(crate) fn eq_keys<W: Width>(a: impl Sealed, b: impl Sealed) -> bool {
    match (a.class(), b.class()) {
        (Class::Finite(x), Class::Finite(y)) => kernel::eq::<W>(x, y),
        (a, b) => rank(&a) == rank(&b),
    }
}

/// Key order, with finite values compared at width `W`. The path for an
/// infinity or NaN operand.
#[cold]
#[inline(never)]
pub(crate) fn cmp_keys<W: Width>(a: impl Sealed, b: impl Sealed) -> Ordering {
    match (a.class(), b.class()) {
        (Class::Finite(x), Class::Finite(y)) => kernel::cmp::<W>(x, y),
        (a, b) => rank(&a).cmp(&rank(&b)),
    }
}

/// The offset that maps the exponent of a canonical finite key, in
/// `-6176..=6144`, into `0..=12320`.
const HASH_EXP_OFFSET: i32 = 6176;
/// The packed exponent field of the infinities.
const HASH_INFINITE: u128 = 0x3ffe;
/// The packed exponent field of NaN.
const HASH_NAN: u128 = 0x3fff;

/// The hashed form of a canonical finite key: the sign in bit 127, the offset
/// exponent in bits 113 to 126, and the coefficient, below `10^34 < 2^113`, in
/// bits 0 to 112.
#[inline]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "canonical exponents are in -6176..=6144, so the offset exponent is in 0..=12320"
)]
pub(crate) fn finite_hash_key(neg: bool, coeff: u128, exp: i32) -> u128 {
    (u128::from(neg) << 127) | (((exp + HASH_EXP_OFFSET) as u128) << 113) | coeff
}

/// The hashed form of an infinity or NaN key, with an exponent field that no
/// finite key has.
#[inline]
pub(crate) fn special_hash_key(nan: bool, neg: bool) -> u128 {
    if nan {
        HASH_NAN << 113
    } else {
        (u128::from(neg) << 127) | (HASH_INFINITE << 113)
    }
}
