use paths::support;

use num_bigint::BigInt;
use support::{Exact, Numeric, numeric_equal};

fn brute_inside(coeff: &BigInt, exp: i32) -> bool {
    (-28..=0).any(|e| {
        let shift = exp - e;
        let ten = BigInt::from(10).pow(shift.unsigned_abs());
        let candidate = if shift >= 0 {
            coeff * ten
        } else {
            if coeff % &ten != BigInt::from(0) {
                return false;
            }
            coeff / ten
        };
        candidate.to_string().trim_start_matches('-').len() <= 28
    })
}

#[test]
fn partition_boundaries_against_definition() {
    for digits in [27, 28, 29] {
        for exp in [-29, -28, 0, 1] {
            for zeros in [0, 1, 2, 28, 29] {
                for sign in [-1, 1] {
                    let coeff: BigInt =
                        (BigInt::from(10).pow(digits) - 1) * BigInt::from(10).pow(zeros) * sign;
                    assert_eq!(
                        Exact::new(coeff.clone(), exp).inside(),
                        brute_inside(&coeff, exp),
                        "coefficient={coeff} exponent={exp}"
                    );
                }
            }
        }
    }
    for exp in [-100, -29, -28, 0, 1, 100] {
        assert!(Exact::new(BigInt::from(0), exp).inside());
    }
    for text in ["-0.00", "+0", "1e-28", "10e-29", "100000e-33"] {
        assert!(Exact::parse(text).inside(), "{text}");
    }
    for text in ["1e-29", "1e28", "99999999999999999999999999999"] {
        assert!(!Exact::parse(text).inside(), "{text}");
    }
}

#[test]
fn partition_random_against_definition() {
    let mut state = 0x243f_6a88_85a3_08d3_u64;
    let mut inside = 0;
    let mut outside = 0;
    for _ in 0..20_000 {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let high = state;
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let coeff: BigInt = (BigInt::from((u128::from(high) << 64) | u128::from(state))
            % BigInt::from(10).pow((high % 40 + 1) as u32))
            * BigInt::from(10).pow((state % 30) as u32)
            * if state & 1 == 0 { 1 } else { -1 };
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let exp = (state % 100) as i32 - 70;
        if Exact::new(coeff.clone(), exp).inside() {
            inside += 1;
        } else {
            outside += 1;
        }
        assert_eq!(
            Exact::new(coeff.clone(), exp).inside(),
            brute_inside(&coeff, exp),
            "coefficient={coeff} exponent={exp}"
        );
    }
    assert!(
        inside > 500 && outside > 500,
        "inside={inside} outside={outside}"
    );
}

#[test]
fn independent_results_and_numeric_preflight() {
    let a = Exact::parse("9999999999999999999999999999");
    assert!(!a.add(&Exact::parse("1")).inside());
    assert!(a.sub(&Exact::parse("1")).inside());
    assert_eq!(
        Exact::parse("-1.53").mul(&Exact::parse("2")),
        Exact::parse("-3.06")
    );
    for (value, floor, ceil) in [("1.53", "1.50", "1.55"), ("-1.53", "-1.55", "-1.50")] {
        let value = Exact::parse(value);
        let step = Exact::parse("0.05");
        assert_eq!(value.rounded_multiple(&step, false), Exact::parse(floor));
        assert_eq!(value.rounded_multiple(&step, true), Exact::parse(ceil));
    }
    let ours = bidecimal::Decimal64::parse("1.00");
    let decimal = "1.0".parse::<rust_decimal::Decimal>().unwrap();
    let base = Some(decimal);
    assert!(numeric_equal(&ours, &base));
    assert!(numeric_equal(&ours, &decimal));
    assert!(!numeric_equal(&ours, &None::<rust_decimal::Decimal>));
    assert!(!numeric_equal(
        &Err::<bidecimal::Decimal64, _>("error"),
        &base
    ));
    assert_eq!(ours.exact(), Some(Exact::parse("1")));
}

#[test]
fn bid_decoder_boundaries_of_every_width() {
    use support::decode_bid;
    for (width, coeff_bits, exp_bits, bias, digits, qmax) in [
        (32, 23, 8, 101, 7, 90),
        (64, 53, 10, 398, 16, 369),
        (128, 113, 14, 6176, 34, 6111),
    ] {
        let max = 10_u128.pow(digits) - 1;
        for negative in [false, true] {
            for exp in [-bias, 0, qmax] {
                for coeff in [
                    0,
                    1,
                    ((1_u128 << coeff_bits) - 1).min(max),
                    1_u128 << coeff_bits,
                    max,
                    max + 1,
                ] {
                    let sign = u128::from(negative) << (width - 1);
                    let bits = if coeff < 1_u128 << coeff_bits {
                        sign | (((exp + bias) as u128) << coeff_bits) | coeff
                    } else {
                        sign | (3_u128 << (width - 3))
                            | (((exp + bias) as u128) << (coeff_bits - 2))
                            | (coeff & ((1_u128 << (coeff_bits - 2)) - 1))
                    };
                    let decoded = decode_bid(bits, width);
                    if coeff > max {
                        assert!(decoded.is_none(), "width={width} bits={bits:x}");
                        continue;
                    }
                    let decoded = decoded.unwrap();
                    assert_eq!(decoded.negative, negative);
                    assert_eq!(decoded.coeff, coeff);
                    assert_eq!(decoded.exp, exp);
                    let expected =
                        Exact::new(BigInt::from(coeff) * if negative { -1 } else { 1 }, exp);
                    assert_eq!(decoded.exact(), expected);
                    let public = match width {
                        32 => bidecimal::Decimal32::from_bits(bits as u32)
                            .unwrap()
                            .exact(),
                        64 => bidecimal::Decimal64::from_bits(bits as u64)
                            .unwrap()
                            .exact(),
                        128 => bidecimal::Decimal128::from_bits(bits).unwrap().exact(),
                        _ => unreachable!(),
                    };
                    assert_eq!(public, Some(expected));
                }
            }
            for combination in [30_u128, 31] {
                assert!(
                    decode_bid(
                        (u128::from(negative) << (width - 1)) | (combination << (width - 6)),
                        width
                    )
                    .is_none()
                );
            }
            let invalid_large_form = (u128::from(negative) << (width - 1))
                | (3_u128 << (width - 3))
                | (((1_u128 << exp_bits) - 1) << (coeff_bits - 2));
            assert!(decode_bid(invalid_large_form, width).is_none());
        }
    }
    assert!(decode_bid(1_u128 << 32, 32).is_none());
    assert!(decode_bid(0, 16).is_none());
}

#[test]
fn wrong_inside_bidecimal_result_fails_before_baseline_comparison() {
    use std::cell::Cell;
    let called = Cell::new(false);
    let ours = bidecimal::Decimal64::parse("2").unwrap();
    let expected = Exact::parse("1");
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        support::preflight(
            "inside",
            &ours,
            || {
                called.set(true);
                rust_decimal::Decimal::ONE
            },
            |v| v.exact() == Some(expected.clone()),
            numeric_equal,
        )
    }));
    assert!(failed.is_err());
    assert!(!called.get());
    let ours = bidecimal::Decimal64::parse("1").unwrap();
    assert!(!support::preflight(
        "baseline differs",
        &ours,
        || rust_decimal::Decimal::TWO,
        |v| v.exact() == Some(expected),
        numeric_equal,
    ));
}

#[allow(dead_code)]
#[path = "../benches/paths.rs"]
mod paths;

#[test]
fn workload_covers_digit_lengths_and_numeric_cohorts() {
    paths::check_workload_coverage();
}

#[test]
fn binary_oracle_rounding_cohorts_and_range() {
    use support::{Bid, BinaryDecimal, binary_decimal, verify_binary_decimal};
    for (input, width, coeff, exp) in [
        (0.1_f64, 64, 1, -1),
        (1.5, 128, 15, -1),
        (123456650.0, 32, 1234566, 2),
        (123456750.0, 32, 1234568, 2),
        (999999950.0, 32, 1000000, 3),
        (2_f64.powi(80), 64, 120892581961463, 10),
        (1e96, 32, 1000000, 90),
        (1e-101, 32, 1, -101),
        (f64::from_bits(1), 32, 0, 0),
        (f64::from_bits(1), 64, 494065645841247, -338),
        (42.0, 128, 42, 0),
        (0.0, 32, 0, 0),
    ] {
        for sign in [1.0_f64, -1.0] {
            let input = input * sign;
            let expected = BinaryDecimal::Finite(Bid {
                negative: sign.is_sign_negative(),
                coeff,
                exp,
            });
            assert_eq!(binary_decimal(input.to_bits(), 64, width), expected);
            let bits = match width {
                32 => bidecimal::Decimal32::from_f64(input).to_bits() as u128,
                64 => bidecimal::Decimal64::from_f64(input).to_bits() as u128,
                128 => bidecimal::Decimal128::from_f64(input).to_bits(),
                _ => unreachable!(),
            };
            verify_binary_decimal(bits, width, &expected);
        }
    }
    for (input, coeff, exp) in [
        (1234565_f32, 123456, 1),
        (1234575_f32, 123458, 1),
        (f32::from_bits(1), 14013, -49),
    ] {
        for width in [32, 64, 128] {
            assert_eq!(
                binary_decimal(input.to_bits() as u64, 32, width),
                BinaryDecimal::Finite(Bid {
                    negative: false,
                    coeff,
                    exp
                })
            );
        }
    }
    for input in [1e97_f64, f64::INFINITY, -1e97, f64::NEG_INFINITY] {
        let expected = BinaryDecimal::Infinity {
            negative: input.is_sign_negative(),
        };
        assert_eq!(binary_decimal(input.to_bits(), 64, 32), expected);
        verify_binary_decimal(
            bidecimal::Decimal32::from_f64(input).to_bits() as u128,
            32,
            &expected,
        );
    }
}

#[test]
fn float_fixtures_are_exact_and_binary_rounding_is_direct() {
    macro_rules! width {
        ($ty:ty, $width:literal) => {
            for (fixture, text) in paths::float_inputs($width) {
                let text = text.as_str();
                let value = if fixture == "huge" {
                    <$ty>::from_coefficient_and_exponent(1, 28).unwrap()
                } else {
                    <$ty>::parse(text).unwrap()
                };
                assert_eq!(value.exact(), Some(Exact::parse(text)));
                assert_eq!(value.to_f64().to_bits(), support::decimal_binary(text, 64));
                assert_eq!(
                    value.to_f32().to_bits(),
                    support::decimal_binary(text, 32) as u32
                );
            }
        };
    }
    width!(bidecimal::Decimal32, 32);
    width!(bidecimal::Decimal64, 64);
    width!(bidecimal::Decimal128, 128);
    let text = "1.000000059604644775390625000000001";
    assert_eq!(support::decimal_binary(text, 32), 0x3f800001);
    assert_eq!(
        (f64::from_bits(support::decimal_binary(text, 64)) as f32).to_bits(),
        0x3f800000
    );
    assert!(!support::rust_decimal_exact(
        None,
        &support::binary_decimal(0.1_f64.to_bits(), 64, 64)
    ));
}

#[test]
fn decimal_binary_oracle_midpoints_neighbors_and_range() {
    for (width, fraction, bias, infinity) in [
        (32, 23, 127, 0x7f800000_u64),
        (64, 52, 1023, 0x7ff0000000000000),
    ] {
        let one = (bias as u64) << fraction;
        let min_power = 1 - bias - fraction;
        for (numerator, power, lower, upper) in [
            (1, min_power - 1, 0, 1),
            (3, min_power - 1, 1, 2),
            (
                (1_u64 << (fraction + 1)) - 1,
                min_power - 1,
                (1 << fraction) - 1,
                1 << fraction,
            ),
            ((1 << (fraction + 1)) + 1, -fraction - 1, one, one + 1),
            ((1 << (fraction + 1)) + 3, -fraction - 1, one + 1, one + 2),
            (
                (1 << (fraction + 2)) - 1,
                bias - fraction - 1,
                infinity - 1,
                infinity,
            ),
        ] {
            let (coeff, exp) = if power < 0 {
                (
                    BigInt::from(numerator) * BigInt::from(5).pow((-power) as u32),
                    power,
                )
            } else {
                (BigInt::from(numerator) << power as usize, 0)
            };
            for (delta, expected) in [
                (-1, lower),
                (0, if lower & 1 == 0 { lower } else { upper }),
                (1, upper),
            ] {
                for negative in [false, true] {
                    let text = format!(
                        "{}{}e{exp}",
                        if negative { "-" } else { "" },
                        &coeff + delta
                    );
                    assert_eq!(
                        support::decimal_binary(&text, width),
                        expected | (u64::from(negative) << (width - 1)),
                        "{text} width={width}"
                    );
                }
            }
        }
        for text in ["0", "-0", "0e-6176", "-0e6111"] {
            assert_eq!(
                support::decimal_binary(text, width),
                u64::from(text.starts_with('-')) << (width - 1)
            );
        }
        for (text, bits) in [("1e-6176", 0), ("1e6144", infinity), ("1", one)] {
            assert_eq!(support::decimal_binary(text, width), bits);
        }
    }
}

#[test]
fn decimal_to_binary_product_zero_subnormal_and_overflow_boundaries() {
    for (width, coefficient, exponent, expected) in [
        (32, 0, 0, 0_u64),
        (64, 0, 0, 0),
        (32, 700649232162408535461864791644958, -78, 0),
        (32, 700649232162408535461864791644959, -78, 1),
        (32, 1401298464324817070923729583289916, -78, 1),
        (32, 3402823567797336616375393954581425, 5, 0x7f7fffff),
        (32, 3402823567797336616375393954581426, 5, 0x7f800000),
        (64, 2470328229206232720882843964341106, -357, 0),
        (64, 2470328229206232720882843964341107, -357, 1),
        (64, 4940656458412465441765687928682214, -357, 1),
        (64, 1797693134862315807937289714053034, 275, 0x7fefffffffffffff),
        (64, 1797693134862315807937289714053035, 275, 0x7ff0000000000000),
    ] {
        for negative in [false, true] {
            let text = format!("{}{coefficient}e{exponent}", if negative { "-" } else { "" });
            let expected = expected | (u64::from(negative) << (width - 1));
            assert_eq!(support::decimal_binary(&text, width), expected);
            let value = bidecimal::Decimal128::from_coefficient_and_exponent(coefficient, exponent).unwrap();
            let value = if negative { -value } else { value };
            let actual = if width == 32 {
                value.to_f32().to_bits() as u64
            } else {
                value.to_f64().to_bits()
            };
            assert_eq!(actual, expected, "{text} width={width}");
        }
    }
}
