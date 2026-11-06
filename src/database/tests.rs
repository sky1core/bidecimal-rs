use super::*;
use num_bigint::BigUint;
use std::{
    format,
    string::{String, ToString},
    vec::Vec,
};

fn text<D: DbDecimal>(d: D) -> Result<String, Error> {
    let p = MysqlText::new(d)?;
    let mut out = Vec::new();
    p.write(|b| {
        out.extend_from_slice(b);
        Ok::<_, Error>(())
    })?;
    assert_eq!(p.len(), out.len());
    assert!(out.len() <= 67);
    Ok(String::from_utf8(out).unwrap())
}
// Independent fixed-point reference, using BigUint decimal extraction. This
// never uses production Display, parse or the codec to construct expectations.
fn reference(negative: bool, c: u128, e: i32) -> String {
    let s = BigUint::from(c).to_str_radix(10);
    let mut out = String::from(if negative { "-" } else { "" });
    if e >= 0 {
        out.push_str(&s);
        out.push_str(&"0".repeat(e as usize));
    } else {
        let scale = (-e) as usize;
        if s.len() > scale {
            out.push_str(&s[..s.len() - scale]);
            out.push('.');
            out.push_str(&s[s.len() - scale..]);
        } else {
            out.push_str("0.");
            out.push_str(&"0".repeat(scale - s.len()));
            out.push_str(&s);
        }
    }
    out
}
macro_rules! width {
    ($name:ident,$d:ty,$i:ty,$p:expr) => {
        #[test]
        fn $name() {
            for c in [0_u128, 1, 9, 10, 15, 100, 150, 10_u128.pow($p) - 1] {
                for e in -30..=65 {
                    for neg in [false, true] {
                        let signed = if neg { -(c as $i) } else { c as $i };
                        let d = <$d>::from_coefficient_and_exponent(signed, e).unwrap();
                        let d = if c == 0 && neg { -d } else { d };
                        let integer = ((BigUint::from(c).to_str_radix(10).len() as i32) + e).max(0);
                        let valid = c == 0 || integer + (-e).max(0) <= 65;
                        if !valid {
                            assert!(text(d).is_err());
                            continue;
                        }
                        let wanted = reference(neg, c, if c == 0 { e.clamp(-30, 0) } else { e });
                        assert_eq!(text(d).unwrap(), wanted);
                        let decoded: $d = decode_text(wanted.as_bytes(), false).unwrap();
                        assert_eq!(decoded, d);
                        // Incoming scale is retained when representable.
                        if e <= 0 && c != 0 {
                            assert_eq!(decoded.exponent().unwrap(), e);
                        }
                    }
                }
            }
            for c in [1, 10, 100, 1000] {
                let d = <$d>::from_coefficient_and_exponent(c, -31).unwrap();
                if c == 1 {
                    assert!(text(d).is_err());
                } else {
                    assert_eq!(
                        text(d).unwrap(),
                        reference(false, 1, -31 + (c as u32).ilog10() as i32)
                    );
                }
            }
            for special in [<$d>::NAN, <$d>::INFINITY, <$d>::NEG_INFINITY] {
                assert_eq!(text(special), Err(Error::InvalidInput));
            }
            // Exact value wins over an unrepresentable input scale.
            let c = 10_u128.pow($p) - 1;
            let input = format!("{}.00", BigUint::from(c));
            let d: $d = decode_text(input.as_bytes(), false).unwrap();
            assert_eq!(d.coefficient().unwrap() as u128, c);
            assert_eq!(d.exponent().unwrap(), 0);
            let input = format!("{}1", BigUint::from(c));
            assert_eq!(
                decode_text::<$d>(input.as_bytes(), false),
                Err(Error::Inexact)
            );
            let low = reference(false, 1, <$d>::MIN_EXP);
            let d: $d = decode_text(low.as_bytes(), false).unwrap();
            assert_eq!(d.coefficient().unwrap(), 1);
            assert_eq!(d.exponent().unwrap(), <$d>::MIN_EXP);
            let high = reference(false, c, <$d>::MAX_EXP);
            let d: $d = decode_text(high.as_bytes(), false).unwrap();
            assert_eq!(d.coefficient().unwrap() as u128, c);
            assert_eq!(d.exponent().unwrap(), <$d>::MAX_EXP);
            assert_eq!(
                decode_text::<$d>(format!("{}0", high).as_bytes(), false),
                Err(Error::Overflow)
            );
        }
    };
}
width!(decimal32_boundaries, Decimal32, i32, 7);
width!(decimal64_boundaries, Decimal64, i64, 16);
width!(decimal128_boundaries, Decimal128, i128, 34);

#[test]
fn invalid_text_and_full_scan() {
    for bytes in [
        b"".as_slice(),
        b".",
        b"-",
        b"1.2.3",
        b" 1",
        b"1 ",
        b"1e1",
        b"NaN",
        b"Infinity",
        b"\xff",
        b"\0",
        b"--1",
    ] {
        assert_eq!(
            decode_text::<Decimal128>(bytes, false),
            Err(Error::InvalidInput)
        );
    }
    let input = format!("{}x", "9".repeat(100_000));
    assert_eq!(
        decode_text::<Decimal32>(input.as_bytes(), false),
        Err(Error::InvalidInput)
    );
    let zeros = format!("0.{}", "0".repeat(100_000));
    let zero = decode_text::<Decimal32>(zeros.as_bytes(), false).unwrap();
    assert_eq!(zero.exponent().unwrap(), Decimal32::MIN_EXP);
    assert_eq!(zero.coefficient().unwrap(), 0);
}

#[test]
fn format_u64_boundary_and_normalization() {
    for c in [
        u128::from(u64::MAX) - 5,
        u128::from(u64::MAX),
        u128::from(u64::MAX) + 1,
        u128::from(u64::MAX) + 5,
    ] {
        for e in [0, -2, -30, -31, 45, 46] {
            let d = Decimal128::from_coefficient_and_exponent(c as i128, e).unwrap();
            if e == 46 || (e == -31 && c % 10 != 0) {
                assert!(text(d).is_err());
            } else if e == -31 {
                assert_eq!(text(d).unwrap(), reference(false, c / 10, -30));
            } else {
                assert_eq!(text(d).unwrap(), reference(false, c, e));
            }
        }
    }
}

#[test]
fn restricted_grammar_matches_independent_ascii_recognizer() {
    fn valid(mut bytes: &[u8]) -> bool {
        if matches!(bytes.first(), Some(b'+' | b'-')) {
            bytes = &bytes[1..];
        }
        let mut point = false;
        let mut digit = false;
        for &b in bytes {
            match b {
                b'0'..=b'9' => digit = true,
                b'.' if !point => point = true,
                _ => return false,
            }
        }
        digit
    }
    let alphabet = b"+-012.eENaInf x\0\xff";
    for len in 0..=4 {
        for mut n in 0..alphabet.len().pow(len) {
            let mut bytes = [0_u8; 4];
            for b in &mut bytes[..len as usize] {
                *b = alphabet[n % alphabet.len()];
                n /= alphabet.len();
            }
            let bytes = &bytes[..len as usize];
            assert_eq!(
                decode_text::<Decimal128>(bytes, false).is_ok(),
                valid(bytes),
                "{bytes:?}"
            );
        }
    }
}

#[test]
fn normalization_uses_existing_trim_and_sink_errors_survive() {
    for negative in [false, true] {
        let d = Decimal128::from_coefficient_and_exponent(if negative { -1000 } else { 1000 }, -31)
            .unwrap();
        assert_eq!(text(d).unwrap(), d.trim_fraction_zeros().to_string());
        let zero = Decimal128::from_coefficient_and_exponent(0, -100).unwrap();
        let zero = if negative { -zero } else { zero };
        assert_eq!(text(zero).unwrap(), zero.trim_fraction_zeros().to_string());
    }
    // Both the short lane and wide Parts path return the original sink error.
    for coefficient in [15, 1234567890123456789012345678901234] {
        let d = Decimal128::from_coefficient_and_exponent(coefficient, -2).unwrap();
        let mut calls = 0;
        let result = MysqlText::new(d).unwrap().write(|_| {
            calls += 1;
            Err(91u8)
        });
        assert!(matches!(result, Err(TextWriteError::Sink(91))));
        assert_eq!(calls, 1);
    }
}

// Promoted unchanged in behavior from the paired A/A/B 32-test snapshots.
// These independent-oracle cases exercise cached digit counts across trimming.
#[test]
fn normalization_crosses_u64_digit_count_lane() {
    for magnitude in [
        u128::from(u64::MAX) - 1,
        u128::from(u64::MAX),
        u128::from(u64::MAX) + 1,
    ] {
        for negative in [false, true] {
            let coefficient = magnitude * 10;
            let signed = if negative {
                -(coefficient as i128)
            } else {
                coefficient as i128
            };
            let value = Decimal128::from_coefficient_and_exponent(signed, -31).unwrap();
            // -31 scale requires normalization. All chosen magnitudes are
            // nonmultiples of ten, so exactly one trailing zero is removed.
            assert_eq!(text(value).unwrap(), reference(negative, magnitude, -30));
        }
    }
}

#[test]
fn structural_digit_count_boundaries_and_extreme_zeros() {
    macro_rules! check {
        ($d:ty,$i:ty,$precision:expr) => {{
            for power in 0..=$precision {
                for c in [
                    10_u128.pow(power).saturating_sub(1),
                    10_u128.pow(power),
                    10_u128.pow(power) + 1,
                ] {
                    if c >= 10_u128.pow($precision) {
                        continue;
                    }
                    for e in [-31, -30, -1, 0, 30, 65] {
                        for negative in [false, true] {
                            let value = <$d>::from_coefficient_and_exponent(
                                if negative { -(c as $i) } else { c as $i },
                                e,
                            )
                            .unwrap();
                            let value = if c == 0 && negative { -value } else { value };
                            let mut coeff = c;
                            let mut exp = e;
                            let fits = |c: u128, e: i32| {
                                (-e).max(0) <= 30
                                    && (c == 0
                                        || (BigUint::from(c).to_str_radix(10).len() as i32 + e)
                                            .max(0)
                                            + (-e).max(0)
                                            <= 65)
                            };
                            if !fits(coeff, exp) {
                                if coeff == 0 {
                                    exp = 0;
                                } else {
                                    while exp < 0 && coeff % 10 == 0 {
                                        coeff /= 10;
                                        exp += 1;
                                    }
                                }
                            }
                            if fits(coeff, exp) {
                                if coeff == 0 && exp > 0 {
                                    exp = 0;
                                }
                                assert_eq!(text(value).unwrap(), reference(negative, coeff, exp));
                            } else {
                                assert!(text(value).is_err());
                            }
                        }
                    }
                }
            }
            for exp in [<$d>::MIN_EXP, -31, -30, 0, <$d>::MAX_EXP] {
                let zero = <$d>::from_coefficient_and_exponent(0, exp).unwrap();
                for value in [zero, -zero] {
                    let expected_exp = if (-exp).max(0) > 30 || exp > 0 {
                        0
                    } else {
                        exp
                    };
                    assert_eq!(
                        text(value).unwrap(),
                        reference(value.is_sign_negative(), 0, expected_exp)
                    );
                }
            }
        }};
    }
    check!(Decimal32, i32, 7);
    check!(Decimal64, i64, 16);
    check!(Decimal128, i128, 34);
    for c in [
        u128::from(u64::MAX) - 1,
        u128::from(u64::MAX),
        u128::from(u64::MAX) + 1,
    ] {
        for exp in [-30, -1, 0, 45] {
            for neg in [false, true] {
                let d = Decimal128::from_coefficient_and_exponent(
                    if neg { -(c as i128) } else { c as i128 },
                    exp,
                )
                .unwrap();
                assert_eq!(text(d).unwrap(), reference(neg, c, exp));
            }
        }
    }
}

#[cfg(feature = "diesel-2-mysql")]
#[test]
fn contiguous_mysql_reuses_small_lane_and_fallback_bytes() {
    for coefficient in [
        0_u128,
        1,
        150,
        u128::from(u64::MAX),
        u128::from(u64::MAX) + 1,
        10_u128.pow(34) - 1,
    ] {
        for exponent in [-30, -2, -1, 0, 1, 3, 31] {
            for negative in [false, true] {
                let value = Decimal128::from_coefficient_and_exponent(
                    if negative {
                        -(coefficient as i128)
                    } else {
                        coefficient as i128
                    },
                    exponent,
                )
                .unwrap();
                let value = if coefficient == 0 && negative {
                    -value
                } else {
                    value
                };
                let plan = MysqlText::new(value).unwrap();
                let mut expected = Vec::new();
                plan.write(|part| {
                    expected.extend_from_slice(part);
                    Ok::<(), Error>(())
                })
                .unwrap();
                let mut calls = 0;
                plan.write_contiguous(|bytes| {
                    calls += 1;
                    assert_eq!(bytes, expected);
                    assert!(bytes.len() <= 67);
                    Ok::<(), Error>(())
                })
                .unwrap();
                assert_eq!(calls, 1);
                let result = plan.write_contiguous(|_| Err::<(), _>(42));
                assert!(matches!(result, Err(TextWriteError::Sink(42))));
            }
        }
    }
}
