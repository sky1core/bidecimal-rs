use crate::kernel::{
    digits, digits_64, div_rem_pow10, div_rem_pow10_64, strip_trailing_zeros,
    strip_trailing_zeros_64,
};
use crate::test_support::Rng;
use crate::{Error, tables};

const LIMIT: u128 = 1_u128 << 127;

fn check_div_rem(x: u128, k: u32, d: u128) {
    assert!(x < LIMIT);
    assert_eq!(div_rem_pow10(x, k), (x / d, x % d), "x={x}, k={k}");
}

#[test]
fn div_rem_matches_native_at_boundaries_and_multiples() {
    for k in 1..=38 {
        let d = 10_u128.pow(k);
        assert_eq!(tables::pow10(k), d);
        for x in [0, 1, d - 1, d, d + 1, LIMIT - 1, 10_u128.pow(34) - 1] {
            check_div_rem(x, k, d);
        }
        for i in 1..=100_000 {
            if let Some(multiple) = d.checked_mul(i) {
                for x in [
                    multiple.checked_sub(1),
                    Some(multiple),
                    multiple.checked_add(1),
                ]
                .into_iter()
                .flatten()
                {
                    if x < LIMIT {
                        check_div_rem(x, k, d);
                    }
                }
            }
        }
    }
}

#[test]
fn div_rem_matches_native_for_fixed_seed_random_inputs() {
    let mut rng = Rng(0x243f_6a88_85a3_08d3);
    for k in 1..=38 {
        let d = 10_u128.pow(k);
        for _ in 0..1_000_000 {
            check_div_rem(rng.next_u128() >> 1, k, d);
        }
    }
}

#[test]
fn div_rem_handles_zero_and_large_exponents() {
    for x in [0, 1, 10_u128.pow(34) - 1, LIMIT - 1] {
        assert_eq!(div_rem_pow10(x, 0), (x, 0));
        for k in [39, 40, 127, 128, u32::MAX] {
            assert_eq!(div_rem_pow10(x, k), (0, x));
        }
    }
    assert_eq!(tables::pow10(0), 1);
}

fn native_digits(x: u128) -> u32 {
    match x.checked_ilog10() {
        None => 0,
        Some(n) => n + 1,
    }
}

#[test]
fn digits_matches_native_at_every_bit_width_endpoint() {
    for width in 0..=128 {
        let low = if width == 0 { 0 } else { 1_u128 << (width - 1) };
        let high = u128::MAX.unbounded_shr(128 - width);
        let estimate = (width * 1233) >> 12;
        assert!(estimate <= 38);
        for x in [low, high] {
            assert_eq!(x.bit_width(), width);
            let expected = native_digits(x);
            assert!((estimate..=estimate + 1).contains(&expected));
            assert_eq!(digits(x), expected, "x={x}, width={width}");
        }
    }
}

#[test]
fn digits_matches_native_at_every_decimal_boundary() {
    for k in 0..=38 {
        let d = 10_u128.pow(k);
        for x in [d - 1, d, d + 1] {
            assert_eq!(digits(x), native_digits(x), "x={x}");
        }
    }
}

fn naive_strip_trailing_zeros(mut coeff: u128) -> (u128, u32) {
    let mut zeros = 0;
    if coeff != 0 {
        while coeff.is_multiple_of(10) {
            coeff /= 10;
            zeros += 1;
        }
    }
    (coeff, zeros)
}

#[test]
fn strip_trailing_zeros_matches_naive_for_every_power() {
    let mut rng = Rng(0x1319_8a2e_0370_7344);
    for k in 0..=38 {
        for coeff in [
            0,
            1,
            2,
            3,
            5,
            7,
            9,
            10,
            11,
            25,
            100,
            125,
            1200,
            12,
            14,
            16,
            18,
            10_u128.pow(16) - 8,
            10_u128.pow(16) - 6,
            10_u128.pow(16) - 4,
            10_u128.pow(16) - 2,
            10_u128.pow(16) - 1,
            10_u128.pow(16),
            10_u128.pow(16) + 1,
            10_u128.pow(16) + 2,
            10_u128.pow(16) + 4,
            10_u128.pow(16) + 6,
            10_u128.pow(16) + 8,
            123_456_789,
            10_u128.pow(34) - 1,
            LIMIT - 1,
        ] {
            if let Some(x) = coeff.checked_mul(10_u128.pow(k))
                && x < LIMIT
            {
                assert_eq!(
                    strip_trailing_zeros(x),
                    naive_strip_trailing_zeros(x),
                    "coeff={coeff}, k={k}"
                );
            }
        }
        let scale = 10_u128.pow(k);
        let top = (LIMIT - 1) / scale;
        for _ in 0..1000 {
            let x = (1 + rng.next_u128() % top) * scale;
            assert_eq!(
                strip_trailing_zeros(x),
                naive_strip_trailing_zeros(x),
                "x={x}"
            );
        }
    }
}

const LIMIT_64: u64 = 1_u64 << 63;
const STRIP_LIMIT_64: u64 = 10_u64.pow(16);

fn check_div_rem_64(x: u64, k: u32, d: u64) {
    assert!(x < LIMIT_64);
    assert_eq!(div_rem_pow10_64(x, k), (x / d, x % d), "x={x}, k={k}");
}

#[test]
fn div_rem_64_matches_native_at_boundaries_and_multiples() {
    for k in 1..=19 {
        let d = 10_u64.pow(k);
        assert_eq!(tables::pow10_64(k), d);
        for x in [0, 1, d - 1, d, d + 1, LIMIT_64 - 1, 2 * 10_u64.pow(16) - 1]
            .into_iter()
            .filter(|x| *x < LIMIT_64)
        {
            check_div_rem_64(x, k, d);
        }
        for i in 1..=100_000 {
            if let Some(multiple) = d.checked_mul(i) {
                for x in [
                    multiple.checked_sub(1),
                    Some(multiple),
                    multiple.checked_add(1),
                ]
                .into_iter()
                .flatten()
                {
                    if x < LIMIT_64 {
                        check_div_rem_64(x, k, d);
                    }
                }
            }
        }
    }
}

#[test]
fn div_rem_64_matches_native_for_fixed_seed_random_inputs() {
    let mut rng = Rng(0x1319_8a2e_0370_7344);
    for k in 1..=19 {
        let d = 10_u64.pow(k);
        for _ in 0..1_000_000 {
            check_div_rem_64(rng.next() >> 1, k, d);
        }
    }
}

#[test]
fn div_rem_64_handles_zero_and_large_exponents() {
    for x in [0, 1, 2 * 10_u64.pow(16) - 1, LIMIT_64 - 1] {
        assert_eq!(div_rem_pow10_64(x, 0), (x, 0));
        for k in [20, 21, 783, 784, u32::MAX] {
            assert_eq!(div_rem_pow10_64(x, k), (0, x));
        }
    }
    assert_eq!(tables::pow10_64(0), 1);
}

#[test]
fn digits_64_matches_native_at_every_bit_width_endpoint_and_decimal_boundary() {
    for width in 0..=64 {
        let low = if width == 0 { 0 } else { 1_u64 << (width - 1) };
        let high = u64::MAX.unbounded_shr(64 - width);
        let estimate = (width * 1233) >> 12;
        assert!(estimate <= 19);
        for x in [low, high] {
            assert_eq!(x.bit_width(), width);
            let expected = native_digits(u128::from(x));
            assert!((estimate..=estimate + 1).contains(&expected));
            assert_eq!(digits_64(x), expected, "x={x}, width={width}");
        }
    }
    for k in 0..=19 {
        let d = 10_u64.pow(k);
        for x in [d - 1, d, d + 1] {
            assert_eq!(digits_64(x), native_digits(u128::from(x)), "x={x}");
        }
    }
}

#[test]
fn strip_trailing_zeros_64_matches_naive_for_every_count() {
    let mut rng = Rng(0xa409_3822_299f_31d0);
    for zeros in 0..=15 {
        let scale = 10_u64.pow(zeros);
        let top = STRIP_LIMIT_64 / scale;
        let mut units = std::vec![1, 2, 3, 5, 7, 9, 11, 25, 125, 1201, top - 1];
        for _ in 0..1000 {
            units.push(1 + rng.next() % (top - 1));
        }
        units.retain(|unit| *unit < top);
        for unit in units {
            let x = unit * scale;
            assert!(x < STRIP_LIMIT_64);
            assert_eq!(
                strip_trailing_zeros_64(x),
                {
                    let (c, z) = naive_strip_trailing_zeros(u128::from(x));
                    (c as u64, z)
                },
                "unit={unit}, zeros={zeros}"
            );
        }
    }
    assert_eq!(strip_trailing_zeros_64(0), (0, 0));
    assert_eq!(
        strip_trailing_zeros_64(STRIP_LIMIT_64 - 1),
        (STRIP_LIMIT_64 - 1, 0)
    );
}

#[test]
fn errors_display_and_implement_core_error() {
    for (error, message) in [
        (Error::Inexact, "result is not exactly representable"),
        (Error::Overflow, "result exceeds the finite range"),
        (Error::DivisionByZero, "division by zero"),
        (Error::InvalidInput, "invalid input"),
    ] {
        assert_eq!(std::format!("{error}"), message);
        let error: &dyn core::error::Error = &error;
        assert!(error.source().is_none());
    }
}
