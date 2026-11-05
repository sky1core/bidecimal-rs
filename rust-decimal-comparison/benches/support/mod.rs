use num_bigint::BigInt;
use std::cmp::Ordering;

#[derive(Clone, Debug)]
pub struct Exact {
    coeff: BigInt,
    exp: i32,
}

impl Exact {
    pub fn new(mut coeff: BigInt, mut exp: i32) -> Self {
        if coeff == BigInt::from(0) {
            return Self { coeff, exp: 0 };
        }
        while &coeff % 10 == BigInt::from(0) {
            coeff /= 10;
            exp += 1;
        }
        Self { coeff, exp }
    }

    pub fn parse(text: &str) -> Self {
        let (mantissa, exp) = match text.split_once(['e', 'E']) {
            Some((m, e)) => (m, e.parse::<i32>().unwrap()),
            None => (text, 0),
        };
        let scale = mantissa.split_once('.').map_or(0, |(_, f)| f.len() as i32);
        Self::new(mantissa.replace('.', "").parse().unwrap(), exp - scale)
    }

    pub fn inside(&self) -> bool {
        self.coeff == BigInt::from(0)
            || (self.exp >= -28
                && self.coeff.to_string().trim_start_matches('-').len() + self.exp.max(0) as usize
                    <= 28)
    }

    fn aligned(&self, other: &Self) -> (BigInt, BigInt, i32) {
        let exp = self.exp.min(other.exp);
        (
            &self.coeff * BigInt::from(10).pow((self.exp - exp) as u32),
            &other.coeff * BigInt::from(10).pow((other.exp - exp) as u32),
            exp,
        )
    }

    pub fn add(&self, other: &Self) -> Self {
        let (a, b, exp) = self.aligned(other);
        Self::new(a + b, exp)
    }

    pub fn sub(&self, other: &Self) -> Self {
        let (a, b, exp) = self.aligned(other);
        Self::new(a - b, exp)
    }

    pub fn mul(&self, other: &Self) -> Self {
        Self::new(&self.coeff * &other.coeff, self.exp + other.exp)
    }

    pub fn rounded_multiple(&self, step: &Self, ceil: bool) -> Self {
        let (a, b, _) = self.aligned(step);
        assert!(b > BigInt::from(0));
        let mut q = &a / &b;
        if &a % &b != BigInt::from(0) {
            if ceil && a > BigInt::from(0) {
                q += 1;
            } else if !ceil && a < BigInt::from(0) {
                q -= 1;
            }
        }
        Self::new(q * &step.coeff, step.exp)
    }

    pub fn numeric_cmp(&self, other: &Self) -> Ordering {
        let (a, b, _) = self.aligned(other);
        a.cmp(&b)
    }
}

impl PartialEq for Exact {
    fn eq(&self, other: &Self) -> bool {
        self.numeric_cmp(other) == Ordering::Equal
    }
}

pub trait Numeric {
    fn exact(&self) -> Option<Exact>;
}
#[derive(Debug, PartialEq)]
pub struct Bid {
    pub negative: bool,
    pub coeff: u128,
    pub exp: i32,
}

pub fn decode_bid(bits: u128, width: u32) -> Option<Bid> {
    let (coeff_bits, exp_bits, bias, digits) = match width {
        32 => (23, 8, 101, 7),
        64 => (53, 10, 398, 16),
        128 => (113, 14, 6176, 34),
        _ => return None,
    };
    if width < 128 && bits >> width != 0 {
        return None;
    }
    let combination = (bits >> (width - 6)) & 31;
    if combination >= 30 {
        return None;
    }
    let steering = combination & 24 == 24;
    let (exp_shift, coeff) = if steering {
        (
            coeff_bits - 2,
            (bits & ((1_u128 << (coeff_bits - 2)) - 1)) | (1_u128 << coeff_bits),
        )
    } else {
        (coeff_bits, bits & ((1_u128 << coeff_bits) - 1))
    };
    if coeff >= 10_u128.pow(digits) {
        return None;
    }
    Some(Bid {
        negative: bits >> (width - 1) != 0,
        coeff,
        exp: ((bits >> exp_shift) & ((1_u128 << exp_bits) - 1)) as i32 - bias,
    })
}

impl Bid {
    pub fn exact(&self) -> Exact {
        Exact::new(
            BigInt::from(self.coeff) * if self.negative { -1 } else { 1 },
            self.exp,
        )
    }
}

macro_rules! bid_numeric {
    ($ty:ty, $width:literal) => {
        impl Numeric for $ty {
            fn exact(&self) -> Option<Exact> {
                decode_bid(self.to_bits() as u128, $width).map(|v| v.exact())
            }
        }
    };
}
bid_numeric!(bidecimal::Decimal32, 32);
bid_numeric!(bidecimal::Decimal64, 64);
bid_numeric!(bidecimal::Decimal128, 128);

impl<T: Numeric, E> Numeric for Result<T, E> {
    fn exact(&self) -> Option<Exact> {
        self.as_ref().ok().and_then(Numeric::exact)
    }
}
impl<T: Numeric> Numeric for Option<T> {
    fn exact(&self) -> Option<Exact> {
        self.as_ref().and_then(Numeric::exact)
    }
}
pub fn numeric_equal(a: &impl Numeric, b: &impl Numeric) -> bool {
    match (a.exact(), b.exact()) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

pub fn preflight<O, P>(
    input: &str,
    ours: &O,
    baseline: impl FnOnce() -> P,
    verified: impl FnOnce(&O) -> bool,
    equal: impl FnOnce(&O, &P) -> bool,
) -> bool {
    assert!(verified(ours), "invalid bidecimal result for {input}");
    equal(ours, &baseline())
}

#[derive(Debug, PartialEq)]
pub enum BinaryDecimal {
    Finite(Bid),
    Infinity { negative: bool },
}

pub fn decimal_binary(text: &str, width: u32) -> u64 {
    let (fraction_bits, exponent_bits, bias) = match width {
        32 => (23, 8, 127),
        64 => (52, 11, 1023),
        _ => panic!("unsupported binary width"),
    };
    let exact = Exact::parse(text);
    let sign = u64::from(text.starts_with('-')) << (width - 1);
    let magnitude = if exact.coeff < BigInt::from(0) {
        -exact.coeff
    } else {
        exact.coeff
    };
    let (numerator, denominator) = if exact.exp >= 0 {
        (
            magnitude * BigInt::from(10).pow(exact.exp as u32),
            BigInt::from(1),
        )
    } else {
        (magnitude, BigInt::from(10).pow(exact.exp.unsigned_abs()))
    };
    let infinity = ((1_u64 << exponent_bits) - 1) << fraction_bits;
    let rational = |bits: u64| {
        let exponent = (bits >> fraction_bits) as i32;
        let fraction = bits & ((1 << fraction_bits) - 1);
        let significand = BigInt::from(if exponent == 0 {
            fraction
        } else {
            fraction | (1 << fraction_bits)
        });
        let power = exponent.max(1) - bias - fraction_bits;
        if power >= 0 {
            (significand << power as usize, BigInt::from(1))
        } else {
            (significand, BigInt::from(1) << (-power) as usize)
        }
    };
    let mut lower = 0;
    let mut upper = infinity;
    while upper - lower > 1 {
        let candidate = lower + (upper - lower) / 2;
        let (n, d) = rational(candidate);
        match (&numerator * d).cmp(&(n * &denominator)) {
            Ordering::Equal => return sign | candidate,
            Ordering::Less => upper = candidate,
            Ordering::Greater => lower = candidate,
        }
    }
    let (ln, ld) = rational(lower);
    let (un, ud) = rational(upper);
    let midpoint_cmp = (&numerator * &ld * &ud * 2_u8).cmp(&((ln * &ud + un * &ld) * &denominator));
    sign | match midpoint_cmp {
        Ordering::Less => lower,
        Ordering::Greater => upper,
        Ordering::Equal => {
            if lower & 1 == 0 {
                lower
            } else {
                upper
            }
        }
    }
}

pub fn binary_decimal(bits: u64, binary_width: u32, width: u32) -> BinaryDecimal {
    let (fraction_bits, exponent_bits, bias, limit) = match binary_width {
        32 => (23, 8, 127, 6),
        64 => (52, 11, 1023, 15),
        _ => panic!("unsupported binary width"),
    };
    let (precision, qmin, qmax) = match width {
        32 => (7, -101, 90),
        64 => (16, -398, 369),
        128 => (34, -6176, 6111),
        _ => panic!("unsupported decimal width"),
    };
    let negative = bits >> (binary_width - 1) != 0;
    let raw_exp = ((bits >> fraction_bits) & ((1 << exponent_bits) - 1)) as i32;
    let fraction = bits & ((1 << fraction_bits) - 1);
    if raw_exp == (1 << exponent_bits) - 1 {
        assert_eq!(fraction, 0, "NaN is not a conversion fixture");
        return BinaryDecimal::Infinity { negative };
    }
    let mut coeff = BigInt::from(if raw_exp == 0 {
        fraction
    } else {
        fraction | (1 << fraction_bits)
    });
    let power = raw_exp.max(1) - bias - fraction_bits;
    let mut exp = 0;
    if power >= 0 {
        coeff <<= power as usize;
    } else {
        coeff *= BigInt::from(5).pow((-power) as u32);
        exp = power;
    }
    while exp < 0 && &coeff % 10 == BigInt::from(0) {
        coeff /= 10;
        exp += 1;
    }
    let limit = precision.min(limit);
    let digits = coeff.to_string().len() as i32;
    let discard = (digits - limit).max(qmin - exp).max(0);
    let divisor = BigInt::from(10).pow(discard as u32);
    let remainder = &coeff % &divisor;
    coeff /= &divisor;
    if &remainder * 2 > divisor || (&remainder * 2 == divisor && &coeff % 2 != BigInt::from(0)) {
        coeff += 1;
    }
    exp += discard;
    if coeff.to_string().len() as i32 > limit {
        assert_eq!(&coeff % 10, BigInt::from(0));
        coeff /= 10;
        exp += 1;
    }
    if exp > qmax {
        if coeff != BigInt::from(0) && coeff.to_string().len() as i32 + exp - qmax > precision {
            return BinaryDecimal::Infinity { negative };
        }
        coeff *= BigInt::from(10).pow((exp - qmax) as u32);
        exp = qmax;
    }
    if coeff == BigInt::from(0) {
        exp = 0;
    } else {
        while exp < 0 && &coeff % 10 == BigInt::from(0) {
            coeff /= 10;
            exp += 1;
        }
    }
    BinaryDecimal::Finite(Bid {
        negative,
        coeff: coeff.to_string().parse().unwrap(),
        exp,
    })
}

pub fn verify_binary_decimal(bits: u128, width: u32, expected: &BinaryDecimal) {
    match expected {
        BinaryDecimal::Finite(value) => assert_eq!(decode_bid(bits, width).as_ref(), Some(value)),
        BinaryDecimal::Infinity { negative } => {
            assert_eq!(
                bits,
                (u128::from(*negative) << (width - 1)) | (30 << (width - 6))
            );
        }
    }
}

pub fn rust_decimal_exact(value: Option<rust_decimal::Decimal>, expected: &BinaryDecimal) -> bool {
    match (value, expected) {
        (Some(value), BinaryDecimal::Finite(expected)) => {
            Exact::new(BigInt::from(value.mantissa()), -(value.scale() as i32)) == expected.exact()
                && (expected.coeff != 0 || value.is_sign_negative() == expected.negative)
        }
        _ => false,
    }
}
