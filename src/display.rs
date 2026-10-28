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
const MAX_LEN: usize = 6179;

struct Buffer<'a> {
    remaining: &'a mut [u8],
}

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

#[expect(
    clippy::arithmetic_side_effects,
    reason = "remaining length starts at 6179 and only decreases by checked splits, so used length is 0..=6179. Finite output needs at most 6178 unsigned ASCII bytes: 0. plus 6175 zeros plus one digit; integer output <= 34+6111 bytes"
)]
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
    let mut bytes = [0_u8; MAX_LEN];
    let mut buffer = Buffer {
        remaining: &mut bytes,
    };
    magnitude(&mut buffer, v, strip)?;
    let used = MAX_LEN - buffer.remaining.len();
    let text =
        core::str::from_utf8(bytes.get(..used).ok_or(fmt::Error)?).map_err(|_| fmt::Error)?;
    f.pad_integral(!v.neg, "", text)
}

/// `NaN` or `inf` with `sign`, padded as `f64` pads them: right-aligned by
/// default, and with the sign written before zero padding.
#[expect(
    clippy::arithmetic_side_effects,
    reason = "pad = width - len only when width > len; pre <= pad"
)]
pub(crate) fn fmt_special(sign: &str, body: &str, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let len = sign.len() + body.len();
    let Some(width) = f.width().filter(|&w| w > len) else {
        f.write_str(sign)?;
        return f.write_str(body);
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
    f.write_str(body)?;
    for _ in pre..pad {
        f.write_char(fill)?;
    }
    Ok(())
}
