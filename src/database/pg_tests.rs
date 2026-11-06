use super::{
    DbDecimal,
    pg_numeric::{PgNumeric, decode},
};
use crate::{Decimal32, Decimal64, Decimal128, Error};
use num_bigint::BigUint;
use std::{vec, vec::Vec};

fn wire(n: u16, weight: i16, sign: u16, scale: u16, digits: &[u16]) -> Vec<u8> {
    let mut out = Vec::new();
    for word in [n, weight as u16, sign, scale] {
        out.extend_from_slice(&word.to_be_bytes());
    }
    for word in digits {
        out.extend_from_slice(&word.to_be_bytes());
    }
    out
}
fn encoded<D: DbDecimal>(d: D) -> Result<Vec<u8>, Error> {
    let plan = PgNumeric::new(d)?;
    let mut result = Vec::new();
    plan.write(|bytes| {
        result.extend_from_slice(bytes);
        Ok::<_, Error>(())
    })?;
    assert_eq!(result.len(), plan.len());
    assert!(result.len() <= 28);
    Ok(result)
}
// Independent oracle: decimal BigUint extraction, full fixed-point string and
// four-character groups. No production parse, Display, or codec arithmetic.
fn reference(neg: bool, coefficient: u128, exp: i32) -> Vec<u8> {
    let scale = (-exp).max(0) as u16;
    if coefficient == 0 {
        return wire(0, 0, 0, scale, &[]);
    }
    let digits = BigUint::from(coefficient).to_str_radix(10);
    let point = digits.len() as i32 + exp;
    let left = point.max(0) as usize;
    let mut fixed = vec![b'0'; left + usize::from(scale)];
    if point < 0 {
        fixed[(-point) as usize..(-point) as usize + digits.len()]
            .copy_from_slice(digits.as_bytes());
    } else {
        fixed[..digits.len()].copy_from_slice(digits.as_bytes());
    }
    let pad_left = (4 - left % 4) % 4;
    let mut padded = vec![b'0'; pad_left];
    padded.extend_from_slice(&fixed);
    while !padded.len().is_multiple_of(4) {
        padded.push(b'0');
    }
    let groups: Vec<u16> = padded
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| c.iter().fold(0, |v, b| v * 10 + u16::from(b - b'0')))
        .collect();
    let first = groups.iter().position(|&v| v != 0).unwrap();
    let end = groups.iter().rposition(|&v| v != 0).unwrap() + 1;
    let weight = ((left + pad_left) / 4) as i32 - 1 - first as i32;
    wire(
        (end - first) as u16,
        weight as i16,
        if neg { 0x4000 } else { 0 },
        scale,
        &groups[first..end],
    )
}
macro_rules! width {
    ($test:ident, $d:ty, $i:ty, $p:expr) => {
        #[test]
        fn $test() {
            let max = 10_u128.pow($p) - 1;
            let mut values = vec![0, 1, 9, 10, 99, 100, 150, 9999, 10000, max];
            let mut state = 0x123456789abcdef_u128;
            for _ in 0..64 {
                state = state
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                values.push(state % max + 1);
            }
            for c in values {
                for e in [
                    <$d>::MIN_EXP,
                    <$d>::MIN_EXP + 1,
                    -33,
                    -9,
                    -8,
                    -7,
                    -4,
                    -3,
                    -2,
                    -1,
                    0,
                    1,
                    2,
                    3,
                    4,
                    7,
                    <$d>::MAX_EXP - 1,
                    <$d>::MAX_EXP,
                ] {
                    for neg in [false, true] {
                        let value = <$d>::from_coefficient_and_exponent(
                            if neg { -(c as $i) } else { c as $i },
                            e,
                        )
                        .unwrap();
                        let value = if c == 0 && neg { -value } else { value };
                        let bytes = encoded(value).unwrap();
                        assert_eq!(bytes, reference(neg, c, e), "c={c} e={e} neg={neg}");
                        let roundtrip = decode::<$d>(&bytes).unwrap();
                        assert_eq!(value, roundtrip, "c={c} e={e} neg={neg}");
                        if e <= 0 {
                            assert_eq!(roundtrip.exponent().unwrap(), e);
                        }
                        if c == 0 {
                            assert!(!roundtrip.is_sign_negative());
                        }
                    }
                }
            }
            for (value, sign) in [
                (<$d>::NAN, 0xc000),
                (<$d>::INFINITY, 0xd000),
                (<$d>::NEG_INFINITY, 0xf000),
            ] {
                let bytes = wire(0, 0, sign, 0, &[]);
                assert_eq!(encoded(value).unwrap(), bytes);
                assert_eq!(decode::<$d>(&bytes).unwrap().to_bits(), value.to_bits());
            }
            assert_eq!(encoded(-<$d>::NAN), Err(Error::InvalidInput));
            let payload = <$d>::from_bits(<$d>::NAN.to_bits() | 1).unwrap();
            let signaling = <$d>::from_bits(<$d>::NAN.to_bits() | (1 << (<$i>::BITS - 7))).unwrap();
            assert_eq!(encoded(payload), Err(Error::InvalidInput));
            assert_eq!(encoded(signaling), Err(Error::InvalidInput));
            let zero = decode::<$d>(&wire(0, 123, 0x4000, 16383, &[])).unwrap();
            assert_eq!(zero.coefficient().unwrap(), 0);
            assert_eq!(zero.exponent().unwrap(), <$d>::MIN_EXP);
            assert!(!zero.is_sign_negative());
        }
    };
}
width!(pg_decimal32, Decimal32, i32, 7);
width!(pg_decimal64, Decimal64, i64, 16);
width!(pg_decimal128, Decimal128, i128, 34);

#[test]
fn pg_known_cohorts_and_precision() {
    let one_fifty = wire(2, 0, 0, 2, &[1, 5000]);
    assert_eq!(one_fifty, [0, 2, 0, 0, 0, 0, 0, 2, 0, 1, 19, 136]);
    let d = decode::<Decimal32>(&one_fifty).unwrap();
    assert_eq!(d.coefficient().unwrap(), 150);
    assert_eq!(d.exponent().unwrap(), -2);
    let redundant = wire(5, 1, 0, 2, &[0, 1, 5000, 0, 0]);
    assert_eq!(
        decode::<Decimal32>(&redundant).unwrap().to_bits(),
        d.to_bits()
    );
    let exact = decode::<Decimal32>(&wire(2, 1, 0, 2, &[123, 4567])).unwrap();
    assert_eq!(exact.coefficient().unwrap(), 1234567);
    assert_eq!(exact.exponent().unwrap(), 0);
    assert_eq!(
        decode::<Decimal32>(&wire(2, 1, 0, 0, &[1234, 5678])),
        Err(Error::Inexact)
    );
    assert_eq!(
        decode::<Decimal128>(&wire(1, -1545, 0, 6177, &[1000])),
        Err(Error::Inexact)
    );
    assert_eq!(
        decode::<Decimal128>(&wire(1, 2000, 0, 0, &[1])),
        Err(Error::Overflow)
    );
    // Kept 35-digit prefix plus nonzero discarded digit must be inexact.
    assert_eq!(
        decode::<Decimal128>(&wire(10, 9, 0, 0, &[1, 0, 0, 0, 0, 0, 0, 0, 0, 1])),
        Err(Error::Inexact)
    );
    assert_eq!(
        encoded(Decimal32::from_bits(0x7c00_0001).unwrap()),
        Err(Error::InvalidInput)
    );
    assert_eq!(
        encoded(Decimal32::from_bits(0x7e00_0000).unwrap()),
        Err(Error::InvalidInput)
    );
}

#[test]
fn pg_rejects_malformed_entire_input() {
    let invalid = [
        wire(1, 0, 0, 0, &[]),
        wire(0, 0, 0, 0, &[0]),
        wire(1, 0, 0, 0, &[10000]),
        wire(0, 0, 0x8000, 0, &[]),
        wire(0, 0, 1, 0, &[]),
        wire(0, 0, 0, 16384, &[]),
        wire(1, -1, 0, 2, &[1234]),
        wire(1, -2, 0, 4, &[1]),
        wire(1, 0, 0xc000, 0, &[10000]),
        wire(0, 1, 0xc000, 16384, &[]),
        wire(1, 0, 0xd000, 1, &[]),
    ];
    for bytes in invalid {
        assert_eq!(decode::<Decimal32>(&bytes), Err(Error::InvalidInput));
        assert_eq!(decode::<Decimal64>(&bytes), Err(Error::InvalidInput));
        assert_eq!(decode::<Decimal128>(&bytes), Err(Error::InvalidInput));
    }
    for n in 0..8 {
        assert_eq!(decode::<Decimal32>(&[0; 8][..n]), Err(Error::InvalidInput));
    }
    let mut digits = vec![0; 32769];
    digits[0] = 1;
    let huge = wire(32769, 32767, 0, 0, &digits);
    assert_eq!(decode::<Decimal128>(&huge), Err(Error::Overflow));
    // An otherwise out-of-range prefix cannot mask a later malformed digit.
    digits[32768] = 10000;
    assert_eq!(
        decode::<Decimal128>(&wire(32769, 32767, 0, 0, &digits)),
        Err(Error::InvalidInput)
    );
    // Largest u16 count, all-zero body: framing is valid and fits exactly.
    assert_eq!(
        decode::<Decimal32>(&wire(u16::MAX, 0, 0, 0, &vec![0; 65535])).unwrap(),
        Decimal32::ZERO
    );
}

#[test]
fn pg_sink_failures_and_generated_inputs() {
    let value = Decimal128::from_coefficient_and_exponent(12345, -2).unwrap();
    let plan = PgNumeric::new(value).unwrap();
    let mut calls = 0;
    assert_eq!(
        plan.write(|_| {
            calls += 1;
            Err::<(), _>(42)
        }),
        Err(42)
    );
    assert_eq!(calls, 1);
    calls = 0;
    assert_eq!(
        plan.write(|bytes| {
            calls += 1;
            assert_eq!(bytes, reference(false, 12345, -2));
            Ok::<(), i32>(())
        }),
        Ok(())
    );
    assert_eq!(calls, 1);
    let mut state = 123_u64;
    for n in 0..2000 {
        let mut bytes = vec![0; n % 129];
        for b in &mut bytes {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            *b = (state >> 32) as u8;
        }
        let _ = decode::<Decimal32>(&bytes);
        let _ = decode::<Decimal64>(&bytes);
        let _ = decode::<Decimal128>(&bytes);
    }
}

// Literal server output, not bytes constructed by our encoder or wire helper.
fn check_server_vectors<D: DbDecimal>(column: usize) {
    let fixtures = include_str!("testdata/postgres-18.6-numeric-send.tsv");
    let mut count = 0;
    for line in fixtures
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
    {
        let columns: Vec<_> = line.split_whitespace().collect();
        assert_eq!(columns.len(), 8);
        let hex = columns[1];
        assert!(hex.len().is_multiple_of(2));
        let bytes: Vec<_> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        let value = decode::<D>(&bytes).unwrap();
        match columns[column] {
            "NaN" => assert_eq!(value.pg_special().unwrap(), Some(0xc000)),
            "Infinity" => assert_eq!(value.pg_special().unwrap(), Some(0xd000)),
            "-Infinity" => assert_eq!(value.pg_special().unwrap(), Some(0xf000)),
            coefficient => {
                let coefficient: i128 = coefficient.parse().unwrap();
                let exponent: i32 = columns[column + 1].parse().unwrap();
                assert_eq!(
                    value.parts().unwrap(),
                    (coefficient < 0, coefficient.unsigned_abs(), exponent),
                    "{}",
                    columns[0]
                );
                assert_eq!(encoded(value).unwrap(), bytes, "{}", columns[0]);
            }
        }
        count += 1;
    }
    assert_eq!(count, 10);
}
#[test]
fn pg_server_numeric_send_vectors_all_widths() {
    check_server_vectors::<Decimal32>(2);
    check_server_vectors::<Decimal64>(4);
    check_server_vectors::<Decimal128>(6);
}

fn check_special_semantics<D: DbDecimal>() {
    for sign in [0xc000, 0xd000, 0xf000] {
        // No magic exception for observed scale32: every permitted dscale
        // is ignored semantically for a special, as in numeric_recv.
        for scale in 0..=16383 {
            let bytes = wire(0, i16::MIN, sign, scale, &[]);
            assert_eq!(
                decode::<D>(&bytes).unwrap().pg_special().unwrap(),
                Some(sign)
            );
        }
        for weight in [i16::MIN, -1, 0, 1, i16::MAX] {
            for scale in [0, 1, 31, 32, 33, 16383] {
                // A finite interpretation would contain hidden fractions for
                // some weights/scales. Specials have no fractional value.
                for digits in [&[][..], &[0][..], &[1, 9999, 0][..]] {
                    let bytes = wire(digits.len() as u16, weight, sign, scale, digits);
                    assert_eq!(
                        decode::<D>(&bytes).unwrap().pg_special().unwrap(),
                        Some(sign)
                    );
                }
            }
        }
        for bytes in [
            wire(0, 0, sign, 16384, &[]),
            wire(0, 0, sign, u16::MAX, &[]),
            wire(1, 0, sign, 0, &[]),
            wire(0, 0, sign, 0, &[0]),
            wire(1, 0, sign, 0, &[10000]),
            wire(1, 0, sign, 0, &[u16::MAX]),
        ] {
            assert!(matches!(decode::<D>(&bytes), Err(Error::InvalidInput)));
        }
        let mut digits = vec![9999; 65535];
        let bytes = wire(u16::MAX, i16::MAX, sign, 16383, &digits);
        assert_eq!(
            decode::<D>(&bytes).unwrap().pg_special().unwrap(),
            Some(sign)
        );
        digits[65534] = 10000;
        assert!(matches!(
            decode::<D>(&wire(u16::MAX, i16::MAX, sign, 16383, &digits)),
            Err(Error::InvalidInput)
        ));
    }
}
#[test]
fn pg_special_metadata_and_full_digit_validation_all_widths() {
    check_special_semantics::<Decimal32>();
    check_special_semantics::<Decimal64>();
    check_special_semantics::<Decimal128>();
}
