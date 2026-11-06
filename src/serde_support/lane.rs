#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use core::fmt;

/// Eligible narrow serde text, classified before any formatting buffer exists.
pub(crate) enum SmallValue {
    Unsigned(u64),
    Signed(i64),
    NegativeZero,
    Fraction { neg: bool, coeff: u64, exp: i32 },
}

impl SmallValue {
    #[inline]
    pub(crate) fn new(neg: bool, coeff: u64, exp: i32) -> Option<Self> {
        if exp < 0 {
            // A u64 has at most 20 digits. An interior point needs at most
            // 22 bytes; a leading point needs sign + 2 + abs(exp) bytes.
            if exp.unsigned_abs() > if neg { 45 } else { 46 } {
                return None;
            }
            return Some(Self::Fraction { neg, coeff, exp });
        }
        if coeff == 0 {
            return Some(if neg {
                Self::NegativeZero
            } else {
                Self::Unsigned(0)
            });
        }
        let integer = if exp == 0 {
            coeff
        } else {
            coeff.checked_mul(10u64.checked_pow(u32::try_from(exp).ok()?)?)?
        };
        if !neg {
            return Some(Self::Unsigned(integer));
        }
        // The negative endpoint has no positive i64 representation.
        let signed = if integer == i64::MIN.unsigned_abs() {
            i64::MIN
        } else {
            i64::try_from(integer).ok()?.checked_neg()?
        };
        Some(Self::Signed(signed))
    }
}

// Static finishing policy. Assembly remains in one generic helper body;
// the text specialization retains checked UTF-8 inside that helper.
trait FractionOutput {
    type Output: ?Sized;
    fn finish(bytes: &[u8]) -> Option<&Self::Output>;
}

#[cfg(feature = "serde")]
struct TextOutput;
#[cfg(feature = "serde")]
impl FractionOutput for TextOutput {
    type Output = str;
    #[inline(always)]
    fn finish(bytes: &[u8]) -> Option<&str> {
        core::str::from_utf8(bytes).ok()
    }
}

#[cfg(any(
    feature = "sqlx-0_9-mysql",
    feature = "diesel-2-mysql",
    feature = "mysql_common-0_37",
    all(
        test,
        any(
            feature = "sqlx-0_9-postgres",
            feature = "diesel-2-postgres",
            feature = "postgres-types-0_2"
        )
    )
))]
struct BytesOutput;
#[cfg(any(
    feature = "sqlx-0_9-mysql",
    feature = "diesel-2-mysql",
    feature = "mysql_common-0_37",
    all(
        test,
        any(
            feature = "sqlx-0_9-postgres",
            feature = "diesel-2-postgres",
            feature = "postgres-types-0_2"
        )
    )
))]
impl FractionOutput for BytesOutput {
    type Output = [u8];
    #[inline(always)]
    fn finish(bytes: &[u8]) -> Option<&[u8]> {
        Some(bytes)
    }
}

/// Per-call storage for the SWAR fractional lane.
/// The prezeroed fraction buffer is never shared across calls.
pub(crate) struct SmallText {
    bytes: [u8; 48],
}

impl SmallText {
    pub(crate) const fn new() -> Self {
        Self { bytes: [b'0'; 48] }
    }

    /// Once classified, assembly failure is an error, never a fallback miss.
    #[cfg(feature = "serde")]
    #[inline]
    pub(crate) fn text(&mut self, neg: bool, coeff: u64, exp: i32) -> Result<&str, fmt::Error> {
        self.fraction::<TextOutput>(neg, coeff, exp)
            .ok_or(fmt::Error)
    }

    /// Same checked assembly, finished as bytes for byte-only MySQL sinks.
    #[cfg(any(
        feature = "sqlx-0_9-mysql",
        feature = "diesel-2-mysql",
        feature = "mysql_common-0_37",
        all(
            test,
            any(
                feature = "sqlx-0_9-postgres",
                feature = "diesel-2-postgres",
                feature = "postgres-types-0_2"
            )
        )
    ))]
    #[inline]
    pub(crate) fn bytes(&mut self, neg: bool, coeff: u64, exp: i32) -> Result<&[u8], fmt::Error> {
        self.fraction::<BytesOutput>(neg, coeff, exp)
            .ok_or(fmt::Error)
    }

    #[inline]
    fn fraction<F: FractionOutput>(
        &mut self,
        neg: bool,
        coeff: u64,
        exp: i32,
    ) -> Option<&F::Output> {
        let n = coeff
            .checked_ilog10()
            .map_or(Some(1), |n| usize::try_from(n).ok()?.checked_add(1))?;
        let fraction = usize::try_from(exp.unsigned_abs()).ok()?;
        let sign = usize::from(neg);
        let len = if n > fraction {
            sign.checked_add(n)?.checked_add(1)?
        } else {
            sign.checked_add(2)?.checked_add(fraction)?
        };
        let start = 48usize.checked_sub(len)?;
        small_blocks(coeff, n, &mut self.bytes)?;
        if n > fraction {
            // n <= 20 and fraction >= 1 give at most 19 integer digits and point >= 28.
            // The fixed 24-byte length avoids a memmove call; a variable length was slower.
            let point = 47usize.checked_sub(fraction)?;
            self.bytes
                .get_mut(point.checked_sub(24)?..point.checked_add(1)?)?
                .copy_within(1..25, 0);
            *self.bytes.get_mut(point)? = b'.';
        } else {
            let zero = start.checked_add(sign)?;
            *self.bytes.get_mut(zero)? = b'0';
            *self.bytes.get_mut(zero.checked_add(1)?)? = b'.';
        }
        if neg {
            *self.bytes.get_mut(start)? = b'-';
        }
        F::finish(self.bytes.get(start..)?)
    }
}

/// Eight decimal digits, x < 10^8. Lane arithmetic deliberately wraps:
/// masked high-lane carry is discarded, not an integer overflow error.
#[inline]
fn small_eight(x: u32) -> [u8; 8] {
    let hi = u64::from(x / 10_000);
    let lo = u64::from(x % 10_000);
    let lanes = hi | (lo << 32);
    let q = (lanes.wrapping_mul(10_486) >> 20) & 0x0000_007f_0000_007f;
    let lanes = (lanes.wrapping_sub(q.wrapping_mul(100)) << 16) | q;
    let q = (lanes.wrapping_mul(103) >> 10) & 0x000f_000f_000f_000f;
    let lanes = (lanes.wrapping_sub(q.wrapping_mul(10)) << 8) | q;
    (lanes | 0x3030_3030_3030_3030).to_le_bytes()
}

/// Right-aligned 8-digit blocks; leading spill only overwrites prezeroed slack.
#[inline]
fn small_blocks(coeff: u64, n: usize, bytes: &mut [u8; 48]) -> Option<()> {
    if n <= 8 {
        bytes
            .get_mut(40..48)?
            .copy_from_slice(&small_eight(u32::try_from(coeff).ok()?));
    } else if n <= 16 {
        bytes
            .get_mut(32..40)?
            .copy_from_slice(&small_eight(u32::try_from(coeff / 100_000_000).ok()?));
        bytes
            .get_mut(40..48)?
            .copy_from_slice(&small_eight(u32::try_from(coeff % 100_000_000).ok()?));
    } else {
        let top = coeff / 10_000_000_000_000_000;
        let rest = coeff % 10_000_000_000_000_000;
        bytes
            .get_mut(24..32)?
            .copy_from_slice(&small_eight(u32::try_from(top).ok()?));
        bytes
            .get_mut(32..40)?
            .copy_from_slice(&small_eight(u32::try_from(rest / 100_000_000).ok()?));
        bytes
            .get_mut(40..48)?
            .copy_from_slice(&small_eight(u32::try_from(rest % 100_000_000).ok()?));
    }
    Some(())
}

#[cfg(all(test, feature = "serde"))]
#[allow(
    clippy::arithmetic_side_effects,
    clippy::unwrap_used,
    clippy::expect_used
)]
mod tests {
    use super::{SmallText, SmallValue, small_eight};
    use crate::Decimal128;
    use serde_test::{Token, assert_ser_tokens};
    use std::{format, string::ToString};
    #[test]
    fn serde_small_lane_direct_lengths_and_exponents() {
        for len in [16usize, 17, 32, 33, 48, 49] {
            for neg in [false, true] {
                let fraction = len - 2 - usize::from(neg);
                let exp = -(fraction as i32);
                let selected = SmallValue::new(neg, 1, exp);
                assert_eq!(selected.is_some(), len <= 48, "len={len} neg={neg}");
                if len <= 48 {
                    let expected = format!(
                        "{}0.{}1",
                        if neg { "-" } else { "" },
                        "0".repeat(fraction - 1)
                    );
                    let mut text = SmallText::new();
                    assert_eq!(text.text(neg, 1, exp).unwrap(), expected);
                }
            }
        }
        for exp in [-45i32, -46, -47] {
            for neg in [false, true] {
                for coeff in [0u64, 1, u64::MAX] {
                    let selected = SmallValue::new(neg, coeff, exp);
                    let fits = exp.unsigned_abs() <= if neg { 45 } else { 46 };
                    assert_eq!(selected.is_some(), fits, "{neg} {coeff} {exp}");
                    if fits {
                        let digits = coeff.to_string();
                        let expected = format!(
                            "{}0.{}{}",
                            if neg { "-" } else { "" },
                            "0".repeat(exp.unsigned_abs() as usize - digits.len()),
                            digits
                        );
                        let mut text = SmallText::new();
                        assert_eq!(text.text(neg, coeff, exp).unwrap(), expected);
                    }
                }
            }
        }
    }

    #[test]
    fn serde_small_lane_direct_integer_and_point_boundaries() {
        for (neg, coeff, exp, expected) in [
            (false, 0, 6111, "0"),
            (true, 0, 6111, "-0"),
            (false, u64::MAX, 0, "18446744073709551615"),
            (true, i64::MAX as u64, 0, "-9223372036854775807"),
            (true, i64::MIN.unsigned_abs(), 0, "-9223372036854775808"),
            (false, 1844674407370955161, 1, "18446744073709551610"),
            (true, 922337203685477580, 1, "-9223372036854775800"),
            (false, u64::MAX, -1, "1844674407370955161.5"),
            (true, u64::MAX, -1, "-1844674407370955161.5"),
            (true, 0, -1, "-0.0"),
        ] {
            assert!(SmallValue::new(neg, coeff, exp).is_some(), "eligible lane");
            let magnitude = i128::from(coeff);
            let value = Decimal128::from_coefficient_and_exponent(
                if neg { -magnitude } else { magnitude },
                exp,
            )
            .unwrap();
            let value = if neg && coeff == 0 { -value } else { value };
            assert_ser_tokens(&value, &[Token::Str(expected)]);
        }
        for (neg, coeff, exp) in [
            (false, u64::MAX, 1),
            (true, u64::MAX, 0),
            (true, i64::MIN.unsigned_abs() + 1, 0),
            (false, 1, 20),
            (false, 1, 6111),
        ] {
            assert!(SmallValue::new(neg, coeff, exp).is_none());
        }
        // Even a malformed internal plan must report assembly failure, not a miss.
        let mut text = SmallText::new();
        assert!(text.text(false, 1, -47).is_err());
    }

    #[test]
    #[ignore = "100 million SWAR inputs; run explicitly in release mode"]
    fn small_eight_all_100_million_match_format() {
        for x in 0..100_000_000u32 {
            let expected = std::format!("{x:08}");
            assert_eq!(small_eight(x).as_slice(), expected.as_bytes(), "x={x}");
        }
    }
}
