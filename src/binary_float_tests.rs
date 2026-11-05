//! Independent exact-rational references for the four binary conversions.
//!
//! Expected values use only explicit format constants, `refbid`, and BigUint
//! integer arithmetic. In particular, neither reference calls a production
//! layout/kernel/converter, decimal parser, or binary floating-point parser.
//! The production from-path expands finite binary numbers in decimal; this
//! reference instead divides an exact N/D at the selected decimal unit.

use core::cmp::Ordering;
use num_bigint::BigUint;
use std::sync::OnceLock;
use std::time::Instant;
use std::vec::Vec;

use crate::refbid::{self, Classified, Finite, Format};
use crate::{Decimal32, Decimal64, Decimal128, Error};

const FORMATS: [Format; 3] = [Format::D32, Format::D64, Format::D128];
const RANDOM_CASES: usize = 10_000;

#[derive(Clone, Copy, Debug)]
struct Binary {
    fraction: u32,
    exponent: u32,
    bias: i32,
    digits: u32,
}

const F32: Binary = Binary {
    fraction: 23,
    exponent: 8,
    bias: 127,
    digits: 6,
};
const F64: Binary = Binary {
    fraction: 52,
    exponent: 11,
    bias: 1023,
    digits: 15,
};

impl Binary {
    fn sign(self) -> u64 {
        1 << (self.fraction + self.exponent)
    }
    fn fraction_mask(self) -> u64 {
        (1 << self.fraction) - 1
    }
    fn exponent_mask(self) -> u64 {
        (1 << self.exponent) - 1
    }
    fn infinity(self) -> u64 {
        self.exponent_mask() << self.fraction
    }
    fn quiet(self) -> u64 {
        1 << (self.fraction - 1)
    }
    fn min_normal(self) -> i32 {
        1 - self.bias
    }
    fn max_normal(self) -> i32 {
        self.exponent_mask() as i32 - 1 - self.bias
    }
    fn subnormal_unit(self) -> i32 {
        self.min_normal() - self.fraction as i32
    }
}

fn bounds(format: Format) -> (u32, i32, i32) {
    match format {
        Format::D32 => (7, -101, 90),
        Format::D64 => (16, -398, 369),
        Format::D128 => (34, -6176, 6111),
    }
}

fn width(format: Format) -> u32 {
    match format {
        Format::D32 => 32,
        Format::D64 => 64,
        Format::D128 => 128,
    }
}

fn powers() -> &'static [BigUint] {
    static POWERS: OnceLock<Vec<BigUint>> = OnceLock::new();
    POWERS.get_or_init(|| {
        let mut result = Vec::with_capacity(6177);
        result.push(BigUint::from(1_u8));
        for i in 1..=6176 {
            result.push(&result[i - 1] * 10_u8);
        }
        result
    })
}

fn pow10(n: u32) -> &'static BigUint {
    &powers()[n as usize]
}

#[derive(Clone, Debug)]
struct Rational {
    n: BigUint,
    d: BigUint,
}

impl Rational {
    fn decimal(coeff: u128, exp: i32) -> Self {
        if exp >= 0 {
            Self {
                n: BigUint::from(coeff) * pow10(exp as u32),
                d: BigUint::from(1_u8),
            }
        } else {
            Self {
                n: BigUint::from(coeff),
                d: pow10(exp.unsigned_abs()).clone(),
            }
        }
    }

    fn dyadic(significand: u64, exp: i32) -> Self {
        if exp >= 0 {
            Self {
                n: BigUint::from(significand) << exp as usize,
                d: BigUint::from(1_u8),
            }
        } else {
            Self {
                n: BigUint::from(significand),
                d: BigUint::from(1_u8) << exp.unsigned_abs() as usize,
            }
        }
    }

    fn cmp_decimal_power(&self, exp: i32) -> Ordering {
        if exp >= 0 {
            self.n.cmp(&(&self.d * pow10(exp as u32)))
        } else {
            (&self.n * pow10(exp.unsigned_abs())).cmp(&self.d)
        }
    }

    fn cmp_dyadic_power(&self, exp: i32) -> Ordering {
        if exp >= 0 {
            self.n.cmp(&(&self.d << exp as usize))
        } else {
            (&self.n << exp.unsigned_abs() as usize).cmp(&self.d)
        }
    }

    fn decimal_order(&self) -> i32 {
        // All finite binary32/64 inputs lie strictly between 10^-350 and 10^350.
        // An integer-only search is deliberately independent of float log10.
        let (mut low, mut high) = (-350, 350);
        assert!(self.cmp_decimal_power(low) != Ordering::Less);
        assert_eq!(self.cmp_decimal_power(high), Ordering::Less);
        while high - low > 1 {
            let mid = low + (high - low) / 2;
            if self.cmp_decimal_power(mid) == Ordering::Less {
                high = mid;
            } else {
                low = mid;
            }
        }
        low
    }

    fn binary_order(&self) -> i32 {
        let candidate = self.n.bits() as i32 - self.d.bits() as i32;
        candidate - i32::from(self.cmp_dyadic_power(candidate) == Ordering::Less)
    }

    fn round_decimal(&self, exp: i32) -> BigUint {
        if exp >= 0 {
            nearest_even(&self.n, &(&self.d * pow10(exp as u32)))
        } else {
            nearest_even(&(&self.n * pow10(exp.unsigned_abs())), &self.d)
        }
    }

    fn round_dyadic(&self, exp: i32) -> BigUint {
        if exp >= 0 {
            nearest_even(&self.n, &(&self.d << exp as usize))
        } else {
            nearest_even(&(&self.n << exp.unsigned_abs() as usize), &self.d)
        }
    }
}

fn nearest_even(n: &BigUint, d: &BigUint) -> BigUint {
    let mut quotient = n / d;
    let twice_remainder = (n % d) << 1_usize;
    if twice_remainder > *d || (twice_remainder == *d && quotient.bit(0)) {
        quotient += 1_u8;
    }
    quotient
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DecimalValue {
    Finite(Finite),
    Infinity(bool),
    Nan { neg: bool, payload: u128 },
}

fn decimal_special(format: Format, neg: bool, payload: Option<u128>, signaling: bool) -> u128 {
    let w = width(format);
    let sign = u128::from(neg) << (w - 1);
    match payload {
        None => sign | (30_u128 << (w - 6)),
        Some(payload) => sign | (31_u128 << (w - 6)) | (u128::from(signaling) << (w - 7)) | payload,
    }
}

fn decode_decimal(bits: u128, format: Format) -> Option<DecimalValue> {
    let neg = bits >> (width(format) - 1) != 0;
    match refbid::classify(bits, format) {
        Classified::Finite(v) => Some(DecimalValue::Finite(v)),
        Classified::NonCanonical => None,
        Classified::Infinity => (bits == decimal_special(format, neg, None, false))
            .then_some(DecimalValue::Infinity(neg)),
        Classified::Nan => {
            let trailing = match format {
                Format::D32 => 20,
                Format::D64 => 50,
                Format::D128 => 110,
            };
            let payload = bits & ((1_u128 << trailing) - 1);
            let signaling = bits & (1_u128 << (width(format) - 7)) != 0;
            (payload < 10_u128.pow(bounds(format).0 - 1)
                && bits == decimal_special(format, neg, Some(payload), signaling))
            .then_some(DecimalValue::Nan { neg, payload })
        }
    }
}

fn encode_decimal(v: DecimalValue, format: Format) -> u128 {
    match v {
        DecimalValue::Finite(v) => refbid::encode(v, format),
        DecimalValue::Infinity(neg) => decimal_special(format, neg, None, false),
        DecimalValue::Nan { neg, payload } => decimal_special(format, neg, Some(payload), false),
    }
}

fn round_to_decimal(value: &Rational, neg: bool, format: Format, digits: u32) -> DecimalValue {
    if value.n == BigUint::from(0_u8) {
        return DecimalValue::Finite(Finite {
            neg,
            coeff: 0,
            exp: 0,
        });
    }
    let (precision, qmin, qmax) = bounds(format);
    let mut exp = (value.decimal_order() - digits as i32 + 1).max(qmin);
    let rounded = value.round_decimal(exp);
    let mut coeff = u128::try_from(rounded).unwrap();
    if coeff == 0 {
        exp = 0;
    }
    // A carry at the precision boundary changes the decimal order by one.
    if coeff == 10_u128.pow(digits) {
        coeff /= 10;
        exp += 1;
    }
    while exp > qmax && coeff < 10_u128.pow(precision - 1) {
        coeff *= 10;
        exp -= 1;
    }
    if exp > qmax || coeff >= 10_u128.pow(precision) {
        return DecimalValue::Infinity(neg);
    }
    // Fractional trimming is specified independently, not called in the oracle.
    while coeff != 0 && exp < 0 && coeff.is_multiple_of(10) {
        coeff /= 10;
        exp += 1;
    }
    DecimalValue::Finite(Finite { neg, coeff, exp })
}

fn binary_finite(bits: u64, binary: Binary) -> Rational {
    let field = (bits >> binary.fraction) & binary.exponent_mask();
    let fraction = bits & binary.fraction_mask();
    assert_ne!(field, binary.exponent_mask());
    if field == 0 {
        Rational::dyadic(fraction, binary.subnormal_unit())
    } else {
        Rational::dyadic(
            (1 << binary.fraction) | fraction,
            field as i32 - binary.bias - binary.fraction as i32,
        )
    }
}

fn reference_from(bits: u64, binary: Binary, format: Format) -> DecimalValue {
    let neg = bits & binary.sign() != 0;
    let fraction = bits & binary.fraction_mask();
    if bits & binary.infinity() == binary.infinity() {
        if fraction == 0 {
            return DecimalValue::Infinity(neg);
        }
        let payload = u128::from(fraction & (binary.quiet() - 1));
        return DecimalValue::Nan {
            neg,
            payload: if payload < 10_u128.pow(bounds(format).0 - 1) {
                payload
            } else {
                0
            },
        };
    }
    round_to_decimal(
        &binary_finite(bits, binary),
        neg,
        format,
        bounds(format).0.min(binary.digits),
    )
}

fn reference_to_rational(value: &Rational, neg: bool, binary: Binary) -> u64 {
    let sign = if neg { binary.sign() } else { 0 };
    if value.n == BigUint::from(0_u8) {
        return sign;
    }
    let mut order = value.binary_order();
    if order > binary.max_normal() {
        return sign | binary.infinity();
    }
    let unit = (order - binary.fraction as i32).max(binary.subnormal_unit());
    let mut significand = u64::try_from(value.round_dyadic(unit)).unwrap();
    if significand == 0 {
        return sign;
    }
    if order < binary.min_normal() {
        // The rounded subnormal integer can reach exactly the first normal.
        return sign | significand;
    }
    if significand == 1 << (binary.fraction + 1) {
        significand >>= 1;
        order += 1;
    }
    if order > binary.max_normal() {
        return sign | binary.infinity();
    }
    sign | (((order + binary.bias) as u64) << binary.fraction)
        | (significand - (1 << binary.fraction))
}

fn reference_to(value: DecimalValue, binary: Binary) -> u64 {
    match value {
        DecimalValue::Finite(v) => {
            reference_to_rational(&Rational::decimal(v.coeff, v.exp), v.neg, binary)
        }
        DecimalValue::Infinity(neg) => (if neg { binary.sign() } else { 0 }) | binary.infinity(),
        DecimalValue::Nan { neg, payload } => {
            let payload = if payload < u128::from(binary.quiet()) {
                payload as u64
            } else {
                0
            };
            (if neg { binary.sign() } else { 0 }) | binary.infinity() | binary.quiet() | payload
        }
    }
}

fn actual_to(bits: u128, format: Format) -> Result<(u64, u64), Error> {
    match format {
        Format::D32 => Decimal32::from_bits(bits as u32).map(|v| {
            assert_eq!(u128::from(v.to_bits()), bits);
            (u64::from(v.to_f32().to_bits()), v.to_f64().to_bits())
        }),
        Format::D64 => Decimal64::from_bits(bits as u64).map(|v| {
            assert_eq!(u128::from(v.to_bits()), bits);
            (u64::from(v.to_f32().to_bits()), v.to_f64().to_bits())
        }),
        Format::D128 => Decimal128::from_bits(bits).map(|v| {
            assert_eq!(v.to_bits(), bits);
            (u64::from(v.to_f32().to_bits()), v.to_f64().to_bits())
        }),
    }
}

fn actual_from(bits: u64, binary: Binary, format: Format) -> u128 {
    match (binary.fraction, format) {
        (23, Format::D32) => u128::from(Decimal32::from_f32(f32::from_bits(bits as u32)).to_bits()),
        (23, Format::D64) => u128::from(Decimal64::from_f32(f32::from_bits(bits as u32)).to_bits()),
        (23, Format::D128) => Decimal128::from_f32(f32::from_bits(bits as u32)).to_bits(),
        (52, Format::D32) => u128::from(Decimal32::from_f64(f64::from_bits(bits)).to_bits()),
        (52, Format::D64) => u128::from(Decimal64::from_f64(f64::from_bits(bits)).to_bits()),
        (52, Format::D128) => Decimal128::from_f64(f64::from_bits(bits)).to_bits(),
        _ => unreachable!(),
    }
}

fn check_to_raw(bits: u128, format: Format) -> Option<(u64, u64)> {
    let expected = decode_decimal(bits, format);
    let actual = actual_to(bits, format);
    if let Some(expected) = expected {
        let pair = match expected {
            DecimalValue::Finite(v) => {
                let rational = Rational::decimal(v.coeff, v.exp);
                (
                    reference_to_rational(&rational, v.neg, F32),
                    reference_to_rational(&rational, v.neg, F64),
                )
            }
            _ => (reference_to(expected, F32), reference_to(expected, F64)),
        };
        assert_eq!(actual, Ok(pair), "D{} raw={bits:032x}", width(format));
        Some(pair)
    } else {
        assert_eq!(
            actual,
            Err(Error::InvalidInput),
            "D{} noncanonical raw={bits:032x}",
            width(format)
        );
        None
    }
}

fn check_to(value: Finite, format: Format) {
    check_to_raw(refbid::encode(value, format), format);
}

fn check_from(bits: u64, binary: Binary, format: Format) -> u128 {
    let expected = encode_decimal(reference_from(bits, binary, format), format);
    let actual = actual_from(bits, binary, format);
    assert_eq!(
        actual,
        expected,
        "binary{} raw={bits:016x} -> D{}",
        binary.fraction + binary.exponent + 1,
        width(format)
    );
    actual
}

fn check_from_f32_all(bits: u32) -> u128 {
    // All decimal widths cover the entire binary32 finite range at six digits.
    // Only special payload fit differs, so share one finite rational division.
    let reference = reference_from(u64::from(bits), F32, Format::D128);
    let mut checksum = 0;
    for format in FORMATS {
        let expected = match reference {
            DecimalValue::Nan { neg, payload } if payload >= 10_u128.pow(bounds(format).0 - 1) => {
                DecimalValue::Nan { neg, payload: 0 }
            }
            _ => reference,
        };
        let expected = encode_decimal(expected, format);
        let actual = actual_from(u64::from(bits), F32, format);
        assert_eq!(actual, expected, "f32 raw={bits:08x} -> D{}", width(format));
        checksum ^= actual.rotate_left(width(format) / 4);
    }
    checksum
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    fn wide(&mut self) -> u128 {
        (u128::from(self.next()) << 64) | u128::from(self.next())
    }
    fn finite(&mut self, format: Format) -> Finite {
        let (p, lo, hi) = bounds(format);
        let digits = 1 + self.next() as u32 % p;
        Finite {
            neg: self.next() & 1 != 0,
            coeff: self.wide() % 10_u128.pow(digits),
            exp: lo + (self.next() % (hi - lo + 1) as u64) as i32,
        }
    }
}

#[test]
fn binary_float_exact_rational_oracle_known_bit_anchors() {
    assert_eq!(
        reference_to_rational(&Rational::decimal(1, -1), false, F32),
        0x3dcc_cccd
    );
    assert_eq!(
        reference_to_rational(&Rational::decimal(1, -1), false, F64),
        0x3fb9_9999_9999_999a
    );
    for binary in [F32, F64] {
        let one = (binary.bias as u64) << binary.fraction;
        let lower_even_tie = Rational::dyadic(
            (1 << (binary.fraction + 1)) + 1,
            -(binary.fraction as i32) - 1,
        );
        let upper_even_tie = Rational::dyadic(
            (1 << (binary.fraction + 1)) + 3,
            -(binary.fraction as i32) - 1,
        );
        let overflow_numerator = (1 << (binary.fraction + 2)) - 1;
        let overflow_unit = binary.max_normal() - binary.fraction as i32 - 1;
        let cases = [
            (Rational::dyadic(1, 0), one),
            (lower_even_tie, one),
            (upper_even_tie, one + 2),
            (Rational::dyadic(1, binary.subnormal_unit() - 1), 0),
            (Rational::dyadic(3, binary.subnormal_unit() - 1), 2),
            (
                Rational::dyadic(
                    (1 << (binary.fraction + 1)) - 1,
                    binary.subnormal_unit() - 1,
                ),
                1 << binary.fraction,
            ),
            (
                Rational::dyadic(overflow_numerator, overflow_unit),
                binary.infinity(),
            ),
            (
                Rational::dyadic(overflow_numerator * 2 - 1, overflow_unit - 1),
                binary.infinity() - 1,
            ),
        ];
        for (rational, expected) in cases {
            for neg in [false, true] {
                assert_eq!(
                    reference_to_rational(&rational, neg, binary),
                    expected | if neg { binary.sign() } else { 0 }
                );
            }
        }
    }
}

#[test]
fn binary_float_fixed_approved_examples() {
    let examples = [
        (0.1_f64.to_bits(), 1, -1),
        ((0.1_f64 + 0.2).to_bits(), 3, -1),
        (86.296_f64.to_bits(), 86_296, -3),
        (1e20_f64.to_bits(), 100_000_000_000_000, 6),
        (
            1.234_567_890_123_456_8e17_f64.to_bits(),
            123_456_789_012_346,
            3,
        ),
        (1, 494_065_645_841_247, -338),
    ];
    for format in [Format::D64, Format::D128] {
        for (bits, coeff, exp) in examples {
            for neg in [false, true] {
                let bits = bits | if neg { F64.sign() } else { 0 };
                let expected = refbid::encode(Finite { neg, coeff, exp }, format);
                assert_eq!(actual_from(bits, F64, format), expected);
                assert_eq!(
                    encode_decimal(reference_from(bits, F64, format), format),
                    expected
                );
            }
        }
    }
    for format in FORMATS {
        for neg in [false, true] {
            let bits = u64::from(0.1_f32.to_bits()) | if neg { F32.sign() } else { 0 };
            assert_eq!(
                actual_from(bits, F32, format),
                refbid::encode(
                    Finite {
                        neg,
                        coeff: 1,
                        exp: -1
                    },
                    format
                )
            );
        }
    }
}

#[test]
fn binary_float_to_all_decimal_exponents_and_coefficient_extrema() {
    for format in FORMATS {
        let (p, lo, hi) = bounds(format);
        let top = 10_u128.pow(p);
        let coefficients = [
            0,
            1,
            2,
            9,
            10,
            top / 10 - 1,
            top / 10,
            top / 10 + 1,
            top - 2,
            top - 1,
        ];
        for exp in lo..=hi {
            for coeff in coefficients {
                for neg in [false, true] {
                    check_to(Finite { neg, coeff, exp }, format);
                }
            }
        }
    }
}

#[test]
fn binary_float_to_fixed_seed_raw_and_finite_10000_per_width() {
    for format in FORMATS {
        let mut raw = Rng(0x493d_bd15_b965_b735 ^ u64::from(width(format)));
        let mut finite = Rng(0xe85c_c982_673d_5e25 ^ u64::from(width(format)));
        for _ in 0..RANDOM_CASES {
            check_to_raw(raw.wide() & (u128::MAX >> (128 - width(format))), format);
            check_to(finite.finite(format), format);
        }
    }
}

#[test]
fn binary_float_from_fixed_seed_10000_per_method_width() {
    let mut binary32 = Rng(0x343e_d1d4_be93_45ac);
    for _ in 0..RANDOM_CASES {
        check_from_f32_all(binary32.next() as u32);
    }
    for format in FORMATS {
        let mut binary64 = Rng(0x9645_58bb_6d1b_c9d0 ^ u64::from(width(format)));
        for _ in 0..RANDOM_CASES {
            check_from(binary64.next(), F64, format);
        }
    }
    // Separate finite-only streams complement the raw encodings above.
    let mut finite32 = Rng(0x58e8_9420_fa51_ba25);
    let mut checked = 0;
    while checked < RANDOM_CASES {
        let bits = finite32.next() as u32;
        if u64::from(bits) & F32.infinity() == F32.infinity() {
            continue;
        }
        check_from_f32_all(bits);
        checked += 1;
    }
    for format in FORMATS {
        let mut finite64 = Rng(0x8e36_910b_f3ac_a529 ^ u64::from(width(format)));
        let mut checked = 0;
        while checked < RANDOM_CASES {
            let bits = finite64.next();
            if bits & F64.infinity() == F64.infinity() {
                continue;
            }
            check_from(bits, F64, format);
            checked += 1;
        }
    }
}

#[test]
fn binary_float_from_every_binary_exponent_and_mantissa_classes() {
    for binary in [F32, F64] {
        let mask = binary.fraction_mask();
        for exponent in 0..=binary.exponent_mask() {
            for fraction in [
                0,
                1,
                2,
                mask / 2 - 1,
                mask / 2,
                mask / 2 + 1,
                mask - 2,
                mask - 1,
                mask,
            ] {
                for neg in [false, true] {
                    let bits = (exponent << binary.fraction)
                        | fraction
                        | if neg { binary.sign() } else { 0 };
                    if binary.fraction == 23 {
                        check_from_f32_all(bits as u32);
                    } else {
                        for format in FORMATS {
                            check_from(bits, binary, format);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn binary_float_signed_zero_infinity_nan_and_payload_boundaries() {
    for format in FORMATS {
        let (p, lo, hi) = bounds(format);
        for neg in [false, true] {
            for exp in [lo, -1, 0, 1, hi] {
                check_to(Finite { neg, coeff: 0, exp }, format);
            }
            check_to_raw(decimal_special(format, neg, None, false), format);
            let mut payloads = std::vec![0, 1, 2, 10_u128.pow(p - 1) - 1];
            for binary in [F32, F64] {
                payloads.extend([
                    u128::from(binary.quiet()) - 1,
                    u128::from(binary.quiet()),
                    u128::from(binary.quiet()) + 1,
                ]);
            }
            for payload in payloads {
                if payload >= 10_u128.pow(p - 1) {
                    continue;
                }
                for signaling in [false, true] {
                    check_to_raw(
                        decimal_special(format, neg, Some(payload), signaling),
                        format,
                    );
                }
            }
            for binary in [F32, F64] {
                let sign = if neg { binary.sign() } else { 0 };
                check_from(sign, binary, format);
                check_from(sign | binary.infinity(), binary, format);
                let decimal_limit = 10_u128.pow(p - 1);
                let mut payloads = std::vec![0, 1, 2, u128::from(binary.quiet()) - 1];
                payloads.extend([decimal_limit - 1, decimal_limit, decimal_limit + 1]);
                for payload in payloads {
                    if payload >= u128::from(binary.quiet()) {
                        continue;
                    }
                    for quiet in [false, true] {
                        if payload == 0 && !quiet {
                            continue;
                        }
                        check_from(
                            sign | binary.infinity()
                                | if quiet { binary.quiet() } else { 0 }
                                | payload as u64,
                            binary,
                            format,
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn binary_float_from_15_7_6_digit_ties_and_adjacent_binary_values() {
    let cases = [
        (
            F64,
            Format::D64,
            1_234_567_890_123_445_u128,
            0,
            123_456_789_012_344_u128,
            1,
        ),
        (
            F64,
            Format::D128,
            1_234_567_890_123_445,
            0,
            123_456_789_012_344,
            1,
        ),
        (
            F64,
            Format::D64,
            1_234_567_890_123_455,
            0,
            123_456_789_012_346,
            1,
        ),
        (
            F64,
            Format::D128,
            1_234_567_890_123_455,
            0,
            123_456_789_012_346,
            1,
        ),
        (F64, Format::D32, 12_345_645, 0, 1_234_564, 1),
        (F64, Format::D32, 12_345_655, 0, 1_234_566, 1),
        (F32, Format::D32, 1_234_565, -1, 123_456, 0),
        (F32, Format::D64, 1_234_565, -1, 123_456, 0),
        (F32, Format::D128, 1_234_565, -1, 123_456, 0),
        (F32, Format::D32, 1_234_575, -1, 123_458, 0),
        (F32, Format::D64, 1_234_575, -1, 123_458, 0),
        (F32, Format::D128, 1_234_575, -1, 123_458, 0),
    ];
    for (binary, format, coeff, exp, want_coeff, want_exp) in cases {
        let rational = Rational::decimal(coeff, exp);
        let center = reference_to_rational(&rational, false, binary);
        let recovered = binary_finite(center, binary);
        assert_eq!(
            &rational.n * &recovered.d,
            &recovered.n * &rational.d,
            "tie must be exact binary"
        );
        for neg in [false, true] {
            let sign = if neg { binary.sign() } else { 0 };
            assert_eq!(
                actual_from(center | sign, binary, format),
                refbid::encode(
                    Finite {
                        neg,
                        coeff: want_coeff,
                        exp: want_exp
                    },
                    format
                )
            );
            for magnitude in center - 2..=center + 2 {
                check_from(magnitude | sign, binary, format);
            }
        }
    }
    // Carries at a decimal precision boundary, including an exact integer tie.
    for binary in [F32, F64] {
        for format in FORMATS {
            let digits = bounds(format).0.min(binary.digits);
            let center = reference_to_rational(
                &Rational::decimal(10_u128.pow(digits + 1) - 5, 0),
                false,
                binary,
            );
            for magnitude in center - 3..=center + 3 {
                for neg in [false, true] {
                    check_from(
                        magnitude | if neg { binary.sign() } else { 0 },
                        binary,
                        format,
                    );
                }
            }
        }
    }
}

#[test]
fn binary_float_decimal32_from_f64_overflow_and_single_rounding_underflow() {
    // The exponent floor participates in the SAME nearest-even decision.
    // These neighborhoods include zero, the first subnormals, and overflow.
    let boundaries = [
        (5, -102),
        (15, -102),
        (25, -102),
        (99_999_995, 89),
        (1, -101),
        (9_999_999, 90),
    ];
    for (coeff, exp) in boundaries {
        let center = reference_to_rational(&Rational::decimal(coeff, exp), false, F64);
        for magnitude in center - 8..=center + 8 {
            for neg in [false, true] {
                check_from(
                    magnitude | if neg { F64.sign() } else { 0 },
                    F64,
                    Format::D32,
                );
            }
        }
    }
    for magnitude in [1, 2, 3, F64.infinity() - 1, F64.infinity() - 2] {
        for neg in [false, true] {
            check_from(
                magnitude | if neg { F64.sign() } else { 0 },
                F64,
                Format::D32,
            );
        }
    }
}

#[test]
fn binary_float_to_binary_range_boundaries_and_direct_f32_rounding() {
    for binary in [F32, F64] {
        let fractions = binary.fraction;
        let thresholds = [
            Rational::dyadic(1, binary.subnormal_unit() - 1),
            Rational::dyadic(3, binary.subnormal_unit() - 1),
            Rational::dyadic((1 << (fractions + 1)) - 1, binary.subnormal_unit() - 1),
            Rational::dyadic(
                (1 << (fractions + 2)) - 1,
                binary.max_normal() - fractions as i32 - 1,
            ),
            Rational::dyadic(1, binary.min_normal()),
            Rational::dyadic(
                (1 << (fractions + 1)) - 1,
                binary.max_normal() - fractions as i32,
            ),
        ];
        for threshold in thresholds {
            for format in FORMATS {
                let (p, lo, hi) = bounds(format);
                let center = round_to_decimal(&threshold, false, format, p);
                if let DecimalValue::Finite(center) = center {
                    for delta in -2_i128..=2 {
                        let coeff = center.coeff as i128 + delta;
                        if coeff < 0 || coeff as u128 >= 10_u128.pow(p) {
                            continue;
                        }
                        for neg in [false, true] {
                            check_to(
                                Finite {
                                    neg,
                                    coeff: coeff as u128,
                                    ..center
                                },
                                format,
                            );
                        }
                    }
                    assert!((lo..=hi).contains(&center.exp));
                }
            }
        }
    }
    // 1 + 2^-24 is an exactly representable f32 midpoint. The ±10^-33
    // perturbations vanish when rounded to f64, but MUST affect direct f32.
    let middle = 10_u128.pow(33) + 5_u128.pow(24) * 10_u128.pow(9);
    for (coeff, expected) in [
        (middle - 1, 0x3f80_0000),
        (middle, 0x3f80_0000),
        (middle + 1, 0x3f80_0001),
    ] {
        for neg in [false, true] {
            let bits = refbid::encode(
                Finite {
                    neg,
                    coeff,
                    exp: -33,
                },
                Format::D128,
            );
            let actual = check_to_raw(bits, Format::D128).unwrap().0;
            assert_eq!(actual, expected | if neg { F32.sign() } else { 0 });
        }
    }
}

fn normalized(mut v: Finite) -> Finite {
    if v.coeff == 0 {
        v.exp = 0;
        return v;
    }
    while v.coeff.is_multiple_of(10) {
        v.coeff /= 10;
        v.exp += 1;
    }
    v
}

fn independent_cmp(a: &Finite, b: &Finite) -> Ordering {
    if a.coeff == 0 && b.coeff == 0 {
        return Ordering::Equal;
    }
    if a.neg != b.neg {
        return if a.neg {
            Ordering::Less
        } else {
            Ordering::Greater
        };
    }
    let magnitude = if a.coeff == 0 {
        Ordering::Less
    } else if b.coeff == 0 {
        Ordering::Greater
    } else {
        let ad = a.coeff.ilog10() as i32 + 1;
        let bd = b.coeff.ilog10() as i32 + 1;
        (a.exp + ad).cmp(&(b.exp + bd)).then_with(|| {
            // Equal adjusted exponents need at most 34 coefficient digits.
            if ad < bd {
                (a.coeff * 10_u128.pow((bd - ad) as u32)).cmp(&b.coeff)
            } else {
                a.coeff.cmp(&(b.coeff * 10_u128.pow((ad - bd) as u32)))
            }
        })
    };
    if a.neg {
        magnitude.reverse()
    } else {
        magnitude
    }
}

fn in_common_normal_range(v: Finite, binary: Binary) -> bool {
    if v.coeff == 0 {
        return false;
    }
    let rational = Rational::decimal(v.coeff, v.exp);
    if rational.cmp_dyadic_power(binary.min_normal()) == Ordering::Less {
        return false;
    }
    let maximum = binary_finite(binary.infinity() - 1, binary);
    &rational.n * &maximum.d <= &maximum.n * &rational.d
}

#[test]
fn binary_float_limited_digit_decimal_roundtrips_numeric_not_cohort() {
    for binary in [F32, F64] {
        for format in FORMATS {
            let (p, qmin, qmax) = bounds(format);
            let digits = p.min(binary.digits);
            let (low, high) = if binary.fraction == 23 {
                (-37, 38)
            } else {
                (-307, 308)
            };
            let (low, high) = (qmin.max(low), qmax.min(high));
            let mut rng =
                Rng(0x8cd6_b952_74b3_b5fd ^ u64::from(width(format)) ^ u64::from(binary.fraction));
            let mut checked = 0;
            while checked < RANDOM_CASES {
                let count = 1 + rng.next() as u32 % digits;
                let value = Finite {
                    neg: rng.next() & 1 != 0,
                    coeff: 1 + rng.wide() % (10_u128.pow(count) - 1),
                    exp: low + (rng.next() % (high - low + 1) as u64) as i32,
                };
                // Eligibility is evaluated from exact N/D, never production.
                if !in_common_normal_range(value, binary) {
                    continue;
                }
                let pair = check_to_raw(refbid::encode(value, format), format).unwrap();
                let converted = if binary.fraction == 23 {
                    pair.0
                } else {
                    pair.1
                };
                let returned = check_from(converted, binary, format);
                let DecimalValue::Finite(returned) = decode_decimal(returned, format).unwrap()
                else {
                    panic!("finite roundtrip became special");
                };
                assert_eq!(
                    normalized(returned),
                    normalized(value),
                    "D{} via binary{} source={value:?}",
                    width(format),
                    binary.fraction + binary.exponent + 1
                );
                checked += 1;
            }
        }
    }
}

#[test]
fn binary_float_to_monotone_in_independently_sorted_decimal_order() {
    for format in FORMATS {
        let mut rng = Rng(0x4039_c24c_a048_42ab ^ u64::from(width(format)));
        let mut values: Vec<_> = (0..4096).map(|_| rng.finite(format)).collect();
        let (p, lo, hi) = bounds(format);
        for neg in [false, true] {
            for exp in [lo, 0, hi] {
                for coeff in [0, 1, 10_u128.pow(p) - 1] {
                    values.push(Finite { neg, coeff, exp });
                }
            }
        }
        values.sort_unstable_by(independent_cmp);
        let mut previous = (f32::NEG_INFINITY, f64::NEG_INFINITY);
        for value in values {
            let pair = check_to_raw(refbid::encode(value, format), format).unwrap();
            let current = (f32::from_bits(pair.0 as u32), f64::from_bits(pair.1));
            assert!(
                previous.0 <= current.0 && previous.1 <= current.1,
                "D{} source={value:?}",
                width(format)
            );
            previous = current;
        }
    }
}

fn exhaustive_decimal32_lane(negative: bool) {
    powers();
    let started = Instant::now();
    let sign = if negative { 1_u32 << 31 } else { 0 };
    let (mut accepted, mut rejected, mut checksum) = (0_u64, 0_u64, 0_u64);
    for magnitude in 0..1_u64 << 31 {
        let bits = sign | magnitude as u32;
        if let Some((f32, f64)) = check_to_raw(u128::from(bits), Format::D32) {
            accepted += 1;
            checksum = checksum.wrapping_add(f32.rotate_left(13) ^ f64);
        } else {
            rejected += 1;
        }
        if (magnitude + 1).is_multiple_of(1 << 26) {
            std::eprintln!(
                "d32 exhaustive sign={} checked={} accepted={} rejected={} checksum={checksum:016x} elapsed={:?}",
                u8::from(negative),
                magnitude + 1,
                accepted,
                rejected,
                started.elapsed()
            );
        }
    }
    assert_eq!(accepted + rejected, 1 << 31);
    // Per sign: 192 quantums × 10^7 finite coefficients, one infinity,
    // and two NaN signaling states × 10^6 canonical integer payloads.
    assert_eq!(accepted, 1_922_000_001);
    assert_eq!(rejected, 225_483_647);
    std::eprintln!(
        "d32 exhaustive COMPLETE sign={} checked={} accepted={} rejected={} checksum={checksum:016x} elapsed={:?}",
        u8::from(negative),
        accepted + rejected,
        accepted,
        rejected,
        started.elapsed()
    );
}

fn exhaustive_f32_lane(negative: bool) {
    powers();
    let started = Instant::now();
    let sign = if negative { 1_u32 << 31 } else { 0 };
    let mut checksum = 0_u128;
    for magnitude in 0..1_u64 << 31 {
        checksum = checksum.wrapping_add(check_from_f32_all(sign | magnitude as u32));
        if (magnitude + 1).is_multiple_of(1 << 26) {
            std::eprintln!(
                "f32 exhaustive sign={} checked={} destination_checks={} checksum={checksum:032x} elapsed={:?}",
                u8::from(negative),
                magnitude + 1,
                (magnitude + 1) * 3,
                started.elapsed()
            );
        }
    }
    std::eprintln!(
        "f32 exhaustive COMPLETE sign={} checked={} destination_checks={} checksum={checksum:032x} elapsed={:?}",
        u8::from(negative),
        1_u64 << 31,
        3_u64 << 31,
        started.elapsed()
    );
}

#[test]
#[ignore = "all 2^31 nonnegative D32 raw patterns; independent validity and both to outputs"]
fn binary_float_exhaustive_decimal32_positive() {
    exhaustive_decimal32_lane(false);
}
#[test]
#[ignore = "all 2^31 negative D32 raw patterns; independent validity and both to outputs"]
fn binary_float_exhaustive_decimal32_negative() {
    exhaustive_decimal32_lane(true);
}
#[test]
#[ignore = "all 2^31 nonnegative f32 raw patterns against all three decimal destinations"]
fn binary_float_exhaustive_f32_positive() {
    exhaustive_f32_lane(false);
}
#[test]
#[ignore = "all 2^31 negative f32 raw patterns against all three decimal destinations"]
fn binary_float_exhaustive_f32_negative() {
    exhaustive_f32_lane(true);
}

// Check the numerical premises of the manual integer arguments separately
// from the independent conversion oracle. Production layout constants are
// read only here; they do not supply any oracle expectation above.
#[test]
fn binary_float_integer_argument_premises() {
    use crate::layout::{W32, W64, W128, Width};

    assert_eq!(f32::DIGITS, 6);
    assert_eq!(f64::DIGITS, 15);
    let layouts = [W32::LAYOUT, W64::LAYOUT, W128::LAYOUT];
    for (layout, format) in layouts.iter().zip(FORMATS) {
        assert_eq!((layout.digits, layout.qmin, layout.qmax), bounds(format));
        assert!(layout.qmin <= 0 && 0 <= layout.qmax);
    }

    // Multiplication is monotone in the positive significand. The largest
    // possible significand bounds every input at every expansion exponent.
    let max_significand = (BigUint::from(1_u8) << 53_usize) - BigUint::from(1_u8);
    assert!(max_significand < BigUint::from(10_u8).pow(18));
    let mut expanded = max_significand.clone();
    for _ in 0..=1074 {
        assert!(expanded < BigUint::from(10_u8).pow(767));
        let digits = expanded.to_str_radix(10).len();
        assert!(digits <= 86 * 9);
        expanded *= 5_u8;
    }
    expanded = max_significand;
    for _ in 0..=971 {
        assert!(expanded < BigUint::from(10_u8).pow(309));
        expanded *= 2_u8;
    }

    let factors = (0..=27)
        .map(|k| 5_u64.pow(k))
        .chain((0..=63).map(|k| 1_u64 << k));
    for factor in factors {
        assert!(factor <= 1 << 63);
        let product = (1_000_000_000_u128 - 1) * u128::from(factor) + u128::from(factor - 1);
        assert!(product < 1_u128 << 93);
        assert!(product / 1_000_000_000 < u128::from(factor));
        assert!(product % 1_000_000_000 < 1_000_000_000);
    }

    for binary in [F32, F64] {
        for raw_exponent in 0..binary.exponent_mask() {
            let e = if raw_exponent == 0 {
                binary.subnormal_unit()
            } else {
                raw_exponent as i32 - binary.bias - binary.fraction as i32
            };
            assert!((-1074..=971).contains(&e));
            for trailing in 0..=binary.fraction {
                let cancelled = if e < 0 {
                    trailing.min(e.unsigned_abs())
                } else {
                    0
                };
                assert!(cancelled <= 52);
                assert!((-1074..=971).contains(&(e + cancelled as i32)));
            }
        }
    }

    // Enumerate the complete length/quantum/precision grid admitted by the
    // expansion bound, including unreachable combinations for a superset.
    for layout in layouts {
        for source_precision in [f32::DIGITS, f64::DIGITS] {
            let precision = layout.digits.min(source_precision) as usize;
            assert!((6..=15).contains(&precision));
            assert!(10_u128.pow(precision as u32) < u128::MAX);
            for len in 1_usize..=767 {
                for quantum in -1074_i32..=0 {
                    let drop = len
                        .saturating_sub(precision)
                        .max((layout.qmin - quantum).max(0) as usize);
                    let keep = len.saturating_sub(drop);
                    assert!(keep <= precision && keep <= len);
                    assert!(drop <= 1074);
                    let exponent = quantum + drop as i32;
                    assert!(exponent >= layout.qmin);
                    assert!(exponent.checked_add(1).is_some());
                    if drop > 0 && drop <= len {
                        assert!(keep < len);
                    }
                }
            }
            for padding in 0..layout.digits {
                assert!(10_u128.checked_pow(padding).is_some());
            }
        }
        for quantum in layout.qmin..=layout.qmax {
            let max_coefficient = 10_u128.pow(layout.digits) - 1;
            let text = std::format!("-{max_coefficient}e{quantum}");
            assert!(text.is_ascii() && text.len() <= 41 && text.len() < 48);
        }
    }

    // Guard/sticky comparisons cover all remainder classes: below half,
    // exactly half, and above half, for both retained parities.
    for parity in 0..=1 {
        for guard in 0..=9 {
            for tail in 0..=9 {
                let increment = guard > 5 || (guard == 5 && (tail != 0 || parity != 0));
                let remainder = guard * 10 + tail;
                assert_eq!(
                    increment,
                    remainder > 50 || (remainder == 50 && parity != 0)
                );
            }
        }
    }
}

#[test]
fn binary_float_clinger_boundaries_and_generated_tables() {
    for (limit, max_power) in [(1_u128 << 53, 22_i32), (1_u128 << 24, 10)] {
        for coeff in [limit - 1, limit, limit + 1] {
            for exp in [
                -max_power - 1,
                -max_power,
                -1,
                0,
                1,
                max_power,
                max_power + 1,
            ] {
                for format in FORMATS {
                    if coeff >= 10_u128.pow(bounds(format).0) {
                        continue;
                    }
                    for neg in [false, true] {
                        check_to(Finite { neg, coeff, exp }, format);
                    }
                }
            }
        }
    }
    for coeff in [
        u128::from(u64::MAX) - 1,
        u128::from(u64::MAX),
        u128::from(u64::MAX) + 1,
    ] {
        for exp in [-28, -27, -26, -18, -17, -16, -11, -10, -1, 0, 1] {
            for neg in [false, true] {
                check_to(Finite { neg, coeff, exp }, Format::D128);
            }
        }
    }
    let mut power = BigUint::from(1_u8);
    for &actual in &crate::tables::BINARY_POW5 {
        assert_eq!(BigUint::from(actual), power);
        power *= 5_u8;
    }
    for (k, &actual) in crate::tables::F64_POW10.iter().enumerate() {
        let exact = BigUint::from(10_u8).pow(k as u32);
        let rational = binary_finite(actual.to_bits(), F64);
        assert_eq!(rational.n, exact * rational.d);
    }
    for (k, &actual) in crate::tables::F32_POW10.iter().enumerate() {
        let exact = BigUint::from(10_u8).pow(k as u32);
        let rational = binary_finite(u64::from(actual.to_bits()), F32);
        assert_eq!(rational.n, exact * rational.d);
    }
}

#[test]
fn binary_float_fixed_width_and_chunk_transition_boundaries() {
    for binary in [F32, F64] {
        for width in 1..=binary.fraction + 1 {
            let m = (1_u64 << width) - 1;
            let shift = (binary.fraction + 1 - width) as i32;
            let fixed_limit = 128 - width as i32;
            let negative_limit = (0..=55)
                .take_while(|&k| width + 5_u128.pow(k).bit_width() <= 128)
                .last()
                .unwrap() as i32;
            for exponent in [
                fixed_limit - 1,
                fixed_limit,
                fixed_limit + 1,
                -negative_limit - 1,
                -negative_limit,
                -negative_limit + 1,
                -56,
                -55,
                -54,
                -109,
                -108,
                -107,
                -82,
                -81,
                -80,
                125 + shift,
                126 + shift,
                127 + shift,
                188 + shift,
                189 + shift,
                190 + shift,
            ] {
                let center = reference_to_rational(&Rational::dyadic(m, exponent), false, binary);
                if center == 0 || center >= binary.infinity() {
                    continue;
                }
                for magnitude in center - 1..=center + 1 {
                    for neg in [false, true] {
                        for format in FORMATS {
                            check_from(
                                magnitude | if neg { binary.sign() } else { 0 },
                                binary,
                                format,
                            );
                        }
                    }
                }
            }
        }
    }
}
