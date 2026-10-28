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
