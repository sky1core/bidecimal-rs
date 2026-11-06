//! PostgreSQL NUMERIC's external binary format (not its on-disk format).
//! Scratch is bounded by the decimal coefficient, never the wire exponent or
//! input length. Every input group is validated before exact fitting.
use super::DbDecimal;
use crate::Error;

pub(crate) struct PgNumeric {
    bytes: [u8; 28],
    start: usize,
}
impl PgNumeric {
    pub(crate) fn new<D: DbDecimal>(value: D) -> Result<Self, Error> {
        let mut out = Self {
            bytes: [0; 28],
            start: 20,
        };
        if let Some(sign) = value.pg_special()? {
            out.bytes[24..26].copy_from_slice(&sign.to_be_bytes());
            return Ok(out);
        }
        let (neg, coefficient, exponent) = value.parts()?;
        let scale = (-i64::from(exponent)).max(0);
        if scale > 16383 {
            return Err(Error::Overflow);
        }
        // PostgreSQL canonicalizes negative zero, while retaining its scale.
        if coefficient == 0 {
            out.bytes[26..28].copy_from_slice(&(scale as u16).to_be_bytes());
            return Ok(out);
        }
        let align = exponent.rem_euclid(4) as u32;
        // At most 34+3 digits, which fits u128 and ten base-10000 groups.
        let mut coefficient = coefficient * 10_u128.pow(align);
        let mut low_weight = i64::from(exponent).div_euclid(4);
        while coefficient.is_multiple_of(10000) {
            coefficient /= 10000;
            low_weight += 1;
        }
        out.start = out.bytes.len();
        while coefficient != 0 {
            let digit = (coefficient % 10000) as u16;
            coefficient /= 10000;
            out.start -= 2;
            out.bytes[out.start..out.start + 2].copy_from_slice(&digit.to_be_bytes());
        }
        let count = (out.bytes.len() - out.start) / 2;
        let weight = i16::try_from(low_weight + count as i64 - 1).map_err(|_| Error::Overflow)?;
        // Ten groups use at most twenty bytes, leaving eight for the header.
        // Place it directly before the occupied suffix, without moving digits.
        out.start -= 8;
        let header = &mut out.bytes[out.start..out.start + 8];
        header[0..2].copy_from_slice(&(count as u16).to_be_bytes());
        header[2..4].copy_from_slice(&weight.to_be_bytes());
        header[4..6].copy_from_slice(&(if neg { 0x4000_u16 } else { 0 }).to_be_bytes());
        header[6..8].copy_from_slice(&(scale as u16).to_be_bytes());
        Ok(out)
    }
    #[cfg(any(test, feature = "sqlx-0_9-postgres", feature = "postgres-types-0_2"))]
    pub(crate) fn len(&self) -> usize {
        self.bytes.len() - self.start
    }
    pub(crate) fn write<E>(&self, mut sink: impl FnMut(&[u8]) -> Result<(), E>) -> Result<(), E> {
        sink(&self.bytes[self.start..])
    }
}

pub(crate) fn decode<D: DbDecimal>(bytes: &[u8]) -> Result<D, Error> {
    if bytes.len() < 8 {
        return Err(Error::InvalidInput);
    }
    let field = |i| u16::from_be_bytes([bytes[i], bytes[i + 1]]);
    // The count is unsigned even though the weight is signed.
    let count = usize::from(field(0));
    let weight = i64::from(field(2) as i16);
    let sign = field(4);
    let scale = field(6);
    if bytes.len() != 8 + count * 2 || scale > 16383 {
        return Err(Error::InvalidInput);
    }
    match sign {
        0 | 0x4000 => {}
        0xc000 | 0xd000 | 0xf000 => {
            // PostgreSQL numeric_recv: https://github.com/postgres/postgres/blob/REL_18_6/src/backend/utils/adt/numeric.c
            for pair in bytes[8..].as_chunks::<2>().0 {
                if u16::from_be_bytes(*pair) >= 10000 {
                    return Err(Error::InvalidInput);
                }
            }
            return D::special(sign);
        }
        _ => return Err(Error::InvalidInput),
    }
    let mut coefficient = 0_u128;
    let mut kept = 0_u32;
    let mut dropped = 0_i64;
    let mut sticky = false;
    for (i, pair) in bytes[8..].as_chunks::<2>().0.iter().enumerate() {
        let digit = u16::from_be_bytes([pair[0], pair[1]]);
        if digit >= 10000 {
            return Err(Error::InvalidInput);
        }
        // The server truncates hidden fractional digits. Reject those inputs
        // instead of silently accepting a lossy numeric value.
        let hidden = (-i64::from(scale) - 4 * (weight - i as i64)).clamp(0, 4) as u32;
        if !digit.is_multiple_of(10_u16.pow(hidden)) {
            return Err(Error::InvalidInput);
        }
        // Preserve exactly the first 35 significant decimal positions.
        // Leading zero groups do not consume retained precision.
        if kept == 0 {
            if digit == 0 {
                continue;
            }
            coefficient = u128::from(digit);
            kept = if digit < 10 {
                1
            } else if digit < 100 {
                2
            } else if digit < 1000 {
                3
            } else {
                4
            };
        } else if kept <= 31 {
            coefficient = coefficient * 10000 + u128::from(digit);
            kept += 4;
        } else if kept < 35 {
            // Split the sole group crossing the retained-prefix boundary.
            let (factor, divisor, discard) = match kept {
                32 => (1000_u128, 10_u16, 1_i64),
                33 => (100_u128, 100_u16, 2_i64),
                _ => (10_u128, 1000_u16, 3_i64),
            };
            coefficient = coefficient * factor + u128::from(digit / divisor);
            dropped += discard;
            sticky |= !digit.is_multiple_of(divisor);
            kept = 35;
        } else {
            dropped += 4;
            sticky |= digit != 0;
        }
    }
    let mut exponent = 4 * (weight - count as i64 + 1) + dropped;
    let preferred = -i64::from(scale);
    // Base-10000 alignment may add up to three decimal zero digits, and
    // noncanonical bodies may add whole zero groups. Fit can extend a
    // coefficient toward its preferred scale, but does not remove these
    // redundant zeros when the coefficient already fits the target width.
    if !sticky && coefficient != 0 {
        while exponent < preferred && coefficient.is_multiple_of(10) {
            coefficient /= 10;
            exponent += 1;
        }
    }
    // Wire zero has no sign, including noncanonical all-zero bodies.
    D::from_wire(
        sign == 0x4000 && coefficient != 0,
        coefficient,
        exponent,
        sticky,
        preferred,
    )
}
