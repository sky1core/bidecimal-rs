#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use core::fmt::{self, Write};

use crate::layout::Unpacked;

const ZEROS: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// Longest text, sign included, that serialization writes into a stack
/// buffer in one piece; longer text is streamed.
#[cfg(feature = "serde")]
pub(crate) const TEXT_LEN: usize = 112;

/// Writes into a fixed byte buffer, failing once it is full.
#[cfg(feature = "serde")]
struct Buffer<'a> {
    remaining: &'a mut [u8],
}

#[cfg(feature = "serde")]
impl Write for Buffer<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let (dest, rest) = core::mem::take(&mut self.remaining)
            .split_at_mut_checked(s.len())
            .ok_or(fmt::Error)?;
        dest.copy_from_slice(s.as_bytes());
        self.remaining = rest;
        Ok(())
    }
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "each block length is min(count,64), so subtraction cannot underflow"
)]
fn zeros(out: &mut impl Write, mut count: usize) -> fmt::Result {
    while count != 0 {
        let len = count.min(ZEROS.len());
        out.write_str(ZEROS.get(..len).ok_or(fmt::Error)?)?;
        count -= len;
    }
    Ok(())
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "decoded coeff < 10^34 gives 1..=34 ASCII digits, exp is -6176..=6111. Stripping removes at most min(digit length,-exp); adjusted exp stays in -6176..=6111. Point offset is guarded by n>fraction; otherwise fraction>=n. Positive zero writes only one digit"
)]
fn magnitude(out: &mut impl Write, v: Unpacked, strip: bool) -> fmt::Result {
    let mut buffer = fmt::NumBuffer::new();
    let mut digits = v.coeff.format_into(&mut buffer);
    let mut exp = v.exp;
    if strip && exp < 0 {
        if v.coeff == 0 {
            return out.write_str("0");
        }
        let trailing = digits.bytes().rev().take_while(|&b| b == b'0').count();
        let remove = trailing.min((-exp) as usize);
        digits = digits.get(..digits.len() - remove).ok_or(fmt::Error)?;
        exp += remove as i32;
    }
    if exp >= 0 {
        out.write_str(digits)?;
        if v.coeff != 0 {
            zeros(out, exp as usize)?;
        }
    } else {
        let fraction = (-exp) as usize;
        if digits.len() > fraction {
            let (integer, fraction) = digits
                .split_at_checked(digits.len() - fraction)
                .ok_or(fmt::Error)?;
            out.write_str(integer)?;
            out.write_char('.')?;
            out.write_str(fraction)?;
        } else {
            out.write_str("0.")?;
            zeros(out, fraction - digits.len())?;
            out.write_str(digits)?;
        }
    }
    Ok(())
}

/// Space for the decimal digits of one coefficient.
pub(crate) struct Digits {
    narrow: fmt::NumBuffer<u64>,
    wide: fmt::NumBuffer<u128>,
}

impl Digits {
    pub(crate) const fn new() -> Self {
        Self {
            narrow: fmt::NumBuffer::new(),
            wide: fmt::NumBuffer::new(),
        }
    }

    /// The pieces of the text of `v`, with the digits written once.
    /// Decimal32 and Decimal64 coefficients, and Decimal128 ones below 2^64,
    /// are formatted as `u64`, which avoids 128-bit division.
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "exp is -6176..=6111 and is incremented only while negative"
    )]
    pub(crate) fn parts(&mut self, v: Unpacked, strip: bool) -> Parts<'_> {
        let mut digits = match u64::try_from(v.coeff) {
            Ok(coeff) => coeff.format_into(&mut self.narrow),
            Err(_) => v.coeff.format_into(&mut self.wide),
        };
        let mut exp = v.exp;
        if v.coeff == 0 {
            // Zero has no trailing zeros, and stripping removes its point.
            if exp > 0 || strip {
                exp = 0;
            }
        } else if strip {
            // A nonzero coefficient has a nonzero digit, so the digits never
            // run out.
            while exp < 0 {
                let Some(rest) = digits.strip_suffix('0') else {
                    break;
                };
                digits = rest;
                exp += 1;
            }
        }
        Parts {
            neg: v.neg,
            digits,
            exp,
        }
    }
}

/// The text of a finite value: the sign, then `digits` followed by `exp`
/// zeros if `exp >= 0`, or `digits` with a point `-exp` places from the
/// right, padded with zeros after `0.` if there are fewer digits.
#[derive(Clone, Copy)]
pub(crate) struct Parts<'a> {
    neg: bool,
    digits: &'a str,
    exp: i32,
}

impl Parts<'_> {
    /// Length of the text that [`write_magnitude`](Self::write_magnitude)
    /// writes.
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "1..=34 digits and |exp| <= 6176 keep every length below 6200"
    )]
    fn magnitude_len(&self) -> usize {
        let n = self.digits.len();
        let fraction = self.exp.unsigned_abs() as usize;
        if self.exp >= 0 {
            n + fraction
        } else if n > fraction {
            n + 1
        } else {
            fraction + 2
        }
    }

    /// Writes the text without its sign.
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "the point offset n - fraction is taken only when n > fraction, and fraction - n only otherwise"
    )]
    #[inline(always)]
    fn write_magnitude(&self, out: &mut impl Write) -> fmt::Result {
        let n = self.digits.len();
        let fraction = self.exp.unsigned_abs() as usize;
        if self.exp >= 0 {
            out.write_str(self.digits)?;
            zeros(out, fraction)
        } else if n > fraction {
            let (integer, fraction) = self
                .digits
                .split_at_checked(n - fraction)
                .ok_or(fmt::Error)?;
            out.write_str(integer)?;
            out.write_char('.')?;
            out.write_str(fraction)
        } else {
            out.write_str("0.")?;
            zeros(out, fraction - n)?;
            out.write_str(self.digits)
        }
    }

    /// Length of the whole text, sign included.
    #[cfg(feature = "serde")]
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "magnitude_len is below 6200, so adding the sign cannot overflow"
    )]
    pub(crate) fn text_len(&self) -> usize {
        usize::from(self.neg) + self.magnitude_len()
    }

    /// The whole text if it is the digits alone: a non-negative value with
    /// no point and no trailing zeros.
    #[cfg(feature = "serde")]
    pub(crate) fn digits_only(&self) -> Option<&str> {
        (!self.neg && self.exp == 0).then_some(self.digits)
    }

    /// Writes the whole text, sign included.
    #[cfg(feature = "serde")]
    fn write_text(&self, out: &mut impl Write) -> fmt::Result {
        if self.neg {
            out.write_char('-')?;
        }
        self.write_magnitude(out)
    }

    /// Writes the whole text into `bytes` and returns it, or an error if it
    /// is longer than [`TEXT_LEN`].
    #[cfg(feature = "serde")]
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "the unused length never exceeds TEXT_LEN"
    )]
    pub(crate) fn compose<'b>(&self, bytes: &'b mut [u8; TEXT_LEN]) -> Result<&'b str, fmt::Error> {
        let mut buffer = Buffer {
            remaining: bytes.as_mut_slice(),
        };
        self.write_text(&mut buffer)?;
        let used = TEXT_LEN - buffer.remaining.len();
        core::str::from_utf8(bytes.get(..used).ok_or(fmt::Error)?).map_err(|_| fmt::Error)
    }
}

/// Writes the whole text without flags; used where serialization streams
/// text longer than [`TEXT_LEN`].
#[cfg(feature = "serde")]
impl fmt::Display for Parts<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_text(f)
    }
}

pub(crate) fn fmt(v: Unpacked, strip: bool, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    if f.width().is_none()
        && f.align().is_none()
        && f.fill() == ' '
        && !f.sign_plus()
        && !f.sign_minus()
        && !f.sign_aware_zero_pad()
    {
        if v.neg {
            f.write_char('-')?;
        }
        return magnitude(f, v, strip);
    }
    let mut digits = Digits::new();
    let parts = digits.parts(v, strip);
    let sign = if parts.neg {
        "-"
    } else if f.sign_plus() {
        "+"
    } else {
        ""
    };
    pad(sign, parts.magnitude_len(), f, |f| parts.write_magnitude(f))
}

/// `NaN` or `inf` with `sign`, padded as `f64` pads them: right-aligned by
/// default, and with the sign written before zero padding.
pub(crate) fn fmt_special(sign: &str, body: &str, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    pad(sign, body.len(), f, |f| f.write_str(body))
}

/// Writes `sign` and an ASCII body of `len` bytes that `body` writes, padded
/// as integers and floats are padded: right-aligned by default, and with the
/// sign written before zero padding.
#[expect(
    clippy::arithmetic_side_effects,
    reason = "pad = width - len only when width > len; pre <= pad"
)]
fn pad(
    sign: &str,
    len: usize,
    f: &mut fmt::Formatter<'_>,
    body: impl FnOnce(&mut fmt::Formatter<'_>) -> fmt::Result,
) -> fmt::Result {
    let len = sign.len() + len;
    let Some(width) = f.width().filter(|&w| w > len) else {
        f.write_str(sign)?;
        return body(f);
    };
    let pad = width - len;
    let zero_pad = f.sign_aware_zero_pad();
    let (fill, align) = if zero_pad {
        ('0', fmt::Alignment::Right)
    } else {
        (f.fill(), f.align().unwrap_or(fmt::Alignment::Right))
    };
    let pre = match align {
        fmt::Alignment::Left => 0,
        fmt::Alignment::Right => pad,
        fmt::Alignment::Center => pad / 2,
    };
    if zero_pad {
        f.write_str(sign)?;
    }
    for _ in 0..pre {
        f.write_char(fill)?;
    }
    if !zero_pad {
        f.write_str(sign)?;
    }
    body(f)?;
    for _ in pre..pad {
        f.write_char(fill)?;
    }
    Ok(())
}
