#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use crate::kernel::{Wide, fit};
use crate::layout::{InRange, W32, W64, Width};
use crate::{Error, Rounding};

const EXP_LIMIT: i128 = 1_i128 << 64;
const SCALE_LIMIT: i128 = 1_i128 << 40;

#[inline(always)]
fn split_sign(s: &str) -> (bool, &[u8]) {
    match s.as_bytes().split_first() {
        Some((b'-', rest)) => (true, rest),
        Some((b'+', rest)) => (false, rest),
        _ => (false, s.as_bytes()),
    }
}

#[inline(always)]
pub(crate) fn parse<W: Width>(s: &str, dir: Option<Rounding>) -> Result<InRange<W>, Error> {
    let (neg, bytes) = split_sign(s);
    parse_chunks(s, neg, bytes, dir)
}

#[inline(always)]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "the SWAR path has 1..=8 bytes, so the pad 8*(8-len) is in 0..=56; the point lane is in 0..=7 because without a point trailing_zeros(0)/8 is 8 and & 7 maps it to 0, so both lane shifts are <=63 and the removed word computed in that case is unused; lane-7 fits i32, and (1<<pad)-1 and (1<<8*lane)-1 cannot underflow"
)]
pub(crate) fn parse_32(s: &str, dir: Option<Rounding>) -> Result<InRange<W32>, Error> {
    let (neg, bytes) = split_sign(s);
    if bytes.len() > 8 {
        return parse_chunks(s, neg, bytes, dir);
    }
    let len = bytes.len();
    if len == 0 {
        return Err(Error::InvalidInput);
    }
    let pad = 8 * (8 - len) as u32;
    let word = (load_short(bytes) << pad) | (LANE_ZERO & ((1_u64 << pad) - 1));
    let point = zero_lanes(word ^ LANE_POINT);
    if point & point.wrapping_sub(1) != 0 {
        return parse_general(s, dir);
    }
    if len == usize::from(point != 0) {
        return Err(Error::InvalidInput);
    }
    let lane = (point.trailing_zeros() / 8) & 7;
    let below = (1_u64 << (8 * lane)) - 1;
    let above = (u64::MAX << (8 * lane)) << 8;
    let removed = ((word & below) << 8) | (word & above) | 0x30;
    let (digits, exp) = if point == 0 {
        (word, 0)
    } else {
        (removed, lane as i32 - 7)
    };
    if let Some(coeff) = parse_eight(&digits.to_le_bytes())
        && let Some(value) = InRange::checked(neg, u128::from(coeff), i128::from(exp))
    {
        return Ok(value);
    }
    parse_general(s, dir)
}

#[inline(always)]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "the SWAR path has 8..=17 bytes and moves the first of 17 into the lead, so the body has 8..=16 bytes and the pad 8*(16-len) is in 0..=64 < 128, where (1<<pad)-1 cannot underflow; the point lane is in 0..=15 because without a point trailing_zeros(0)/8 is 16 and & 15 maps it to 0, so the lane shifts are at most 120 and lane-15 fits i32; nibbles are at most 0x0f per lane, so adding 0x06 per lane cannot carry out of the word; validated lanes are digits, so pair, quad and half sums stay below 100, 10^4 and 10^8 within their lanes, and the coefficient is below 10^16"
)]
pub(crate) fn parse_64(s: &str, dir: Option<Rounding>) -> Result<InRange<W64>, Error> {
    let (neg, bytes) = split_sign(s);
    if !(8..=17).contains(&bytes.len()) {
        return parse_chunks(s, neg, bytes, dir);
    }
    let (lead, body) = match bytes.split_first() {
        Some((&lead, body)) if bytes.len() == 17 => (lead, body),
        _ => (b'0', bytes),
    };
    let (Some(first), Some(last)) = (body.first_chunk::<8>(), body.last_chunk::<8>()) else {
        return parse_chunks(s, neg, bytes, dir);
    };
    let pad = 8 * (16 - body.len()) as u32;
    let fill = ((1_u128 << pad) - 1) as u64;
    let head = (u128::from(u64::from_le_bytes(*first)) << pad) as u64 | (LANE_ZERO & fill);
    let tail = u64::from_le_bytes(*last);
    let word = u128::from(head) | (u128::from(tail) << 64);
    let point = u128::from(zero_lanes(head ^ LANE_POINT))
        | (u128::from(zero_lanes(tail ^ LANE_POINT)) << 64);
    if point & point.wrapping_sub(1) != 0 {
        return parse_general(s, dir);
    }
    let lane = (point.trailing_zeros() / 8) & 15;
    let below = (1_u128 << (8 * lane)) - 1;
    let above = (u128::MAX << (8 * lane)) << 8;
    let removed = ((word & below) << 8) | (word & above) | u128::from(lead);
    let (digits, exp) = match (point, lead) {
        (0, b'0') => (word, 0),
        (0, b'.') => (word, -16),
        (0, _) => return parse_chunks(s, neg, bytes, dir),
        _ => (removed, lane as i32 - 15),
    };
    let nibbles = digits & 0x0f0f_0f0f_0f0f_0f0f_0f0f_0f0f_0f0f_0f0f;
    if digits & 0xf0f0_f0f0_f0f0_f0f0_f0f0_f0f0_f0f0_f0f0
        == 0x3030_3030_3030_3030_3030_3030_3030_3030
        && (nibbles + 0x0606_0606_0606_0606_0606_0606_0606_0606)
            & 0x1010_1010_1010_1010_1010_1010_1010_1010
            == 0
    {
        let high = nibbles as u64;
        let low = (nibbles >> 64) as u64;
        let pairs_high =
            (high & 0x00ff_00ff_00ff_00ff) * 10 + ((high >> 8) & 0x00ff_00ff_00ff_00ff);
        let pairs_low = (low & 0x00ff_00ff_00ff_00ff) * 10 + ((low >> 8) & 0x00ff_00ff_00ff_00ff);
        let quads_high = (pairs_high & 0x0000_ffff_0000_ffff) * 100
            + ((pairs_high >> 16) & 0x0000_ffff_0000_ffff);
        let quads_low =
            (pairs_low & 0x0000_ffff_0000_ffff) * 100 + ((pairs_low >> 16) & 0x0000_ffff_0000_ffff);
        let coeff = ((quads_high & 0xffff_ffff) * 10_000 + (quads_high >> 32)) * 100_000_000
            + (quads_low & 0xffff_ffff) * 10_000
            + (quads_low >> 32);
        if let Some(value) = InRange::checked(neg, u128::from(coeff), i128::from(exp)) {
            return Ok(value);
        }
    }
    parse_general(s, dir)
}

#[inline(always)]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "at most 35 bytes give coeff < 10^35 < 2^127; validated chunks are <10^8; after chunk scanning each digit run has <=7 bytes, so tail <10^7 and factor <=10^7 fit u32; ASCII guards protect subtraction, and remaining length <=35 fits i32"
)]
fn parse_chunks<W: Width>(
    s: &str,
    neg: bool,
    mut bytes: &[u8],
    dir: Option<Rounding>,
) -> Result<InRange<W>, Error> {
    if bytes.len() > 35 {
        return parse_general(s, dir);
    }
    let len = bytes.len();
    let mut coeff = 0_u128;
    let mut point = false;
    let mut exp = 0_i32;
    loop {
        while let Some((chunk, rest)) = bytes.split_first_chunk::<8>() {
            let Some(value) = parse_eight(chunk) else {
                break;
            };
            coeff = coeff * 100_000_000 + u128::from(value);
            bytes = rest;
        }
        let mut tail = 0_u32;
        let mut factor = 1_u32;
        while let Some((&byte, rest)) = bytes.split_first() {
            if !byte.is_ascii_digit() {
                break;
            }
            tail = tail * 10 + u32::from(byte - b'0');
            factor *= 10;
            bytes = rest;
        }
        coeff = coeff * u128::from(factor) + u128::from(tail);
        match bytes.split_first() {
            None => break,
            Some((b'.', rest)) if !point => {
                point = true;
                exp = -(rest.len() as i32);
                bytes = rest;
            }
            _ => return parse_general(s, dir),
        }
    }
    if len <= usize::from(point) {
        return Err(Error::InvalidInput);
    }
    if let Some(value) = InRange::checked(neg, coeff, i128::from(exp)) {
        return Ok(value);
    }
    parse_general(s, dir)
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "at most 35 significant digits give coeff < 10^35 < 2^127; each digit subtraction is guarded by ASCII range. Counts <= input length <= isize::MAX <= 2^63-1; written exponent <= 2^64 and its next step <= 10*2^64+9, so all i128 arithmetic fits. Clamping is monotone, preferred <= wide exponent; sticky requires 35 retained digits, hence coeff >= 10^34 >= 10^(P-1) for P=7/16/34"
)]
#[inline(never)]
pub(crate) fn parse_general<W: Width>(s: &str, dir: Option<Rounding>) -> Result<InRange<W>, Error> {
    let mut bytes = s.bytes().peekable();
    let neg = match bytes.peek() {
        Some(b'-') => {
            bytes.next();
            true
        }
        Some(b'+') => {
            bytes.next();
            false
        }
        _ => false,
    };
    let mut coeff = 0_u128;
    let mut retained = 0_u32;
    let mut fraction = 0_i128;
    let mut dropped = 0_i128;
    let mut point = false;
    let mut any_digit = false;
    let mut sticky = false;
    while let Some(&byte) = bytes.peek() {
        match byte {
            b'0'..=b'9' => {
                bytes.next();
                any_digit = true;
                fraction += i128::from(point);
                let digit = u128::from(byte - b'0');
                if retained < 35 {
                    if coeff != 0 || digit != 0 {
                        coeff = coeff * 10 + digit;
                        retained += 1;
                    }
                } else {
                    dropped += 1;
                    sticky |= digit != 0;
                }
            }
            b'.' if !point => {
                bytes.next();
                point = true;
            }
            _ => break,
        }
    }
    if !any_digit {
        return Err(Error::InvalidInput);
    }
    let mut written = 0_i128;
    if let Some(byte) = bytes.next() {
        if byte != b'e' && byte != b'E' {
            return Err(Error::InvalidInput);
        }
        let exp_neg = match bytes.peek() {
            Some(b'-') => {
                bytes.next();
                true
            }
            Some(b'+') => {
                bytes.next();
                false
            }
            _ => false,
        };
        let mut exp_digit = false;
        for byte in bytes {
            if !byte.is_ascii_digit() {
                return Err(Error::InvalidInput);
            }
            exp_digit = true;
            written = (written * 10 + i128::from(byte - b'0')).min(EXP_LIMIT);
        }
        if !exp_digit {
            return Err(Error::InvalidInput);
        }
        if exp_neg {
            written = -written;
        }
    }
    let preferred = written - fraction;
    if dropped == 0
        && let Some(value) = InRange::checked(neg, coeff, preferred)
    {
        return Ok(value);
    }
    fit(
        Wide {
            neg,
            coeff,
            exp: (preferred + dropped).clamp(-SCALE_LIMIT, SCALE_LIMIT) as i64,
            sticky,
        },
        preferred.clamp(-SCALE_LIMIT, SCALE_LIMIT) as i64,
        dir,
    )
}

const LANE_ZERO: u64 = 0x3030_3030_3030_3030;
const LANE_POINT: u64 = 0x2e2e_2e2e_2e2e_2e2e;

#[inline(always)]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "the only caller passes 1..=8 bytes; with >=4 bytes the shift 8*(len-4) is in 0..=32, and the empty and >=4-byte arms of the inner match are unreachable"
)]
fn load_short(bytes: &[u8]) -> u64 {
    match (bytes.first_chunk::<4>(), bytes.last_chunk::<4>()) {
        (Some(head), Some(tail)) => {
            u64::from(u32::from_le_bytes(*head))
                | (u64::from(u32::from_le_bytes(*tail)) << (8 * (bytes.len() - 4)))
        }
        _ => match *bytes {
            [a] => u64::from(a),
            [a, b] => u64::from(u16::from_le_bytes([a, b])),
            [a, b, c] => u64::from(u16::from_le_bytes([a, b])) | (u64::from(c) << 16),
            [] | [_, _, _, _, ..] => 0,
        },
    }
}

#[inline(always)]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "each masked lane is <=0x7f, so adding 0x7f is <=0xfe and cannot carry into the next lane or overflow u64"
)]
const fn zero_lanes(x: u64) -> u64 {
    !((((x & 0x7f7f_7f7f_7f7f_7f7f) + 0x7f7f_7f7f_7f7f_7f7f) | x) | 0x7f7f_7f7f_7f7f_7f7f)
}

const _: () = {
    let mut pos = 0;
    while pos < 8 {
        let lane = 0xff_u64 << (8 * pos);
        let mut value = 0;
        while value <= 255 {
            let one = (value as u64) << (8 * pos);
            let hit = if value == 0 { 0x80_u64 << (8 * pos) } else { 0 };
            assert!(zero_lanes(one) == hit | (0x8080_8080_8080_8080 & !lane));
            assert!(zero_lanes(one | !lane) == hit);
            value += 1;
        }
        pos += 1;
    }
};

#[inline]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "nibble lanes <=15 plus 6 are <=21 with no byte carry; validated digits <=9 form pairs <=99 and groups <=9999 with no lane carry, and the final value <=99999999 fits u64"
)]
const fn parse_eight(bytes: &[u8; 8]) -> Option<u64> {
    let word = u64::from_le_bytes(*bytes);
    let digits = word & 0x0f0f_0f0f_0f0f_0f0f;
    if word & 0xf0f0_f0f0_f0f0_f0f0 != 0x3030_3030_3030_3030
        || (digits + 0x0606_0606_0606_0606) & 0x1010_1010_1010_1010 != 0
    {
        return None;
    }
    let pairs = (digits & 0x00ff_00ff_00ff_00ff) * 10 + ((digits >> 8) & 0x00ff_00ff_00ff_00ff);
    let groups = (pairs & 0x0000_ffff_0000_ffff) * 100 + ((pairs >> 16) & 0x0000_ffff_0000_ffff);
    Some((groups & 0xffff_ffff) * 10_000 + (groups >> 32))
}

#[expect(
    clippy::indexing_slicing,
    reason = "compile-time enumeration indexes the eight-byte array only while pos < 8"
)]
const _: () = {
    let mut pos = 0;
    while pos < 8 {
        let mut value = 0;
        while value <= 255 {
            let mut bytes = [b'0'; 8];
            bytes[pos] = value as u8;
            match parse_eight(&bytes) {
                Some(n) => {
                    assert!(value >= 48 && value <= 57);
                    assert!(n == (value as u64 - 48) * 10_u64.pow(7 - pos as u32));
                }
                None => assert!(value < 48 || value > 57),
            }
            value += 1;
        }
        pos += 1;
    }
    assert!(matches!(parse_eight(b"99999999"), Some(99_999_999)));
    assert!(10_u32.pow(8) < u32::MAX);
};

const _: () = assert!(usize::BITS <= 64 && 10_u128.pow(35) < (1_u128 << 127));

#[cfg(kani)]
#[path = "parse_proofs.rs"]
mod proofs;
