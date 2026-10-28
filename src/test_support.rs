use core::cmp::Ordering;
use core::hash::{Hash, Hasher};
use num_bigint::{BigInt, BigUint, Sign};
use std::borrow::Cow;
use std::collections::hash_map::DefaultHasher;
use std::vec::Vec;

use crate::layout::Unpacked;
use crate::refbid::{self, Classified, Finite, Format};
use crate::{Decimal32, Decimal64, Decimal128, Error, Rounding};

pub(crate) const FORMATS: [Format; 3] = [Format::D32, Format::D64, Format::D128];
pub(crate) const MODES: [Option<Rounding>; 3] = [
    None,
    Some(Rounding::TowardNegative),
    Some(Rounding::TowardPositive),
];

pub(crate) fn bounds(format: Format) -> (u32, i32, i32) {
    match format {
        Format::D32 => (7, -101, 90),
        Format::D64 => (16, -398, 369),
        Format::D128 => (34, -6176, 6111),
    }
}

pub(crate) struct Rng(pub(crate) u64);
impl Rng {
    pub(crate) fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    pub(crate) fn next_u128(&mut self) -> u128 {
        (u128::from(self.next()) << 64) | u128::from(self.next())
    }

    pub(crate) fn value(&mut self, format: Format) -> Finite {
        let (p, lo, hi) = bounds(format);
        let n = (self.next() % u64::from(p) + 1) as u32;
        let coeff = ((u128::from(self.next()) << 64) | u128::from(self.next())) % 10_u128.pow(n);
        Finite {
            neg: self.next() & 1 != 0,
            coeff,
            exp: lo + (self.next() % (hi - lo + 1) as u64) as i32,
        }
    }
}

pub(crate) fn finite(value: Unpacked) -> Finite {
    Finite {
        neg: value.neg,
        coeff: value.coeff,
        exp: value.exp,
    }
}

pub(crate) fn value(coeff: u128, exp: i32) -> Finite {
    Finite {
        neg: false,
        coeff,
        exp,
    }
}

pub(crate) fn coefficients(p: u32) -> Vec<u128> {
    let mut rng = Rng(0x4528_21e6_38d0_1377);
    let mut c = std::vec![0, 1];
    for n in 1..=p {
        let power = 10_u128.pow(n);
        let low = power / 10;
        c.extend([power - 1, low, low + 1, 5 * low]);
        for _ in 0..2 {
            c.push(low + ((u128::from(rng.next()) << 64) | u128::from(rng.next())) % (power - low));
        }
    }
    c.sort_unstable();
    c.dedup();
    c
}

pub(crate) fn classes(source: Format, target: Format) -> Vec<Finite> {
    let (p, lo, hi) = bounds(source);
    let (tp, tlo, thi) = bounds(target);
    let mut values = Vec::new();
    for coeff in coefficients(p) {
        let n = if coeff == 0 { 0 } else { coeff.ilog10() + 1 };
        let mut exps: Vec<i32> = (-(p as i32) - 1..=p as i32 + 1).collect();
        exps.extend([34, 35, 36, 37, 38, 39]);
        for delta in -3..=3 {
            exps.extend([lo + delta, hi + delta, tlo + delta, thi + delta]);
            exps.push(thi + tp as i32 - n as i32 + delta);
            exps.push(tlo - n as i32 + delta);
        }
        exps.retain(|&exp| (lo..=hi).contains(&exp));
        exps.sort_unstable();
        exps.dedup();
        for exp in exps {
            for neg in [false, true] {
                values.push(Finite { neg, coeff, exp });
            }
        }
    }
    values
}

pub(crate) fn d32(v: Finite) -> Decimal32 {
    Decimal32::from_bits(refbid::encode(v, Format::D32) as u32).unwrap()
}
pub(crate) fn d64(v: Finite) -> Decimal64 {
    Decimal64::from_bits(refbid::encode(v, Format::D64) as u64).unwrap()
}
pub(crate) fn d128(v: Finite) -> Decimal128 {
    Decimal128::from_bits(refbid::encode(v, Format::D128)).unwrap()
}
pub(crate) fn decoded(bits: u128, format: Format) -> Finite {
    match refbid::classify(bits, format) {
        Classified::Finite(v) => v,
        _ => panic!("unexpected rejected encoding {bits:x}"),
    }
}

/// A decoded production result: the fields of a finite value, or the special
/// value that an IEEE 754-2019 exception produces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Value {
    Finite(Finite),
    Infinity(bool),
    Nan,
}

pub(crate) fn classified(bits: u128, format: Format) -> Value {
    assert!(
        canonical_special(bits, format)
            || matches!(refbid::classify(bits, format), Classified::Finite(_)),
        "non-canonical result {bits:x}"
    );
    let (k, ..) = format.parameters();
    match refbid::classify(bits, format) {
        Classified::Finite(v) => Value::Finite(v),
        Classified::Infinity => Value::Infinity(bits >> (k - 1) != 0),
        Classified::Nan => Value::Nan,
        Classified::NonCanonical => panic!("non-canonical result {bits:x}"),
    }
}

/// Whether `bits` are a canonical infinity or NaN: an infinity with every
/// bit after the combination field clear, or a NaN with only the sign, the
/// signaling bit, and a payload below 10^(P-1) set.
pub(crate) fn canonical_special(bits: u128, format: Format) -> bool {
    let (k, _, t, _, _) = format.parameters();
    let (p, ..) = bounds(format);
    let sign = 1_u128 << (k - 1);
    let inf = 0b11110_u128 << (k - 6);
    let nan = 0b11111_u128 << (k - 6);
    let signaling = 1_u128 << (k - 7);
    let payload = (1_u128 << t) - 1;
    if bits & nan == nan {
        bits & !(sign | nan | signaling | payload) == 0 && bits & payload < 10_u128.pow(p - 1)
    } else {
        bits & inf == inf && bits & !sign == inf
    }
}

/// The encoding in `format` of the quiet NaN with sign `neg` and `payload`, or
/// of the infinity with sign `neg`.
pub(crate) fn special_bits(format: Format, nan: bool, neg: bool, payload: u128) -> u128 {
    let (k, ..) = format.parameters();
    let sign = u128::from(neg) << (k - 1);
    if nan {
        sign | (0b11111 << (k - 6)) | payload
    } else {
        sign | (0b11110 << (k - 6))
    }
}

/// The production result for an oracle result and the sign `neg` of the
/// result: a directed overflow is the IEEE 754-2019 infinity or largest
/// finite value for that direction and sign.
pub(crate) fn directed(
    oracle: Result<Finite, Error>,
    neg: bool,
    dir: Option<Rounding>,
    format: Format,
) -> Result<Value, Error> {
    let (p, _, hi) = bounds(format);
    match (oracle, dir) {
        (Ok(v), _) => Ok(Value::Finite(v)),
        (Err(Error::Overflow), Some(Rounding::TowardPositive)) if !neg => {
            Ok(Value::Infinity(false))
        }
        (Err(Error::Overflow), Some(Rounding::TowardNegative)) if neg => Ok(Value::Infinity(true)),
        (Err(Error::Overflow), Some(_)) => Ok(Value::Finite(Finite {
            neg,
            coeff: 10_u128.pow(p) - 1,
            exp: hi,
        })),
        (Err(e), _) => Err(e),
    }
}

pub(crate) fn hash(v: impl Hash) -> u64 {
    let mut state = DefaultHasher::new();
    v.hash(&mut state);
    state.finish()
}

pub(crate) struct Powers(Vec<BigUint>);
impl Powers {
    pub(crate) fn new(max: usize) -> Self {
        let mut powers = Vec::with_capacity(max + 1);
        powers.push(BigUint::from(1_u8));
        for i in 0..max {
            powers.push(&powers[i] * 10_u8);
        }
        Self(powers)
    }
    pub(crate) fn get(&self, n: i64) -> &BigUint {
        &self.0[usize::try_from(n).unwrap()]
    }
    pub(crate) fn pow(&self, n: i64) -> Cow<'_, BigUint> {
        let n = usize::try_from(n).unwrap();
        match self.0.get(n) {
            Some(power) => Cow::Borrowed(power),
            None => Cow::Owned(BigUint::from(10_u8).pow(u32::try_from(n).unwrap())),
        }
    }
    pub(crate) fn decimal_len(&self, c: &BigUint) -> i32 {
        if self.0.last().is_some_and(|largest| c >= largest) {
            c.to_str_radix(10).len() as i32
        } else {
            self.0.partition_point(|power| power <= c) as i32
        }
    }
}

pub(crate) fn integer_at(v: Finite, base: i32, powers: &Powers) -> BigInt {
    BigInt::from_biguint(
        if v.neg { Sign::Minus } else { Sign::Plus },
        BigUint::from(v.coeff) * powers.get(i64::from(v.exp) - i64::from(base)),
    )
}

pub(crate) fn compare(a: Finite, b: Finite, powers: &Powers) -> Ordering {
    let base = a.exp.min(b.exp);
    integer_at(a, base, powers).cmp(&integer_at(b, base, powers))
}

pub(crate) fn narrow(
    v: Finite,
    target: Format,
    dir: Option<Rounding>,
    powers: &Powers,
) -> Result<Finite, Error> {
    round_integer(
        BigInt::from_biguint(
            if v.neg { Sign::Minus } else { Sign::Plus },
            BigUint::from(v.coeff),
        ),
        v.exp,
        v.neg,
        target,
        dir,
        powers,
    )
}

pub(crate) fn round_integer(
    value: BigInt,
    source_exp: i32,
    zero_neg: bool,
    target: Format,
    dir: Option<Rounding>,
    powers: &Powers,
) -> Result<Finite, Error> {
    let (p, lo, hi) = bounds(target);
    if value.sign() == Sign::NoSign {
        return Ok(Finite {
            neg: zero_neg,
            coeff: 0,
            exp: source_exp.clamp(lo, hi),
        });
    }
    let neg = value.sign() == Sign::Minus;
    let c = value.magnitude();
    let max = powers.get(i64::from(p)) - 1_u8;
    let base = source_exp.min(hi);
    if dir.is_none()
        && c * &*powers.pow(i64::from(source_exp - base))
            > &max * &*powers.pow(i64::from(hi - base))
    {
        return Err(Error::Overflow);
    }
    let decimal_len = powers.decimal_len(c);
    let mut exp = lo.max(source_exp + decimal_len - p as i32);
    let (num, den) = if source_exp >= exp {
        (
            c * &*powers.pow(i64::from(source_exp - exp)),
            BigUint::from(1_u8),
        )
    } else {
        (
            c.clone(),
            powers.pow(i64::from(exp - source_exp)).into_owned(),
        )
    };
    let mut q = &num / &den;
    let r = &num % &den;
    if r != BigUint::from(0_u8) {
        match dir {
            None => return Err(Error::Inexact),
            Some(Rounding::TowardNegative) if neg => q += 1_u8,
            Some(Rounding::TowardPositive) if !neg => q += 1_u8,
            Some(_) => {}
        }
        if q > max {
            q /= 10_u8;
            exp += 1;
        }
    } else {
        let preferred = source_exp.clamp(lo, hi);
        if preferred > exp {
            q /= &*powers.pow(i64::from(preferred - exp));
            exp = preferred;
        }
    }
    if exp > hi {
        return Err(Error::Overflow);
    }
    Ok(Finite {
        neg,
        coeff: u128::try_from(q).unwrap(),
        exp,
    })
}
