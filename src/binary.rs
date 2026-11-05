#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

//! Manual binary floating-point conversion. See the exact expansion and
//! single-rounding arguments in docs/VERIFICATION.md.

use crate::layout::{InRange, Unpacked, Width};
use crate::operand::sealed::Class;
use crate::tables::{BINARY_POW5, F32_POW10, F64_POW10, pow10, pow10_64};

pub(crate) enum Converted<W: Width> {
    Finite(InRange<W>),
    Infinite(bool),
    Nan { neg: bool, payload: u128 },
}

struct Text {
    bytes: [u8; 48],
    start: usize,
}

impl Text {
    #[expect(
        clippy::arithmetic_side_effects,
        clippy::indexing_slicing,
        reason = "scientific prepends at most 41 bytes into 48 bytes; start is positive before each write"
    )]
    fn prepend(&mut self, byte: u8) {
        self.start -= 1;
        self.bytes[self.start] = byte;
    }

    #[expect(
        clippy::arithmetic_side_effects,
        reason = "division decreases the integer; each remainder is below 10 and width is decremented only when positive"
    )]
    fn integer(&mut self, mut value: u64, mut width: u32) {
        loop {
            self.prepend(b'0' + (value % 10) as u8);
            value /= 10;
            width = width.saturating_sub(1);
            if value == 0 && width == 0 {
                break;
            }
        }
    }

    #[expect(
        clippy::expect_used,
        reason = "start is within the 48-byte buffer and all written bytes are ASCII digits, signs or e"
    )]
    fn as_str(&self) -> &str {
        core::str::from_utf8(self.bytes.get(self.start..).expect("bounded text"))
            .expect("ASCII scientific notation")
    }
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "the divisor is the nonzero constant 10^19; splitting a <=34-digit coefficient leaves a <=15-digit high part and a 19-digit low part"
)]
fn scientific(value: Unpacked) -> Text {
    let mut text = Text {
        bytes: [0; 48],
        start: 48,
    };
    text.integer(u64::from(value.exp.unsigned_abs()), 1);
    if value.exp < 0 {
        text.prepend(b'-');
    }
    text.prepend(b'e');
    if value.coeff <= u128::from(u64::MAX) {
        text.integer(value.coeff as u64, 1);
    } else {
        let divisor = u128::from(pow10_64(19));
        text.integer((value.coeff % divisor) as u64, 19);
        text.integer((value.coeff / divisor) as u64, 1);
    }
    if value.neg {
        text.prepend(b'-');
    }
    text
}

#[expect(
    clippy::expect_used,
    reason = "scientific writes a nonempty valid finite decimal literal; core parsing returns a rounded value, including infinity or zero on range overflow or underflow"
)]
pub(crate) fn to_f64(value: Class) -> f64 {
    match value {
        Class::Finite(value) => {
            if cfg!(not(all(target_arch = "x86", not(target_feature = "sse2"))))
                && value.coeff <= 1_u128 << 53
                && value.exp.unsigned_abs() <= 22
            {
                let power = *F64_POW10
                    .get(value.exp.unsigned_abs() as usize)
                    .expect("Clinger exponent within table");
                let coefficient = value.coeff as u64 as f64;
                let magnitude = if value.exp < 0 {
                    coefficient / power
                } else {
                    coefficient * power
                };
                if value.neg { -magnitude } else { magnitude }
            } else {
                scientific(value)
                    .as_str()
                    .parse()
                    .expect("valid exact decimal")
            }
        }
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
        Class::Finite(value) => {
            if cfg!(not(all(target_arch = "x86", not(target_feature = "sse2"))))
                && value.coeff <= 1_u128 << 24
                && value.exp.unsigned_abs() <= 10
            {
                let power = *F32_POW10
                    .get(value.exp.unsigned_abs() as usize)
                    .expect("Clinger exponent within table");
                let coefficient = value.coeff as u32 as f32;
                let magnitude = if value.exp < 0 {
                    coefficient / power
                } else {
                    coefficient * power
                };
                if value.neg { -magnitude } else { magnitude }
            } else if value.coeff != 0
                && value.coeff <= u128::from(u64::MAX)
                && value.exp < 0
                && value.exp >= -17
            {
                rational_f32(value)
            } else {
                scientific(value)
                    .as_str()
                    .parse()
                    .expect("valid exact decimal")
            }
        }
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

#[expect(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    reason = "0<C<2^64 and 1<=k<=17; D=5^k<2^40. Normalization compares <=64-bit integers in u128; scaling to 24 bits gives N<2^64 and D<2^41. Quotient is 24 bits, carry 25 bits, and -57<=b-k<=63 is normal f32"
)]
fn rational_f32(value: Unpacked) -> f32 {
    let numerator = value.coeff;
    let k = value.exp.unsigned_abs();
    let denominator = BINARY_POW5[k as usize];
    let mut exponent = numerator.bit_width() as i32 - denominator.bit_width() as i32;
    let below = if exponent >= 0 {
        numerator < denominator << exponent
    } else {
        numerator << -exponent < denominator
    };
    exponent -= i32::from(below);
    let shift = 23 - exponent;
    let (numerator, denominator) = if shift >= 0 {
        (numerator << shift, denominator)
    } else {
        (numerator, denominator << -shift)
    };
    let numerator = numerator as u64;
    let denominator = denominator as u64;
    let mut coefficient = (numerator / denominator) as u32;
    let remainder = numerator % denominator;
    coefficient += u32::from(
        remainder * 2 > denominator || (remainder * 2 == denominator && coefficient & 1 != 0),
    );
    if coefficient == 1 << 24 {
        coefficient >>= 1;
        exponent += 1;
    }
    let biased = (exponent - k as i32 + 127) as u32;
    f32::from_bits((u32::from(value.neg) << 31) | (biased << 23) | (coefficient - (1 << 23)))
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
        reason = "factor<=2^63 and carry<factor give limb*factor+carry<10^9*2^63<2^93; the exact <=767-digit expansion fits 86 limbs, including every intermediate carry append"
    )]
    fn multiply(&mut self, factor: u64) {
        let mut carry = 0_u128;
        for limb in self.limbs.iter_mut().take(self.used) {
            let product = u128::from(*limb) * u128::from(factor) + carry;
            *limb = (product % 1_000_000_000) as u32;
            carry = product / 1_000_000_000;
        }
        while carry != 0 {
            *self
                .limbs
                .get_mut(self.used)
                .expect("774-digit exact integer capacity") = (carry % 1_000_000_000) as u32;
            self.used += 1;
            carry /= 1_000_000_000;
        }
    }

    #[expect(
        clippy::arithmetic_side_effects,
        clippy::indexing_slicing,
        reason = "used is 1..=86; drop<=1074, keep<=15; at most 23 high digits are accumulated before the partial-limb division, so all arithmetic fits u128; a guard index exists only for 0<drop<=len"
    )]
    fn round<W: Width>(&self, quantum: i32, neg: bool, precision: u32) -> Converted<W> {
        let len = (self.used as u32 - 1) * 9
            + crate::kernel::digits_64(u64::from(self.limbs[self.used - 1]));
        let drop = len
            .saturating_sub(precision)
            .max((W::LAYOUT.qmin - quantum).max(0) as u32);
        let whole = (drop / 9) as usize;
        let part = drop % 9;
        let mut coefficient = 0_u128;
        for &limb in self.limbs.iter().take(self.used).skip(whole).rev() {
            coefficient = coefficient * 1_000_000_000 + u128::from(limb);
        }
        let divisor = pow10(part);
        let remainder = coefficient % divisor;
        coefficient /= divisor;
        let increment = if drop == 0 || drop > len {
            false
        } else {
            let (remainder, half, tail) = if part == 0 {
                (u128::from(self.limbs[whole - 1]), 500_000_000, whole - 1)
            } else {
                (remainder, divisor / 2, whole)
            };
            remainder > half
                || (remainder == half
                    && (coefficient & 1 != 0
                        || self.limbs.iter().take(tail).any(|&limb| limb != 0)))
        };
        finish::<W>(
            coefficient + u128::from(increment),
            quantum + drop as i32,
            neg,
            precision,
        )
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
    clippy::indexing_slicing,
    reason = "binary fields give m<2^53 and -1074<=e<=971; cancellation shifts <=52; u128 operations require bit-width certificates; table indices are <56 or <27; chunk counts decrease, shifts are <=63"
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
    let quantum = exponent.min(0);
    let precision = W::LAYOUT.digits.min(source_digits);
    if exponent >= 0 && significand.bit_width() + exponent as u32 <= 128 {
        return rounded_integer::<W>(u128::from(significand) << exponent, quantum, neg, precision);
    }
    if exponent < 0 && exponent.unsigned_abs() < BINARY_POW5.len() as u32 {
        let power = BINARY_POW5[exponent.unsigned_abs() as usize];
        if significand.bit_width() + power.bit_width() <= 128 {
            return rounded_integer::<W>(u128::from(significand) * power, quantum, neg, precision);
        }
    }
    let mut integer = DecimalInteger::new(significand);
    if exponent < 0 {
        let mut remaining = exponent.unsigned_abs();
        while remaining >= 27 {
            integer.multiply(BINARY_POW5[27] as u64);
            remaining -= 27;
        }
        if remaining != 0 {
            integer.multiply(BINARY_POW5[remaining as usize] as u64);
        }
    } else {
        let mut remaining = exponent as u32;
        while remaining >= 63 {
            integer.multiply(1 << 63);
            remaining -= 63;
        }
        if remaining != 0 {
            integer.multiply(1 << remaining);
        }
    }
    integer.round::<W>(quantum, neg, precision)
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "integer is nonzero and has <=39 digits; drop<=1074, but the divisor is read only for drop<=len; drop=39 is below half since integer<2^128<5*10^38; other divisors fit u128 and quotient<=10^15"
)]
fn rounded_integer<W: Width>(
    integer: u128,
    quantum: i32,
    neg: bool,
    precision: u32,
) -> Converted<W> {
    let len = crate::kernel::digits(integer);
    let drop = len
        .saturating_sub(precision)
        .max((W::LAYOUT.qmin - quantum).max(0) as u32);
    let coefficient = if drop == 0 {
        integer
    } else if drop > len || drop >= 39 {
        0
    } else {
        let divisor = pow10(drop);
        let coefficient = integer / divisor;
        let remainder = integer % divisor;
        coefficient
            + u128::from(
                remainder > divisor / 2 || (remainder == divisor / 2 && coefficient & 1 != 0),
            )
    };
    finish::<W>(coefficient, quantum + drop as i32, neg, precision)
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "coefficient<=10^precision for precision in 6..=15; exponent>=qmin and <=1074 before carry. Sealed widths have P=7/16/34; padding<P, table powers<=10^34 and multiplication is checked"
)]
fn finish<W: Width>(
    mut coefficient: u128,
    mut exponent: i32,
    neg: bool,
    precision: u32,
) -> Converted<W> {
    if coefficient == pow10(precision) {
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
        match coefficient.checked_mul(pow10(padding)) {
            Some(padded) if padded < pow10(W::LAYOUT.digits) => {
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
