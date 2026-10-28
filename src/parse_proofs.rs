use super::{LANE_POINT, load_short, parse_32, parse_64, parse_eight, zero_lanes};
use crate::layout::{InRange, W32, W64, Width};
use crate::{Error, Rounding};

#[kani::proof]
#[kani::unwind(10)]
fn zero_lanes_marks_exactly_the_zero_bytes() {
    let x: u64 = kani::any();
    let marks = zero_lanes(x);
    for lane in 0..8 {
        let byte = (x >> (8 * lane)) as u8;
        let mark = (marks >> (8 * lane)) as u8;
        assert_eq!(mark, if byte == 0 { 0x80 } else { 0 });
    }
}

#[kani::proof]
#[kani::unwind(10)]
fn parse_eight_reads_eight_ascii_digits() {
    let bytes: [u8; 8] = kani::any();
    let mut value = 0_u64;
    let mut all_digits = true;
    for &byte in &bytes {
        all_digits &= byte.is_ascii_digit();
        value = value * 10 + u64::from(byte.wrapping_sub(b'0') % 10);
    }
    assert_eq!(
        parse_eight(&bytes),
        if all_digits { Some(value) } else { None }
    );
}

macro_rules! load_short_proofs {
    ($($name:ident = $len:literal),* $(,)?) => {
        $(
            #[kani::proof]
            #[kani::unwind(10)]
            fn $name() {
                let bytes: [u8; $len] = kani::any();
                let mut expected = 0_u64;
                for (i, &byte) in bytes.iter().enumerate() {
                    expected |= u64::from(byte) << (8 * i);
                }
                assert_eq!(load_short(&bytes), expected);
            }
        )*
    };
}

load_short_proofs!(
    load_short_len0 = 0,
    load_short_len1 = 1,
    load_short_len2 = 2,
    load_short_len3 = 3,
    load_short_len4 = 4,
    load_short_len5 = 5,
    load_short_len6 = 6,
    load_short_len7 = 7,
    load_short_len8 = 8,
);

#[kani::proof]
fn point_lane_constant() {
    assert_eq!(LANE_POINT, u64::from_le_bytes([b'.'; 8]));
}

#[allow(dead_code)]
fn fallback<W: Width>(_s: &str, _dir: Option<Rounding>) -> Result<InRange<W>, Error> {
    kani::assume(false);
    unreachable!()
}

#[allow(dead_code)]
fn chunk_fallback<W: Width>(
    _s: &str,
    _neg: bool,
    _bytes: &[u8],
    _dir: Option<Rounding>,
) -> Result<InRange<W>, Error> {
    kani::assume(false);
    unreachable!()
}

struct Plain {
    neg: bool,
    coeff: u128,
    exp: i32,
    body_len: usize,
    valid: bool,
}

fn plain(bytes: &[u8]) -> Plain {
    let (neg, body) = match bytes.split_first() {
        Some((b'-', rest)) => (true, rest),
        Some((b'+', rest)) => (false, rest),
        _ => (false, bytes),
    };
    let mut coeff = 0_u128;
    let mut exp = 0_i32;
    let mut point = false;
    let mut any_digit = false;
    let mut valid = true;
    for &byte in body {
        if byte.is_ascii_digit() {
            any_digit = true;
            coeff = coeff * 10 + u128::from(byte - b'0');
            exp -= i32::from(point);
        } else if byte == b'.' && !point {
            point = true;
        } else {
            valid = false;
        }
    }
    Plain {
        neg,
        coeff,
        exp,
        body_len: body.len(),
        valid: valid && any_digit,
    }
}

fn any_ascii<const N: usize>() -> [u8; N] {
    let bytes: [u8; N] = kani::any();
    for &byte in &bytes {
        kani::assume(byte.is_ascii());
    }
    bytes
}

fn any_direction() -> Option<Rounding> {
    match kani::any::<u8>() {
        0 => Some(Rounding::TowardNegative),
        1 => Some(Rounding::TowardPositive),
        _ => None,
    }
}

macro_rules! fast_path_proofs {
    ($parse:ident, $width:ty, $($name:ident = $len:literal),* $(,)?) => {
        $(
            #[kani::proof]
            #[kani::stub(super::parse_general, fallback)]
            #[kani::stub(super::parse_chunks, chunk_fallback)]
            #[kani::unwind(20)]
            fn $name() {
                let bytes = any_ascii::<$len>();
                let Ok(s) = core::str::from_utf8(&bytes) else {
                    panic!("ASCII is UTF-8");
                };
                let spec = plain(&bytes);
                match $parse(s, any_direction()) {
                    Err(Error::InvalidInput) => assert!(spec.body_len <= 1 && !spec.valid),
                    Err(_) => panic!("fast path produced an unexpected error"),
                    Ok(value) => {
                        let v = value.unpacked();
                        assert!(spec.valid);
                        assert!(spec.coeff < 10_u128.pow(<$width as Width>::LAYOUT.digits));
                        assert!(v.neg == spec.neg && v.coeff == spec.coeff && v.exp == spec.exp);
                    }
                }
            }
        )*
    };
}

fast_path_proofs!(
    parse_32,
    W32,
    parse_32_fast_path_len0 = 0,
    parse_32_fast_path_len1 = 1,
    parse_32_fast_path_len2 = 2,
    parse_32_fast_path_len3 = 3,
    parse_32_fast_path_len4 = 4,
    parse_32_fast_path_len5 = 5,
    parse_32_fast_path_len6 = 6,
    parse_32_fast_path_len7 = 7,
    parse_32_fast_path_len8 = 8,
    parse_32_fast_path_len9 = 9,
);

fast_path_proofs!(parse_64, W64, parse_64_fast_path_len8 = 8,);
