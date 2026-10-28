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

    fn aligned(&self, other: &Self) -> (BigInt, BigInt, i32) {
        let exp = self.exp.min(other.exp);
        (
            &self.coeff * BigInt::from(10).pow((self.exp - exp) as u32),
            &other.coeff * BigInt::from(10).pow((other.exp - exp) as u32),
            exp,
        )
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
