use crate::keyed::Spec;
use crate::support::Bid;
use num_bigint::BigInt;

pub fn signed(spec: Spec) -> BigInt {
    BigInt::from(spec.coeff) * if spec.neg { -1 } else { 1 }
}

/// `n / d` rounded toward negative, for `d > 0`.
pub fn floor_div(n: &BigInt, d: &BigInt) -> BigInt {
    let quotient = n / d;
    if (n % d) < BigInt::ZERO {
        quotient - 1
    } else {
        quotient
    }
}

pub fn pow10(n: i32) -> BigInt {
    BigInt::from(10).pow(n as u32)
}

/// The fields of `coeff × 10^exp` with `digits` significant digits: an exact
/// value that fits keeps `exp`, the preferred exponent; otherwise the value is
/// rounded toward negative at the least exponent that fits. A zero is negative
/// when `zero_neg`.
pub fn floor_fields(coeff: BigInt, exp: i32, digits: u32, zero_neg: bool) -> Bid {
    let negative = coeff < BigInt::ZERO || (coeff == BigInt::ZERO && zero_neg);
    let (mut coeff, mut exp) = (coeff, exp);
    let excess = coeff.magnitude().to_string().len() as i32 - digits as i32;
    if excess > 0 {
        coeff = floor_div(&coeff, &pow10(excess));
        exp += excess;
        if coeff.magnitude() == pow10(digits as i32).magnitude() {
            coeff /= 10;
            exp += 1;
        }
    }
    Bid {
        negative,
        coeff: u128::try_from(coeff.magnitude().clone()).unwrap(),
        exp,
    }
}

/// The fields of `x / y` rounded toward negative to `digits` significant
/// digits, or `None` when the quotient is an exact decimal. The quotient is
/// first floored at 10^-128 below the operands' exponent difference, which
/// keeps more than `digits` digits for coefficients below 10^34, so flooring
/// that again gives the floor of the quotient.
pub fn floor_quotient(x: Spec, y: Spec, digits: u32) -> Option<Bid> {
    let scaled = signed(x) * pow10(128) * if y.neg { -1 } else { 1 };
    let divisor = BigInt::from(y.coeff);
    if &scaled % &divisor == BigInt::ZERO {
        return None;
    }
    Some(floor_fields(
        floor_div(&scaled, &divisor),
        x.exp - y.exp - 128,
        digits,
        false,
    ))
}
