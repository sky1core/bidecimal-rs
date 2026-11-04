#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

//! Manual binary floating-point conversion. See the exact expansion and
//! single-rounding arguments in docs/VERIFICATION.md.

use core::fmt::{self, Write};

use crate::layout::{InRange, Unpacked, Width};
use crate::operand::sealed::Class;

pub(crate) enum Converted<W: Width> {
    Finite(InRange<W>),
    Infinite(bool),
    Nan { neg: bool, payload: u128 },
}

struct Text<const N: usize> {
    bytes: [u8; N],
    len: usize,
}

impl<const N: usize> Text<N> {
    fn new() -> Self {
        Self {
            bytes: [0; N],
            len: 0,
        }
    }

    #[expect(
        clippy::expect_used,
        reason = "the private writer advances len only after a successful bounds check and its callers write only ASCII integer digits, signs and e"
    )]
    fn as_str(&self) -> &str {
        let bytes = self.bytes.get(..self.len).expect("bounded text length");
        core::str::from_utf8(bytes).expect("ASCII integer formatting")
    }
}

impl<const N: usize> Write for Text<N> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let end = self.len.checked_add(text.len()).ok_or(fmt::Error)?;
        self.bytes
            .get_mut(self.len..end)
            .ok_or(fmt::Error)?
            .copy_from_slice(text.as_bytes());
        self.len = end;
        Ok(())
    }
}

#[expect(
    clippy::expect_used,
    reason = "a validated decimal has at most 34 coefficient digits and q in -6176..=6111; sign, coefficient, e and signed exponent use at most 41 ASCII bytes"
)]
fn scientific(value: Unpacked) -> Text<48> {
    let mut text = Text::new();
    write!(
        text,
        "{}{}e{}",
        if value.neg { "-" } else { "" },
        value.coeff,
        value.exp
    )
    .expect("41-byte scientific notation bound");
    text
}

#[expect(
    clippy::expect_used,
    reason = "scientific writes a nonempty valid finite decimal literal; core parsing returns a rounded value, including infinity or zero on range overflow or underflow"
)]
pub(crate) fn to_f64(value: Class) -> f64 {
    match value {
        Class::Finite(value) => scientific(value)
            .as_str()
            .parse()
            .expect("valid exact decimal"),
        Class::Infinite(neg) => f64::from_bits((u64::from(neg) << 63) | 0x7ff0_0000_0000_0000),
        Class::Nan { neg, payload } => {
            let payload = if payload < (1_u128 << 51) {
                payload as u64
            } else {
                0
            };
            f64::from_bits((u64::from(neg) << 63) | 0x7ff8_0000_0000_0000 | payload)
        }
    }
}

#[expect(
    clippy::expect_used,
    reason = "scientific writes valid exact decimal syntax and parse is instantiated directly for f32; range endpoints return rounded infinity or signed zero, not a syntax error"
)]
pub(crate) fn to_f32(value: Class) -> f32 {
    match value {
        Class::Finite(value) => scientific(value)
            .as_str()
            .parse()
            .expect("valid exact decimal"),
        Class::Infinite(neg) => f32::from_bits((u32::from(neg) << 31) | 0x7f80_0000),
        Class::Nan { neg, payload } => {
            let payload = if payload < (1_u128 << 22) {
                payload as u32
            } else {
                0
            };
            f32::from_bits((u32::from(neg) << 31) | 0x7fc0_0000 | payload)
        }
    }
}

// Exact m * 5^k or m * 2^k, not a truncated floating approximation.
// m < 2^53 and k <= 1074 give at most 767 decimal digits. The positive
// binary-exponent case has at most 309 digits. 86 limbs hold 774 digits.
struct DecimalInteger {
    limbs: [u32; 86],
    used: usize,
}

impl DecimalInteger {
    #[expect(
        clippy::arithmetic_side_effects,
        clippy::expect_used,
        reason = "the nonzero binary significand is below 2^53 and needs at most two base-10^9 limbs; division decreases it and used stays below 86"
    )]
    fn new(mut significand: u64) -> Self {
        let mut result = Self {
            limbs: [0; 86],
            used: 0,
        };
        loop {
            *result
                .limbs
                .get_mut(result.used)
                .expect("two-limb significand") = (significand % 1_000_000_000) as u32;
            result.used += 1;
            significand /= 1_000_000_000;
            if significand == 0 {
                return result;
            }
        }
    }

    #[expect(
        clippy::arithmetic_side_effects,
        clippy::expect_used,
        reason = "each factor is at most 2^29, each limb below 10^9, and carry below factor, so product+carry is below 10^9*2^29 < 2^59; all intermediate integers are prefixes of a <=767-digit exact expansion and fit 86 limbs"
    )]
    fn multiply(&mut self, factor: u32) {
        let mut carry = 0_u64;
        for limb in self.limbs.iter_mut().take(self.used) {
            let product = u64::from(*limb) * u64::from(factor) + carry;
            *limb = (product % 1_000_000_000) as u32;
            carry = product / 1_000_000_000;
        }
        if carry != 0 {
            *self
                .limbs
                .get_mut(self.used)
                .expect("774-digit exact integer capacity") = carry as u32;
            self.used += 1;
        }
    }

    #[expect(
        clippy::expect_used,
        reason = "used is in 1..=86 and the nonzero expansion contains at most 767 digits, so all limbs and their ASCII representation fit the 800-byte writer"
    )]
    fn digits(&self) -> Text<800> {
        let mut text = Text::new();
        let mut limbs = self
            .limbs
            .get(..self.used)
            .expect("bounded limb count")
            .iter()
            .rev();
        let first = limbs.next().expect("nonzero exact integer");
        write!(text, "{first}").expect("767-digit exact expansion bound");
        for limb in limbs {
            write!(text, "{limb:09}").expect("767-digit exact expansion bound");
        }
        text
    }
}

pub(crate) fn from_f64<W: Width>(value: f64) -> Converted<W> {
    from_bits::<W, false>(value.to_bits())
}

pub(crate) fn from_f32<W: Width>(value: f32) -> Converted<W> {
    from_bits::<W, true>(u64::from(value.to_bits()))
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "binary32/64 fields have at most 52 fraction bits, exponent e in -1074..=971 and nonzero significand <2^53; cancelled shifts are <=52; chunk loops decrease nonnegative counts and use 5^r for r<12 or 2^r for r<29"
)]
fn from_bits<W: Width, const SINGLE: bool>(bits: u64) -> Converted<W> {
    let (fraction_bits, bias, exponent_mask, sign_bit, source_digits) = if SINGLE {
        (23, 127, 255, 31, f32::DIGITS)
    } else {
        (52, 1023, 2047, 63, f64::DIGITS)
    };
    let neg = bits >> sign_bit != 0;
    let fraction = bits & ((1_u64 << fraction_bits) - 1);
    let raw_exponent = (bits >> fraction_bits) & exponent_mask;
    if raw_exponent == exponent_mask {
        return if fraction == 0 {
            Converted::Infinite(neg)
        } else {
            Converted::Nan {
                neg,
                payload: u128::from(fraction & ((1_u64 << (fraction_bits - 1)) - 1)),
            }
        };
    }
    if raw_exponent == 0 && fraction == 0 {
        return Converted::Finite(InRange::unchecked(Unpacked {
            neg,
            coeff: 0,
            exp: 0,
        }));
    }

    let mut significand = fraction;
    let mut exponent = if raw_exponent == 0 {
        1 - bias - fraction_bits
    } else {
        significand |= 1_u64 << fraction_bits;
        raw_exponent as i32 - bias - fraction_bits
    };
    if exponent < 0 {
        let cancelled = significand.trailing_zeros().min(exponent.unsigned_abs());
        significand >>= cancelled;
        exponent += cancelled as i32;
    }
    let mut integer = DecimalInteger::new(significand);
    let quantum = exponent.min(0);
    if exponent < 0 {
        let mut remaining = exponent.unsigned_abs();
        while remaining >= 12 {
            integer.multiply(244_140_625); // 5^12
            remaining -= 12;
        }
        if remaining != 0 {
            integer.multiply(5_u32.pow(remaining));
        }
    } else {
        let mut remaining = exponent as u32;
        while remaining >= 29 {
            integer.multiply(1 << 29);
            remaining -= 29;
        }
        if remaining != 0 {
            integer.multiply(1 << remaining);
        }
    }
    let digits = integer.digits();
    rounded::<W>(
        digits.as_str().as_bytes(),
        quantum,
        neg,
        W::LAYOUT.digits.min(source_digits),
    )
}

#[expect(
    clippy::arithmetic_side_effects,
    clippy::expect_used,
    reason = "digits is a nonzero ASCII integer of at most 767 digits, q in -1074..=0, precision in 6..=15; keep<=precision, so coefficient and its increment fit u128. drop<=1074 and exponent stays in i32. Width is sealed to P=7/16/34; exact exponent padding uses powers below 10^34 and checked multiplication"
)]
fn rounded<W: Width>(digits: &[u8], quantum: i32, neg: bool, precision: u32) -> Converted<W> {
    // Choose the final decimal unit before rounding: precision reduction and
    // gradual underflow are one quotient/remainder operation, not two rounds.
    let drop = digits
        .len()
        .saturating_sub(precision as usize)
        .max((W::LAYOUT.qmin - quantum).max(0) as usize);
    let keep = digits.len().saturating_sub(drop);
    let mut coefficient = 0_u128;
    for &digit in digits.iter().take(keep) {
        coefficient = coefficient * 10 + u128::from(digit - b'0');
    }
    let increment = if drop == 0 || drop > digits.len() {
        false
    } else {
        let tail = digits
            .get(keep..)
            .expect("rounding tail within exact digits");
        let (&guard, tail) = tail.split_first().expect("at least one discarded digit");
        guard > b'5'
            || (guard == b'5' && (tail.iter().any(|&digit| digit != b'0') || coefficient & 1 != 0))
    };
    coefficient += u128::from(increment);
    let mut exponent = quantum + drop as i32;
    if coefficient == 10_u128.pow(precision) {
        coefficient /= 10;
        exponent += 1;
    }
    if coefficient == 0 {
        return Converted::Finite(InRange::unchecked(Unpacked {
            neg,
            coeff: 0,
            exp: 0,
        }));
    }
    if exponent > W::LAYOUT.qmax {
        let padding = (exponent - W::LAYOUT.qmax) as u32;
        if padding >= W::LAYOUT.digits {
            return Converted::Infinite(neg);
        }
        match coefficient.checked_mul(10_u128.pow(padding)) {
            Some(padded) if padded < 10_u128.pow(W::LAYOUT.digits) => {
                coefficient = padded;
                exponent = W::LAYOUT.qmax;
            }
            _ => return Converted::Infinite(neg),
        }
    }
    // drop chose q>=qmin; carry can only raise it. The branch above enforces
    // q<=qmax and coefficient<10^P. P and precision are positive, so q=0
    // zero and every retained nonzero result satisfy InRange's preconditions.
    Converted::Finite(InRange::unchecked(Unpacked {
        neg,
        coeff: coefficient,
        exp: exponent,
    }))
}
