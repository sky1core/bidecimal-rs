use core::cmp::Ordering;
use core::hash::{Hash, Hasher};
use core::num::FpCategory;
use std::collections::{BTreeSet, HashMap};
use std::format;
use std::string::String;
use std::vec::Vec;

use crate::refbid::{self, Classified, Finite, Format};
use crate::test_support::{Rng, bounds, classes, hash};
use crate::{Decimal32, Decimal64, Decimal128, Error, Operand, OrderedDecimal, Rounding};

/// The observable class of a result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Out {
    Nan {
        neg: bool,
        signaling: bool,
        payload: u128,
    },
    Inf(bool),
    Fin(Finite),
}

trait S:
    Copy
    + PartialEq
    + PartialOrd
    + core::fmt::Debug
    + core::fmt::Display
    + core::ops::Neg<Output = Self>
    + core::str::FromStr<Err = Error>
    + crate::ordered::sealed::Keyed
    + Operand
{
    fn stripped_texts(self) -> (String, String);
    const FORMAT: Format;
    const TRAIL: u32;
    fn bits(self) -> u128;
    fn of(bits: u128) -> Self;
    fn from_bits_checked(bits: u128) -> Result<Self, Error>;
    fn add(a: impl Operand, b: impl Operand) -> Result<Self, Error>;
    fn sub(a: impl Operand, b: impl Operand) -> Result<Self, Error>;
    fn mul(a: impl Operand, b: impl Operand) -> Result<Self, Error>;
    fn div(a: impl Operand, b: impl Operand) -> Result<Self, Error>;
    fn add_rounded(a: impl Operand, b: impl Operand, dir: Rounding) -> Result<Self, Error>;
    fn sub_rounded(a: impl Operand, b: impl Operand, dir: Rounding) -> Result<Self, Error>;
    fn mul_rounded(a: impl Operand, b: impl Operand, dir: Rounding) -> Result<Self, Error>;
    fn div_rounded(a: impl Operand, b: impl Operand, dir: Rounding) -> Result<Self, Error>;
    fn parse(s: &str) -> Result<Self, Error>;
    fn parse_rounded(s: &str, dir: Rounding) -> Result<Self, Error>;
    fn quantize(self, exp: i32, dir: Rounding) -> Result<Self, Error>;
    fn round_to_multiple(self, step: Self, dir: Rounding) -> Result<Self, Error>;
    fn total(&self, other: &Self) -> Ordering;
    fn category(self) -> FpCategory;
    fn nan(self) -> bool;
    fn infinite(self) -> bool;
    fn finite(self) -> bool;
    fn zero(self) -> bool;
    fn positive(self) -> bool;
    fn negative(self) -> bool;
    fn sign_negative(self) -> bool;
    fn max() -> Self;
    fn min() -> Self;
    fn pos_nan() -> Self;
    fn pos_inf() -> Self;
    fn neg_inf() -> Self;
}

macro_rules! impl_s {
    ($t:ty, $bits:ty, $format:expr, $trail:expr) => {
        impl S for $t {
            const FORMAT: Format = $format;
            const TRAIL: u32 = $trail;
            fn stripped_texts(self) -> (String, String) {
                let stripped = self.stripped();
                (format!("{stripped}"), format!("{stripped:>8}"))
            }
            fn bits(self) -> u128 {
                u128::from(self.to_bits())
            }
            fn of(bits: u128) -> Self {
                <$t>::from_bits(bits as $bits).unwrap()
            }
            fn from_bits_checked(bits: u128) -> Result<Self, Error> {
                <$t>::from_bits(bits as $bits)
            }
            fn add(a: impl Operand, b: impl Operand) -> Result<Self, Error> {
                <$t>::add(a, b)
            }
            fn sub(a: impl Operand, b: impl Operand) -> Result<Self, Error> {
                <$t>::sub(a, b)
            }
            fn mul(a: impl Operand, b: impl Operand) -> Result<Self, Error> {
                <$t>::mul(a, b)
            }
            fn div(a: impl Operand, b: impl Operand) -> Result<Self, Error> {
                <$t>::div(a, b)
            }
            fn add_rounded(a: impl Operand, b: impl Operand, dir: Rounding) -> Result<Self, Error> {
                <$t>::add_rounded(a, b, dir)
            }
            fn sub_rounded(a: impl Operand, b: impl Operand, dir: Rounding) -> Result<Self, Error> {
                <$t>::sub_rounded(a, b, dir)
            }
            fn mul_rounded(a: impl Operand, b: impl Operand, dir: Rounding) -> Result<Self, Error> {
                <$t>::mul_rounded(a, b, dir)
            }
            fn div_rounded(a: impl Operand, b: impl Operand, dir: Rounding) -> Result<Self, Error> {
                <$t>::div_rounded(a, b, dir)
            }
            fn parse(s: &str) -> Result<Self, Error> {
                <$t>::parse(s)
            }
            fn parse_rounded(s: &str, dir: Rounding) -> Result<Self, Error> {
                <$t>::parse_rounded(s, dir)
            }
            fn quantize(self, exp: i32, dir: Rounding) -> Result<Self, Error> {
                <$t>::quantize(self, exp, dir)
            }
            fn round_to_multiple(self, step: Self, dir: Rounding) -> Result<Self, Error> {
                <$t>::round_to_multiple(self, step, dir)
            }
            fn total(&self, other: &Self) -> Ordering {
                self.total_cmp(other)
            }
            fn category(self) -> FpCategory {
                self.classify()
            }
            fn nan(self) -> bool {
                self.is_nan()
            }
            fn infinite(self) -> bool {
                self.is_infinite()
            }
            fn finite(self) -> bool {
                self.is_finite()
            }
            fn zero(self) -> bool {
                self.is_zero()
            }
            fn positive(self) -> bool {
                self.is_positive()
            }
            fn negative(self) -> bool {
                self.is_negative()
            }
            fn sign_negative(self) -> bool {
                self.is_sign_negative()
            }
            fn max() -> Self {
                <$t>::MAX
            }
            fn min() -> Self {
                <$t>::MIN
            }
            fn pos_nan() -> Self {
                <$t>::NAN
            }
            fn pos_inf() -> Self {
                <$t>::INFINITY
            }
            fn neg_inf() -> Self {
                <$t>::NEG_INFINITY
            }
        }
    };
}

impl_s!(Decimal32, u32, Format::D32, 20);
impl_s!(Decimal64, u64, Format::D64, 50);
impl_s!(Decimal128, u128, Format::D128, 110);

fn width_bits(format: Format) -> u32 {
    format.parameters().0
}

fn nan_bits<T: S>(neg: bool, signaling: bool, payload: u128) -> u128 {
    let k = width_bits(T::FORMAT);
    (u128::from(neg) << (k - 1))
        | (0b11111 << (k - 6))
        | (u128::from(signaling) << (k - 7))
        | payload
}

fn inf_bits<T: S>(neg: bool) -> u128 {
    let k = width_bits(T::FORMAT);
    (u128::from(neg) << (k - 1)) | (0b11110 << (k - 6))
}

fn out<T: S>(x: T) -> Out {
    let bits = x.bits();
    let k = width_bits(T::FORMAT);
    match refbid::classify(bits, T::FORMAT) {
        Classified::Finite(v) => Out::Fin(v),
        Classified::Infinity => {
            assert_eq!(
                bits & !(1 << (k - 1)),
                inf_bits::<T>(false),
                "non-canonical infinity"
            );
            Out::Inf(bits >> (k - 1) != 0)
        }
        Classified::Nan => {
            let payload = bits & ((1 << T::TRAIL) - 1);
            assert!(
                payload < 10_u128.pow(bounds(T::FORMAT).0 - 1),
                "payload out of range"
            );
            assert_eq!(
                bits & !(nan_bits::<T>(true, true, (1 << T::TRAIL) - 1)),
                0,
                "non-canonical NaN"
            );
            Out::Nan {
                neg: bits >> (k - 1) != 0,
                signaling: bits & (1 << (k - 7)) != 0,
                payload,
            }
        }
        Classified::NonCanonical => panic!("non-canonical result {bits:#x}"),
    }
}

fn fin<T: S>(neg: bool, coeff: u128, exp: i32) -> T {
    T::of(refbid::encode(Finite { neg, coeff, exp }, T::FORMAT))
}

fn qnan<T: S>(neg: bool, payload: u128) -> T {
    T::of(nan_bits::<T>(neg, false, payload))
}

fn snan<T: S>(neg: bool, payload: u128) -> T {
    T::of(nan_bits::<T>(neg, true, payload))
}

/// The f64 counterpart of a special or small finite value.
fn as_f64<T: S>(x: T) -> f64 {
    match out(x) {
        Out::Nan { neg, .. } => {
            if neg {
                -f64::NAN
            } else {
                f64::NAN
            }
        }
        Out::Inf(neg) => {
            if neg {
                f64::NEG_INFINITY
            } else {
                f64::INFINITY
            }
        }
        Out::Fin(v) => format!("{}{}e{}", if v.neg { "-" } else { "" }, v.coeff, v.exp)
            .parse()
            .unwrap(),
    }
}

/// Whether the f64 image of `x` keeps its class: a tiny or huge finite value
/// becomes an f64 zero or infinity and cannot serve as an oracle.
fn f64_faithful<T: S>(x: T) -> bool {
    match out(x) {
        Out::Fin(v) => v.coeff == 0 || (as_f64(x) != 0.0 && as_f64(x).is_finite()),
        _ => true,
    }
}

fn same_class_and_sign<T: S>(x: T, y: f64) -> bool {
    match out(x) {
        Out::Nan { .. } => y.is_nan(),
        Out::Inf(neg) => y.is_infinite() && y.is_sign_negative() == neg,
        Out::Fin(v) => {
            y.is_finite()
                && (v.coeff == 0) == (y == 0.0)
                && (v.coeff == 0 || v.neg == y.is_sign_negative())
                && (v.coeff != 0 || v.neg == y.is_sign_negative())
        }
    }
}

fn samples<T: S>() -> Vec<T> {
    let (p, lo, hi) = bounds(T::FORMAT);
    let full = 10_u128.pow(p) - 1;
    let mut v: Vec<T> = Vec::new();
    for neg in [false, true] {
        v.push(T::of(nan_bits::<T>(neg, false, 0)));
        v.push(T::of(nan_bits::<T>(neg, true, 7)));
        v.push(T::of(inf_bits::<T>(neg)));
        v.push(fin(neg, 0, 0));
        v.push(fin(neg, 0, lo));
        v.push(fin(neg, 1, 0));
        v.push(fin(neg, 25, -1));
        v.push(fin(neg, 1, lo));
        v.push(fin(neg, full, hi));
        v.push(fin(neg, 3, 2));
    }
    v
}

fn special_arithmetic<T: S>() {
    let values = samples::<T>();
    for &a in &values {
        for &b in &values {
            if !f64_faithful(a) || !f64_faithful(b) {
                continue;
            }
            let (x, y) = (as_f64(a), as_f64(b));
            let finite_pair = a.finite() && b.finite();
            for (name, ours, theirs) in [
                ("add", T::add(a, b), x + y),
                ("sub", T::sub(a, b), x - y),
                ("mul", T::mul(a, b), x * y),
                ("div", T::div(a, b), x / y),
            ] {
                if finite_pair
                    && !(name == "div"
                        && out(b)
                            == Out::Fin(Finite {
                                neg: b.sign_negative(),
                                coeff: 0,
                                exp: refbid_exp(b),
                            }))
                {
                    continue;
                }
                let got = ours.unwrap_or_else(|e| panic!("{name} {a:?} {b:?}: {e:?}"));
                assert!(
                    same_class_and_sign(got, theirs),
                    "{name} {a:?} {b:?}: got {got:?}, f64 {theirs}"
                );
                if let (Out::Fin(v), true) = (out(got), name == "div") {
                    assert_eq!(v.exp, bounds(T::FORMAT).1, "finite / infinity exponent");
                }
            }
            for dir in [Rounding::TowardNegative, Rounding::TowardPositive] {
                if finite_pair {
                    continue;
                }
                assert_eq!(
                    out(T::add_rounded(a, b, dir).unwrap()),
                    out(T::add(a, b).unwrap())
                );
                assert_eq!(
                    out(T::mul_rounded(a, b, dir).unwrap()),
                    out(T::mul(a, b).unwrap())
                );
                assert_eq!(
                    out(T::div_rounded(a, b, dir).unwrap()),
                    out(T::div(a, b).unwrap())
                );
            }
        }
    }
}

fn refbid_exp<T: S>(x: T) -> i32 {
    match refbid::classify(x.bits(), T::FORMAT) {
        Classified::Finite(v) => v.exp,
        _ => i32::MIN,
    }
}

#[test]
fn special_value_arithmetic_matches_ieee_and_f64() {
    special_arithmetic::<Decimal32>();
    special_arithmetic::<Decimal64>();
    special_arithmetic::<Decimal128>();
}

#[derive(Clone, Copy, Debug)]
enum Op {
    Add,
    Sub,
    Mul,
    Div,
}

/// The IEEE 754-2019 result in `R` of `a op b` when an operand is an infinity
/// or NaN, or of a division by zero; `None` for the other finite operands.
fn special_result<R: S>(op: Op, a: Out, b: Out) -> Option<Out> {
    let nan = Out::Nan {
        neg: false,
        signaling: false,
        payload: 0,
    };
    let sign = |x: Out| match x {
        Out::Nan { neg, .. } | Out::Inf(neg) => neg,
        Out::Fin(v) => v.neg,
    };
    let zero = |x: Out| matches!(x, Out::Fin(v) if v.coeff == 0);
    let quotient_neg = sign(a) != sign(b);
    Some(match (a, b) {
        (Out::Nan { neg, payload, .. }, _) | (_, Out::Nan { neg, payload, .. }) => Out::Nan {
            neg,
            signaling: false,
            payload: if payload < 10_u128.pow(bounds(R::FORMAT).0 - 1) {
                payload
            } else {
                0
            },
        },
        (Out::Fin(_), Out::Fin(_)) => match op {
            Op::Div if zero(b) && zero(a) => nan,
            Op::Div if zero(b) => Out::Inf(quotient_neg),
            _ => return None,
        },
        _ => match op {
            Op::Add | Op::Sub => {
                let b_neg = sign(b) != matches!(op, Op::Sub);
                match (a, b) {
                    (Out::Inf(a_neg), Out::Inf(_)) if a_neg != b_neg => nan,
                    (Out::Inf(a_neg), _) => Out::Inf(a_neg),
                    _ => Out::Inf(b_neg),
                }
            }
            Op::Mul if zero(a) || zero(b) => nan,
            Op::Mul => Out::Inf(quotient_neg),
            Op::Div => match (a, b) {
                (Out::Inf(_), Out::Inf(_)) => nan,
                (Out::Inf(_), _) => Out::Inf(quotient_neg),
                _ => Out::Fin(Finite {
                    neg: quotient_neg,
                    coeff: 0,
                    exp: bounds(R::FORMAT).1,
                }),
            },
        },
    })
}

/// [`samples`] plus NaNs whose payloads are at and beyond the largest
/// Decimal32 payload and at the largest payload of the width.
fn mixed_samples<T: S>() -> Vec<T> {
    let largest = 10_u128.pow(bounds(T::FORMAT).0 - 1) - 1;
    let mut v = samples::<T>();
    for payload in [999_999, 1_000_000, largest] {
        if payload <= largest {
            v.push(qnan(true, payload));
            v.push(snan(false, payload));
        }
    }
    v
}

fn mixed_special_arithmetic<A: S, B: S, R: S>() {
    for &a in &mixed_samples::<A>() {
        for &b in &mixed_samples::<B>() {
            for (op, exact, rounded) in [
                (
                    Op::Add,
                    R::add(a, b),
                    [Rounding::TowardNegative, Rounding::TowardPositive]
                        .map(|dir| R::add_rounded(a, b, dir)),
                ),
                (
                    Op::Sub,
                    R::sub(a, b),
                    [Rounding::TowardNegative, Rounding::TowardPositive]
                        .map(|dir| R::sub_rounded(a, b, dir)),
                ),
                (
                    Op::Mul,
                    R::mul(a, b),
                    [Rounding::TowardNegative, Rounding::TowardPositive]
                        .map(|dir| R::mul_rounded(a, b, dir)),
                ),
                (
                    Op::Div,
                    R::div(a, b),
                    [Rounding::TowardNegative, Rounding::TowardPositive]
                        .map(|dir| R::div_rounded(a, b, dir)),
                ),
            ] {
                let Some(expected) = special_result::<R>(op, out(a), out(b)) else {
                    continue;
                };
                for got in [exact].into_iter().chain(rounded) {
                    assert_eq!(got.map(out), Ok(expected), "{op:?} {a:?} {b:?}");
                }
            }
        }
    }
}

#[test]
fn special_arithmetic_every_width_combination() {
    macro_rules! with_r {
        ($a:ty, $b:ty) => {
            mixed_special_arithmetic::<$a, $b, Decimal32>();
            mixed_special_arithmetic::<$a, $b, Decimal64>();
            mixed_special_arithmetic::<$a, $b, Decimal128>();
        };
    }
    macro_rules! with_b {
        ($a:ty) => {
            with_r!($a, Decimal32);
            with_r!($a, Decimal64);
            with_r!($a, Decimal128);
        };
    }
    with_b!(Decimal32);
    with_b!(Decimal64);
    with_b!(Decimal128);
}

fn nan_payload_propagation<T: S>() {
    let (p, ..) = bounds(T::FORMAT);
    let payload = 10_u128.pow(p - 1) - 1;
    let q: T = qnan(true, payload);
    let s: T = snan(false, 5);
    let one: T = fin(false, 1, 0);
    let quiet = |neg, payload| Out::Nan {
        neg,
        signaling: false,
        payload,
    };
    assert_eq!(out(T::add(q, one).unwrap()), quiet(true, payload));
    assert_eq!(out(T::add(one, q).unwrap()), quiet(true, payload));
    assert_eq!(out(T::mul(s, one).unwrap()), quiet(false, 5));
    assert_eq!(out(T::div(one, s).unwrap()), quiet(false, 5));
    assert_eq!(
        out(T::sub(q, s).unwrap()),
        quiet(true, payload),
        "first NaN operand wins"
    );
    assert_eq!(out(T::sub(s, q).unwrap()), quiet(false, 5));
    assert_eq!(
        out(q.quantize(0, Rounding::TowardNegative).unwrap()),
        quiet(true, payload)
    );
    assert_eq!(
        out(one.round_to_multiple(s, Rounding::TowardPositive).unwrap()),
        quiet(false, 5)
    );
    assert_eq!(
        out(T::pos_inf().quantize(0, Rounding::TowardNegative).unwrap()),
        quiet(false, 0)
    );
    assert_eq!(
        out(T::neg_inf()
            .round_to_multiple(one, Rounding::TowardPositive)
            .unwrap()),
        Out::Inf(true)
    );
    assert_eq!(
        out(one
            .round_to_multiple(T::pos_inf(), Rounding::TowardPositive)
            .unwrap()),
        quiet(false, 0)
    );
    assert_eq!(
        one.round_to_multiple(T::neg_inf(), Rounding::TowardPositive),
        Err(Error::InvalidInput)
    );
    assert_eq!(
        one.round_to_multiple(fin(false, 0, 0), Rounding::TowardPositive),
        Err(Error::DivisionByZero)
    );
    assert_eq!(
        q.round_to_multiple(fin(false, 0, 0), Rounding::TowardPositive)
            .map(out),
        Ok(quiet(true, payload))
    );
}

#[test]
fn nan_payloads_propagate_and_signaling_nans_are_quieted() {
    nan_payload_propagation::<Decimal32>();
    nan_payload_propagation::<Decimal64>();
    nan_payload_propagation::<Decimal128>();
}

fn directed_overflow<T: S>() {
    let (p, _, hi) = bounds(T::FORMAT);
    let max = T::max();
    let one_ulp: T = fin(false, 1, hi);
    let big: T = fin(false, 6 * 10_u128.pow(p - 1), hi);
    for (neg, a, b) in [(false, max, one_ulp), (true, T::min(), one_ulp)] {
        let add = |dir| {
            if neg {
                T::add_rounded(a, -b, dir)
            } else {
                T::add_rounded(a, b, dir)
            }
        };
        assert_eq!(T::add(a, if neg { -b } else { b }), Err(Error::Overflow));
        assert_eq!(
            out(add(Rounding::TowardPositive).unwrap()),
            if neg { out(T::min()) } else { Out::Inf(false) }
        );
        assert_eq!(
            out(add(Rounding::TowardNegative).unwrap()),
            if neg { Out::Inf(true) } else { out(T::max()) }
        );
        let two: T = fin(neg, 2, 0);
        assert_eq!(T::mul(big, two), Err(Error::Overflow));
        assert_eq!(
            out(T::mul_rounded(big, two, Rounding::TowardPositive).unwrap()),
            if neg { out(T::min()) } else { Out::Inf(false) }
        );
        assert_eq!(
            out(T::mul_rounded(big, two, Rounding::TowardNegative).unwrap()),
            if neg { Out::Inf(true) } else { out(T::max()) }
        );
        let half: T = fin(neg, 5, -1);
        assert_eq!(T::div(big, half), Err(Error::Overflow));
        assert_eq!(
            out(T::div_rounded(big, half, Rounding::TowardPositive).unwrap()),
            if neg { out(T::min()) } else { Out::Inf(false) }
        );
        assert_eq!(
            out(T::div_rounded(big, half, Rounding::TowardNegative).unwrap()),
            if neg { Out::Inf(true) } else { out(T::max()) }
        );
        let text = format!("{}1e{}", if neg { "-" } else { "" }, hi + p as i32);
        assert_eq!(T::parse(&text), Err(Error::Overflow));
        assert_eq!(
            out(T::parse_rounded(&text, Rounding::TowardPositive).unwrap()),
            if neg { out(T::min()) } else { Out::Inf(false) }
        );
        assert_eq!(
            out(T::parse_rounded(&text, Rounding::TowardNegative).unwrap()),
            if neg { Out::Inf(true) } else { out(T::max()) }
        );
    }
}

#[test]
fn directed_overflow_gives_the_ieee_infinity_or_largest_finite_value() {
    directed_overflow::<Decimal32>();
    directed_overflow::<Decimal64>();
    directed_overflow::<Decimal128>();
}

#[test]
fn narrowing_overflow_and_specials() {
    let wide = Decimal128::parse("1e400").unwrap();
    assert_eq!(Decimal64::try_from(wide), Err(Error::Overflow));
    assert_eq!(
        Decimal64::from_d128_rounded(wide, Rounding::TowardPositive),
        Ok(Decimal64::INFINITY)
    );
    assert_eq!(
        Decimal64::from_d128_rounded(wide, Rounding::TowardNegative),
        Ok(Decimal64::MAX)
    );
    assert_eq!(
        Decimal64::from_d128_rounded(-wide, Rounding::TowardPositive),
        Ok(Decimal64::MIN)
    );
    assert_eq!(
        Decimal64::from_d128_rounded(-wide, Rounding::TowardNegative),
        Ok(Decimal64::NEG_INFINITY)
    );
    assert_eq!(
        Decimal32::try_from(Decimal128::NEG_INFINITY).map(out),
        Ok(Out::Inf(true))
    );
    let payload = 123_456_789_u128;
    let wide_nan: Decimal128 = snan(true, payload);
    assert_eq!(
        out(Decimal64::try_from(wide_nan).unwrap()),
        Out::Nan {
            neg: true,
            signaling: false,
            payload
        }
    );
    assert_eq!(
        out(Decimal32::try_from(wide_nan).unwrap()),
        Out::Nan {
            neg: true,
            signaling: false,
            payload: 0
        }
    );
    assert_eq!(
        out(Decimal32::from_d128_rounded(wide_nan, Rounding::TowardNegative).unwrap()),
        Out::Nan {
            neg: true,
            signaling: false,
            payload: 0
        }
    );
    let narrow_nan: Decimal32 = snan(false, 999_999);
    assert_eq!(
        out(Decimal64::from(narrow_nan)),
        Out::Nan {
            neg: false,
            signaling: false,
            payload: 999_999
        }
    );
    assert_eq!(
        out(Decimal128::from(narrow_nan)),
        Out::Nan {
            neg: false,
            signaling: false,
            payload: 999_999
        }
    );
    assert_eq!(
        out(Decimal128::from(Decimal32::NEG_INFINITY)),
        Out::Inf(true)
    );
    assert_eq!(out(Decimal64::from(Decimal32::INFINITY)), Out::Inf(false));
    assert_eq!(
        Decimal128::add(Decimal32::INFINITY, Decimal64::NEG_INFINITY).map(out),
        Ok(Out::Nan {
            neg: false,
            signaling: false,
            payload: 0
        })
    );
    assert_eq!(
        Decimal32::div(Decimal64::parse("1").unwrap(), Decimal128::NEG_INFINITY).map(out),
        Ok(Out::Fin(Finite {
            neg: true,
            coeff: 0,
            exp: -101
        }))
    );
}

fn canonical_bits<T: S>() {
    let (p, ..) = bounds(T::FORMAT);
    let k = width_bits(T::FORMAT);
    let max_payload = 10_u128.pow(p - 1) - 1;
    for neg in [false, true] {
        assert_eq!(
            T::from_bits_checked(inf_bits::<T>(neg)).map(out),
            Ok(Out::Inf(neg))
        );
        assert_eq!(
            T::from_bits_checked(inf_bits::<T>(neg) | 1),
            Err(Error::InvalidInput)
        );
        assert_eq!(
            T::from_bits_checked(inf_bits::<T>(neg) | (1 << (k - 7))),
            Err(Error::InvalidInput)
        );
        assert_eq!(
            T::from_bits_checked(inf_bits::<T>(neg) | (1 << (k - 8))),
            Err(Error::InvalidInput)
        );
        for signaling in [false, true] {
            for payload in [0, 1, max_payload] {
                let bits = nan_bits::<T>(neg, signaling, payload);
                assert_eq!(
                    T::from_bits_checked(bits).map(out),
                    Ok(Out::Nan {
                        neg,
                        signaling,
                        payload
                    })
                );
                assert_eq!(T::from_bits_checked(bits).unwrap().bits(), bits);
            }
            assert_eq!(
                T::from_bits_checked(nan_bits::<T>(neg, signaling, max_payload + 1)),
                Err(Error::InvalidInput)
            );
            assert_eq!(
                T::from_bits_checked(nan_bits::<T>(neg, signaling, 0) | (1 << (k - 8))),
                Err(Error::InvalidInput)
            );
            assert_eq!(
                T::from_bits_checked(nan_bits::<T>(neg, signaling, 0) | (1 << T::TRAIL)),
                Err(Error::InvalidInput)
            );
        }
    }
}

#[test]
fn from_bits_accepts_exactly_canonical_specials() {
    canonical_bits::<Decimal32>();
    canonical_bits::<Decimal64>();
    canonical_bits::<Decimal128>();
}

fn comparisons<T: S>() {
    let values = samples::<T>();
    for &a in &values {
        if !f64_faithful(a) {
            continue;
        }
        for &b in &values {
            if !f64_faithful(b) {
                continue;
            }
            let (x, y) = (as_f64(a), as_f64(b));
            assert_eq!(a == b, x == y, "{a:?} == {b:?}");
            assert_eq!(a != b, x != y, "{a:?} != {b:?}");
            assert_eq!(a.partial_cmp(&b), x.partial_cmp(&y), "{a:?} cmp {b:?}");
            assert_eq!(a < b, x < y);
            assert_eq!(a <= b, x <= y);
            assert_eq!(a > b, x > y);
            assert_eq!(a >= b, x >= y);
            let cohort = x == y && refbid_exp(a) != refbid_exp(b);
            if !x.is_nan() && !y.is_nan() && !cohort {
                assert_eq!(a.total(&b), x.total_cmp(&y), "total {a:?} {b:?}");
            }
        }
        let x = as_f64(a);
        assert_eq!(a.nan(), x.is_nan());
        assert_eq!(a.infinite(), x.is_infinite());
        assert_eq!(a.finite(), x.is_finite());
        assert_eq!(a.zero(), x == 0.0);
        assert_eq!(a.positive(), x > 0.0, "{a:?}");
        assert_eq!(a.negative(), x < 0.0, "{a:?}");
        assert_eq!(a.sign_negative(), x.is_sign_negative());
        let category = a.category();
        match x.classify() {
            FpCategory::Subnormal | FpCategory::Normal => assert!(matches!(
                category,
                FpCategory::Subnormal | FpCategory::Normal
            )),
            other => assert_eq!(category, other, "{a:?}"),
        }
    }
    let (p, lo, _) = bounds(T::FORMAT);
    let emin = lo + p as i32 - 1;
    assert_eq!(fin::<T>(false, 1, lo).category(), FpCategory::Subnormal);
    assert_eq!(
        fin::<T>(true, 10_u128.pow(p - 1) - 1, lo).category(),
        FpCategory::Subnormal
    );
    assert_eq!(
        fin::<T>(false, 10_u128.pow(p - 1), lo).category(),
        FpCategory::Normal
    );
    assert_eq!(fin::<T>(false, 1, emin).category(), FpCategory::Normal);
    assert_eq!(
        fin::<T>(false, 1, emin - 1).category(),
        FpCategory::Subnormal
    );
}

#[test]
fn special_value_comparisons_and_classification_match_f64() {
    comparisons::<Decimal32>();
    comparisons::<Decimal64>();
    comparisons::<Decimal128>();
}

#[test]
fn cross_width_comparisons_with_specials() {
    let nan32 = Decimal32::NAN;
    let one64 = Decimal64::parse("1").unwrap();
    assert!(nan32 != one64 && one64 != nan32);
    assert_eq!(nan32.partial_cmp(&one64), None);
    assert_eq!(one64.partial_cmp(&nan32), None);
    assert!(Decimal32::NEG_INFINITY < Decimal128::MIN);
    assert!(Decimal128::MAX < Decimal32::INFINITY);
    assert!(Decimal32::INFINITY == Decimal128::INFINITY);
    assert!(Decimal64::NEG_INFINITY != Decimal128::INFINITY);
    assert_eq!(
        Decimal64::INFINITY.partial_cmp(&Decimal32::INFINITY),
        Some(Ordering::Equal)
    );
    assert_eq!(
        Decimal128::NEG_INFINITY.partial_cmp(&Decimal32::INFINITY),
        Some(Ordering::Less)
    );
    assert!(Decimal128::NAN != Decimal128::NAN);
}

fn total_order<T: S>() {
    let (p, lo, hi) = bounds(T::FORMAT);
    let full = 10_u128.pow(p) - 1;
    let mut rng = Rng(0x7a7a);
    let mut values: Vec<T> = samples::<T>();
    for _ in 0..200 {
        let v = rng.value(T::FORMAT);
        values.push(fin(v.neg, v.coeff, v.exp));
    }
    for neg in [false, true] {
        values.push(fin(neg, 100, -2));
        values.push(fin(neg, 10, -1));
        values.push(fin(neg, 1, 0));
        values.push(fin(neg, full, hi));
        values.push(fin(neg, 0, hi));
        values.push(fin(neg, 0, lo));
        values.push(T::of(nan_bits::<T>(neg, true, 0)));
        values.push(T::of(nan_bits::<T>(neg, true, 9)));
        values.push(T::of(nan_bits::<T>(neg, false, 9)));
    }
    let key = |x: T| -> (i8, u8, (bool, i128, i32), u128) {
        let bits = x.bits();
        let k = width_bits(T::FORMAT);
        let neg = bits >> (k - 1) != 0;
        let sign: i8 = if neg { -1 } else { 1 };
        let rank: u8 = match out(x) {
            Out::Fin(_) => 0,
            Out::Inf(_) => 1,
            Out::Nan {
                signaling: true, ..
            } => 2,
            Out::Nan {
                signaling: false, ..
            } => 3,
        };
        let magnitude = match out(x) {
            Out::Fin(v) => (true, i128::try_from(v.coeff).unwrap(), v.exp),
            Out::Nan { payload, .. } => (true, i128::try_from(payload).unwrap(), 0),
            Out::Inf(_) => (true, 0, 0),
        };
        (sign, rank, magnitude, 0)
    };
    let powers = crate::test_support::Powers::new(13_000);
    let expected = |a: T, b: T| -> Ordering {
        let (ka, kb) = (key(a), key(b));
        if ka.0 != kb.0 {
            return ka.0.cmp(&kb.0);
        }
        let magnitude = ka.1.cmp(&kb.1).then_with(|| match (out(a), out(b)) {
            (Out::Fin(x), Out::Fin(y)) => {
                let base = x.exp.min(y.exp);
                let xv = crate::test_support::integer_at(Finite { neg: false, ..x }, base, &powers);
                let yv = crate::test_support::integer_at(Finite { neg: false, ..y }, base, &powers);
                xv.cmp(&yv).then(x.exp.cmp(&y.exp))
            }
            _ => ka.2.cmp(&kb.2),
        });
        if ka.0 < 0 {
            magnitude.reverse()
        } else {
            magnitude
        }
    };
    for &a in &values {
        for &b in &values {
            assert_eq!(a.total(&b), expected(a, b), "total_cmp {a:?} {b:?}");
            assert_eq!(a.total(&b), b.total(&a).reverse());
        }
    }
    let mut sorted = values.clone();
    sorted.sort_by(|a, b| a.total(b));
    for pair in sorted.windows(2) {
        assert_ne!(pair[0].total(&pair[1]), Ordering::Greater);
    }
}

#[test]
fn total_cmp_is_the_ieee_total_order() {
    total_order::<Decimal32>();
    total_order::<Decimal64>();
    total_order::<Decimal128>();
}

fn ordered_keys<T: S>() {
    let (p, lo, hi) = bounds(T::FORMAT);
    let full = 10_u128.pow(p) - 1;
    let nans: Vec<T> = [
        (false, false, 0),
        (true, false, 0),
        (false, true, 1),
        (true, true, 5),
        (false, false, 10_u128.pow(p - 1) - 1),
    ]
    .into_iter()
    .map(|(neg, signaling, payload)| T::of(nan_bits::<T>(neg, signaling, payload)))
    .collect();
    let zeros: Vec<T> = [(false, 0), (true, 0), (false, lo), (true, hi), (false, 3)]
        .into_iter()
        .map(|(neg, exp)| fin(neg, 0, exp))
        .collect();
    let ones: Vec<T> = (0..p)
        .map(|z| fin(false, 10_u128.pow(z), -(z as i32)))
        .collect();
    for group in [&nans, &zeros, &ones] {
        for &a in group.iter() {
            for &b in group.iter() {
                let (x, y) = (OrderedDecimal(a), OrderedDecimal(b));
                assert!(x == y, "{a:?} {b:?}");
                assert_eq!(x.cmp(&y), Ordering::Equal);
                assert_eq!(x.partial_cmp(&y), Some(Ordering::Equal));
                assert_eq!(hash(x), hash(y), "{a:?} {b:?}");
            }
        }
    }
    let ladder: Vec<T> = [
        T::neg_inf(),
        T::min(),
        fin(true, 1, 0),
        fin(true, 1, lo),
        fin(false, 0, 0),
        fin(false, 1, lo),
        fin(false, 1, 0),
        fin(false, full, hi),
        T::pos_inf(),
        T::pos_nan(),
    ]
    .into_iter()
    .collect();
    for (i, &a) in ladder.iter().enumerate() {
        for (j, &b) in ladder.iter().enumerate() {
            let (x, y) = (OrderedDecimal(a), OrderedDecimal(b));
            assert_eq!(x.cmp(&y), i.cmp(&j), "{a:?} {b:?}");
            assert_eq!(x == y, i == j);
            assert_eq!(x.max(y).0.bits(), if i >= j { a.bits() } else { b.bits() });
        }
    }
    let mut set: BTreeSet<OrderedDecimal<T>> = BTreeSet::new();
    let mut map: HashMap<OrderedDecimal<T>, usize> = HashMap::new();
    let mut inserted = 0;
    for v in nans.iter().chain(&zeros).chain(&ones).chain(&ladder) {
        if set.insert(OrderedDecimal(*v)) {
            inserted += 1;
        }
        *map.entry(OrderedDecimal(*v)).or_insert(0) += 1;
    }
    assert_eq!(set.len(), inserted);
    assert_eq!(set.len(), ladder.len());
    assert_eq!(map.len(), set.len());
    assert_eq!(map[&OrderedDecimal(T::pos_nan())], nans.len() + 1);
    assert_eq!(map[&OrderedDecimal(fin(true, 0, 5))], zeros.len() + 1);
    assert_eq!(map[&OrderedDecimal(fin(false, 1, 0))], ones.len() + 1);
    assert_eq!(map.get(&OrderedDecimal(fin(false, 2, 0))), None);
    let order: Vec<u128> = set.iter().map(|v| v.0.bits()).collect();
    let expected: Vec<u128> = ladder.iter().map(|v| v.bits()).collect();
    assert!(expected.iter().all(|b| order.contains(b)));
    assert_eq!(set.iter().next().unwrap().0.bits(), T::neg_inf().bits());
    assert!(set.iter().last().unwrap().0.nan());
    let other_nan = OrderedDecimal(*nans.last().unwrap());
    assert_ne!(other_nan.0.bits(), T::pos_nan().bits());
    assert!(set.remove(&other_nan));
    assert!(!set.contains(&OrderedDecimal(T::pos_nan())));
    assert_eq!(map.remove(&other_nan), Some(nans.len() + 1));
    assert_eq!(map.get(&OrderedDecimal(T::pos_nan())), None);
    for inf in [T::neg_inf(), T::pos_inf()] {
        assert!(set.remove(&OrderedDecimal(inf)));
        assert_eq!(map.remove(&OrderedDecimal(inf)), Some(1));
        assert!(!set.contains(&OrderedDecimal(inf)));
    }
    assert_eq!(set.len(), ladder.len() - 3);
    assert_eq!(map.len(), set.len());
    assert!(set.iter().all(|k| k.0.finite()));
    assert_eq!(
        core::mem::size_of::<OrderedDecimal<T>>(),
        core::mem::size_of::<T>()
    );
    assert_eq!(
        core::mem::align_of::<OrderedDecimal<T>>(),
        core::mem::align_of::<T>()
    );
    let wrapped = OrderedDecimal(fin::<T>(false, 15, -1));
    assert_eq!(format!("{wrapped}"), "1.5");
    assert_eq!(format!("{wrapped:?}"), "1.5");
    assert_eq!(wrapped.into_inner().bits(), fin::<T>(false, 15, -1).bits());
    assert!(wrapped.finite());
}

#[test]
fn ordered_decimal_keys_merge_nans_zeros_and_cohorts() {
    ordered_keys::<Decimal32>();
    ordered_keys::<Decimal64>();
    ordered_keys::<Decimal128>();
}

/// A hasher that keeps every byte written to it, so a test can compare hash
/// inputs instead of hash values, which may collide.
struct HashInput(Vec<u8>);
impl Hasher for HashInput {
    fn finish(&self) -> u64 {
        0
    }
    fn write(&mut self, bytes: &[u8]) {
        self.0.extend_from_slice(bytes);
    }
}

/// A key of `OrderedDecimal`, computed independently of the library: a finite
/// value without trailing zeros, with `(false, 0, 0)` for every zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Key {
    Finite(bool, u128, i32),
    Infinity(bool),
    Nan,
}

fn finite_key(v: Finite) -> Key {
    if v.coeff == 0 {
        return Key::Finite(false, 0, 0);
    }
    let (mut coeff, mut exp) = (v.coeff, v.exp);
    while coeff % 10 == 0 {
        coeff /= 10;
        exp += 1;
    }
    Key::Finite(v.neg, coeff, exp)
}

fn hash_inputs<T: S>(by_key: &mut HashMap<Key, Vec<u8>>, by_input: &mut HashMap<Vec<u8>, Key>) {
    let (p, lo, hi) = bounds(T::FORMAT);
    let mut record = |key: Key, x: T| {
        let mut state = HashInput(Vec::new());
        OrderedDecimal(x).hash(&mut state);
        let input = state.0;
        assert_eq!(
            by_key.entry(key).or_insert_with(|| input.clone()),
            &input,
            "{x:?}"
        );
        assert_eq!(*by_input.entry(input).or_insert(key), key, "{x:?}");
    };
    let mut values = classes(T::FORMAT, T::FORMAT);
    let top = 1_u128 << 112;
    for coeff in [
        11,
        12,
        14,
        16,
        18,
        10_u128.pow(16) - 8,
        10_u128.pow(16) - 6,
        10_u128.pow(16) - 4,
        10_u128.pow(16) - 2,
        10_u128.pow(16) - 1,
        10_u128.pow(16),
        10_u128.pow(16) + 1,
        10_u128.pow(16) + 2,
        10_u128.pow(16) + 4,
        10_u128.pow(16) + 6,
        10_u128.pow(16) + 8,
        u128::from(u64::MAX),
        u128::from(u64::MAX) + 1,
        u128::from(u64::MAX) + 2,
        top - 1,
        top,
        top + 1,
        top | 1 << 64,
        10_u128.pow(p) - 1,
    ] {
        if coeff < 10_u128.pow(p) {
            for exp in [lo, 0, hi] {
                for neg in [false, true] {
                    values.push(Finite { neg, coeff, exp });
                }
            }
        }
    }
    for v in values {
        record(finite_key(v), fin::<T>(v.neg, v.coeff, v.exp));
    }
    for neg in [false, true] {
        record(Key::Infinity(neg), T::of(inf_bits::<T>(neg)));
        for (signaling, payload) in [
            (false, 0),
            (true, 0),
            (false, 1),
            (true, 10_u128.pow(p - 1) - 1),
        ] {
            record(Key::Nan, T::of(nan_bits::<T>(neg, signaling, payload)));
        }
    }
}

#[test]
fn ordered_decimal_hash_inputs_separate_keys_across_widths() {
    let (mut by_key, mut by_input) = (HashMap::new(), HashMap::new());
    hash_inputs::<Decimal32>(&mut by_key, &mut by_input);
    hash_inputs::<Decimal64>(&mut by_key, &mut by_input);
    hash_inputs::<Decimal128>(&mut by_key, &mut by_input);
}

const SPELLINGS: [&str; 30] = [
    "inf",
    "Inf",
    "INF",
    "+inf",
    "-inf",
    "infinity",
    "Infinity",
    "INFINITY",
    "-Infinity",
    "+INFINITY",
    "nan",
    "NaN",
    "NAN",
    "+nan",
    "-nan",
    "snan",
    "nan(1)",
    "infin",
    "in",
    "",
    "+",
    "-",
    " inf",
    "inf ",
    "infinit",
    "nans",
    "1e",
    "--inf",
    "+-nan",
    "in f",
];

fn spellings<T: S>() {
    for s in SPELLINGS {
        let (ours, theirs) = (T::parse(s), s.parse::<f64>());
        assert_eq!(ours.is_ok(), theirs.is_ok(), "{s:?}");
        if let (Ok(x), Ok(y)) = (ours, theirs) {
            assert!(same_class_and_sign(x, y), "{s:?}");
            assert_eq!(x.sign_negative(), y.is_sign_negative(), "{s:?}");
            if let Out::Nan {
                signaling, payload, ..
            } = out(x)
            {
                assert!(!signaling && payload == 0, "{s:?}");
            }
            assert_eq!(
                T::parse_rounded(s, Rounding::TowardNegative)
                    .unwrap()
                    .bits(),
                x.bits()
            );
            assert_eq!(s.parse::<T>().unwrap().bits(), x.bits());
        }
    }
}

#[test]
fn special_spellings_parse_like_f64() {
    spellings::<Decimal32>();
    spellings::<Decimal64>();
    spellings::<Decimal128>();
}

macro_rules! assert_same_format {
    ($x:expr, $y:expr, $s:expr; $($spec:literal),+ $(,)?) => {
        $(assert_eq!(format!($spec, $x), format!($spec, $y), "{} {}", $spec, $s);)+
    };
}

fn formatting<T: S>() {
    for s in ["nan", "-nan", "inf", "-inf"] {
        let (x, y) = (T::parse(s).unwrap(), s.parse::<f64>().unwrap());
        assert_same_format!(x, y, s;
            "{}", "{:?}", "{:+}", "{:>8}", "{:<8}", "{:^9}", "{:08}", "{:+08}", "{:*^7}", "{:8?}",
            "{:<8?}", "{:+?}", "{:#?}", "{:3}", "{:.2}", "{:+.0}", "{:>10.3}",
        );
        let (plain, padded) = x.stripped_texts();
        assert_eq!(plain, format!("{y}"));
        assert_eq!(padded, format!("{y:>8}"));
    }
    let payload_nan: T = snan(true, 3);
    assert_eq!(format!("{payload_nan}"), "NaN");
    assert_eq!(format!("{payload_nan:?}"), "NaN");
}

#[test]
fn special_values_format_like_f64() {
    formatting::<Decimal32>();
    formatting::<Decimal64>();
    formatting::<Decimal128>();
}

#[test]
fn special_constants_and_negation() {
    assert_eq!(
        out(Decimal32::NAN),
        Out::Nan {
            neg: false,
            signaling: false,
            payload: 0
        }
    );
    assert_eq!(
        out(-Decimal64::NAN),
        Out::Nan {
            neg: true,
            signaling: false,
            payload: 0
        }
    );
    assert_eq!(out(Decimal128::INFINITY), Out::Inf(false));
    assert_eq!(out(-Decimal128::INFINITY), Out::Inf(true));
    assert_eq!(out(Decimal64::NEG_INFINITY.abs()), Out::Inf(false));
    assert_eq!(
        out(Decimal64::INFINITY.copysign(Decimal64::MIN)),
        Out::Inf(true)
    );
    assert_eq!(
        out(Decimal32::NAN.copysign(Decimal32::NEG_INFINITY)),
        Out::Nan {
            neg: true,
            signaling: false,
            payload: 0
        }
    );
    assert_eq!(Decimal32::NEG_INFINITY.to_bits(), 0xf800_0000);
    assert_eq!(Decimal64::NAN.to_bits(), 0x7c00_0000_0000_0000);
    assert_eq!(
        Decimal128::INFINITY.to_bits(),
        0x7800_0000_0000_0000_0000_0000_0000_0000
    );
    let text: String = format!("{}", Decimal32::NEG_INFINITY);
    assert_eq!(text, "-inf");
}
