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
