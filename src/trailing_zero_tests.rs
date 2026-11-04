use core::cmp::Ordering;
use num_bigint::BigUint;
use std::string::ToString;
use std::time::Instant;
use std::vec::Vec;

use crate::refbid::{self, Classified, Finite, Format};
use crate::test_support::{
    Powers, Rng, bounds, canonical_special, classes, decoded, hash, special_bits,
};
use crate::{Decimal32, Decimal64, Decimal128, Error, OrderedDecimal};

// Search powers of ten using arbitrary-precision remainder and quotient,
// independently of the production trailing-zero and division kernels.
fn reference(v: Finite, format: Format, fraction_only: bool, powers: &Powers) -> Finite {
    if v.coeff == 0 {
        return Finite {
            exp: if fraction_only { v.exp.max(0) } else { 0 },
            ..v
        };
    }
    let (precision, _, max_exp) = bounds(format);
    let limit = if fraction_only {
        (-v.exp).max(0)
    } else {
        max_exp - v.exp
    };
    let integer = BigUint::from(v.coeff);
    let zero = BigUint::from(0_u8);
    for removed in (0..=limit.min(precision as i32 - 1)).rev() {
        let divisor = powers.get(i64::from(removed));
        if &integer % divisor == zero {
            return Finite {
                coeff: u128::try_from(&integer / divisor).unwrap(),
                exp: v.exp + removed,
                ..v
            };
        }
    }
    unreachable!("every integer is divisible by 10^0")
}

// The D32 exhaustive oracle uses bounded integer arithmetic. Its largest
// coefficient is 9,999,999 and its divisor at most 1,000,000. Generated D32
// cases also compare this oracle with the arbitrary-precision reference.
fn decimal32_zero_count(mut coeff: u32) -> u32 {
    if coeff == 0 {
        return 0;
    }
    let mut zeros = 0;
    while coeff.is_multiple_of(10) {
        coeff /= 10;
        zeros += 1;
    }
    zeros
}

fn decimal32_reference(v: Finite, fraction_only: bool, zeros: u32) -> Finite {
    if v.coeff == 0 {
        return Finite {
            exp: if fraction_only { v.exp.max(0) } else { 0 },
            ..v
        };
    }
    let limit = if fraction_only {
        (-v.exp).max(0)
    } else {
        90 - v.exp
    };
    let removed = zeros.min(limit as u32);
    Finite {
        coeff: u128::from(v.coeff as u32 / 10_u32.pow(removed)),
        exp: v.exp + removed as i32,
        ..v
    }
}

fn boundary_cohorts(format: Format) -> Vec<Finite> {
    let (precision, min_exp, max_exp) = bounds(format);
    let max_coeff = 10_u128.pow(precision) - 1;
    let mut values = classes(format, format);
    let mut add = |coeff, exponents: &[i32]| {
        for &exp in exponents {
            if coeff <= max_coeff && (min_exp..=max_exp).contains(&exp) {
                for neg in [false, true] {
                    values.push(Finite { neg, coeff, exp });
                }
            }
        }
    };
    // Steering forms at 2^23/2^53, the u64 width boundary, and the D128
    // 113-bit coefficient boundary. Invalid coefficients are not generated.
    for boundary in [1_u128 << 23, 1_u128 << 53, 1_u128 << 64, 1_u128 << 112] {
        for coeff in [boundary - 1, boundary, boundary + 1] {
            add(
                coeff,
                &[min_exp, min_exp + 1, -1, 0, 1, max_exp - 1, max_exp],
            );
        }
    }
    for zeros in 0..precision {
        let divisor = 10_u128.pow(zeros);
        let z = zeros as i32;
        for unit in [1, 7, 11, 23, max_coeff / divisor] {
            if unit * divisor <= max_coeff {
                add(
                    unit * divisor,
                    &[
                        min_exp,
                        min_exp + 1,
                        -z - 1,
                        -z,
                        -z + 1,
                        -1,
                        0,
                        1,
                        max_exp - z - 1,
                        max_exp - z,
                        max_exp - z + 1,
                        max_exp - 1,
                        max_exp,
                    ],
                );
            }
        }
    }
    for exp in min_exp..=max_exp {
        add(0, &[exp]);
    }
    values.sort_unstable_by_key(|v| (v.coeff, v.exp, v.neg));
    values.dedup();
    values
}

macro_rules! width_tests {
    ($module:ident, $decimal:ty, $bits:ty, $format:ident) => {
        mod $module {
            use super::*;

            // This allocation-free part is also the D32 exhaustive check.
            pub(super) fn check_bits(value: $decimal, expected: [u128; 2]) -> [$decimal; 2] {
                let a = value.trim_fraction_zeros();
                let b = value.strip_trailing_zeros();
                let bits = value.to_bits();
                assert_eq!(a.to_bits() as u128, expected[0], "A, bits={bits:x}");
                assert_eq!(b.to_bits() as u128, expected[1], "B, bits={bits:x}");
                assert_eq!(
                    a.trim_fraction_zeros().to_bits(),
                    a.to_bits(),
                    "A(A), bits={bits:x}"
                );
                assert_eq!(
                    b.strip_trailing_zeros().to_bits(),
                    b.to_bits(),
                    "B(B), bits={bits:x}"
                );
                assert_eq!(
                    a.strip_trailing_zeros().to_bits(),
                    b.to_bits(),
                    "B(A), bits={bits:x}"
                );
                assert_eq!(
                    b.trim_fraction_zeros().to_bits(),
                    b.to_bits(),
                    "A(B), bits={bits:x}"
                );
                for result in [a, b] {
                    assert_eq!(
                        result.is_sign_negative(),
                        value.is_sign_negative(),
                        "bits={bits:x}"
                    );
                    if value.is_nan() {
                        assert!(result.is_nan(), "bits={bits:x}");
                    } else {
                        assert_eq!(result, value, "numeric value, bits={bits:x}");
                    }
                }
                [a, b]
            }

            fn check_observations(value: $decimal, results: [$decimal; 2]) {
                let stripped = value.stripped().to_string();
                let key = OrderedDecimal(value);
                for result in results {
                    assert_eq!(result.to_string(), stripped);
                    assert_eq!(OrderedDecimal(result), key);
                    assert_eq!(OrderedDecimal(result).cmp(&key), Ordering::Equal);
                    assert_eq!(hash(OrderedDecimal(result)), hash(key));
                    #[cfg(feature = "serde")]
                    {
                        let json = serde_json::to_string(&result).unwrap();
                        assert_eq!(json, serde_json::to_string(&stripped).unwrap());
                        let parsed: $decimal = serde_json::from_str(&json).unwrap();
                        assert_eq!(OrderedDecimal(parsed), key);
                        if result.is_nan() {
                            // Existing string serialization intentionally loses
                            // NaN sign, payload and signaling state.
                            assert_eq!(parsed.to_bits(), <$decimal>::NAN.to_bits());
                        } else {
                            assert_eq!(parsed, value);
                            assert_eq!(parsed.is_sign_negative(), value.is_sign_negative());
                        }
                    }
                }
            }

            fn check_finite(v: Finite, powers: &Powers) {
                let format = Format::$format;
                let input = refbid::encode(v, format);
                let value = <$decimal>::from_bits(input as $bits).unwrap();
                let expected = [
                    reference(v, format, true, powers),
                    reference(v, format, false, powers),
                ];
                if matches!(format, Format::D32) {
                    let zeros = decimal32_zero_count(v.coeff as u32);
                    assert_eq!(decimal32_reference(v, true, zeros), expected[0]);
                    assert_eq!(decimal32_reference(v, false, zeros), expected[1]);
                }
                let results = check_bits(value, expected.map(|v| refbid::encode(v, format)));
                for (index, result) in results.iter().enumerate() {
                    let actual = decoded(result.to_bits() as u128, format);
                    assert_eq!(actual, expected[index]);
                    assert_eq!(actual.neg, v.neg);
                    if v.coeff != 0 {
                        assert_eq!(
                            BigUint::from(actual.coeff) * powers.get(i64::from(actual.exp - v.exp)),
                            BigUint::from(v.coeff),
                            "exact integer value, input={input:x}"
                        );
                        if index == 0 {
                            assert!(actual.exp >= 0 || !actual.coeff.is_multiple_of(10));
                        } else {
                            assert!(
                                actual.exp == bounds(format).2 || !actual.coeff.is_multiple_of(10)
                            );
                        }
                    }
                }
                check_observations(value, results);
            }

            pub(super) fn check_special(bits: $bits, observations: bool) -> bool {
                let raw = bits as u128;
                let format = Format::$format;
                assert!(matches!(
                    refbid::classify(raw, format),
                    Classified::Infinity | Classified::Nan
                ));
                if !canonical_special(raw, format) {
                    assert_eq!(<$decimal>::from_bits(bits), Err(Error::InvalidInput));
                    return false;
                }
                let value = <$decimal>::from_bits(bits).unwrap();
                let expected = if matches!(refbid::classify(raw, format), Classified::Nan) {
                    raw & !(1_u128 << (<$bits>::BITS - 7))
                } else {
                    raw
                };
                // Exact expected bits check the complete sign and payload,
                // not just NaN equality (which is always false).
                let results = check_bits(value, [expected; 2]);
                if observations {
                    check_observations(value, results);
                }
                true
            }

            #[test]
            fn boundary_cohorts() {
                let powers = Powers::new(34);
                let values = super::boundary_cohorts(Format::$format);
                std::eprintln!(
                    "{} trailing-zero boundary cohorts: {}",
                    stringify!($decimal),
                    values.len()
                );
                for v in values {
                    check_finite(v, &powers);
                }
            }

            #[test]
            fn fixed_seed_ten_thousand() {
                let format = Format::$format;
                let (precision, _, _) = bounds(format);
                let mut rng = Rng(0x7472_6169_6c30_3030 ^ u64::from(precision));
                let powers = Powers::new(34);
                for index in 0..10_000 {
                    let mut v = rng.value(format);
                    if index % 2 == 0 {
                        let zeros = (rng.next() % u64::from(precision)) as u32;
                        v.coeff =
                            (rng.next_u128() % 10_u128.pow(precision - zeros)) * 10_u128.pow(zeros);
                    }
                    check_finite(v, &powers);
                }
            }

            #[test]
            fn special_encodings() {
                let format = Format::$format;
                let (precision, _, _) = bounds(format);
                let (_, _, trailing, _, _) = format.parameters();
                let signaling = 1_u128 << (<$bits>::BITS - 7);
                let payload_limit = 10_u128.pow(precision - 1);
                for neg in [false, true] {
                    let inf = special_bits(format, false, neg, 0);
                    assert!(check_special(inf as $bits, true));
                    // Infinity permits no bits after the combination field.
                    for bit in 0..<$bits>::BITS - 6 {
                        assert!(!check_special((inf | (1_u128 << bit)) as $bits, false));
                    }
                    for quiet_or_signaling in [0, signaling] {
                        for payload in [
                            0,
                            1,
                            7,
                            payload_limit / 10,
                            payload_limit - 1,
                            payload_limit,
                            (1_u128 << trailing) - 1,
                        ] {
                            let nan = special_bits(format, true, neg, payload) | quiet_or_signaling;
                            assert_eq!(check_special(nan as $bits, true), payload < payload_limit);
                        }
                        for bit in trailing..<$bits>::BITS - 7 {
                            let nan = special_bits(format, true, neg, 1)
                                | quiet_or_signaling
                                | (1_u128 << bit);
                            assert!(!check_special(nan as $bits, false));
                        }
                    }
                }
                let powers = Powers::new(34);
                let mut rng = Rng(0x7472_6169_6c62_6974 ^ u64::from(precision));
                for _ in 0..10_000 {
                    let bits = rng.next_u128() as $bits;
                    match refbid::classify(bits as u128, format) {
                        Classified::Finite(v) => check_finite(v, &powers),
                        Classified::Infinity | Classified::Nan => {
                            check_special(bits, true);
                        }
                        Classified::NonCanonical => {
                            assert_eq!(<$decimal>::from_bits(bits), Err(Error::InvalidInput))
                        }
                    }
                }
            }

            #[test]
            fn documented_cohorts_and_limit_stop() {
                let format = Format::$format;
                let max_exp = bounds(format).2;
                let powers = Powers::new(34);
                for neg in [false, true] {
                    for (coeff, exp, a, b) in [
                        (15_000, -4, (15, -1), (15, -1)),
                        (1_500, 0, (1_500, 0), (15, 2)),
                        (0, 5, (0, 5), (0, 0)),
                        (0, -5, (0, 0), (0, 0)),
                        (100, max_exp - 1, (100, max_exp - 1), (10, max_exp)),
                        (100, max_exp, (100, max_exp), (100, max_exp)),
                    ] {
                        let v = Finite { neg, coeff, exp };
                        let value =
                            <$decimal>::from_bits(refbid::encode(v, format) as $bits).unwrap();
                        let expected = [a, b]
                            .map(|(coeff, exp)| refbid::encode(Finite { neg, coeff, exp }, format));
                        check_bits(value, expected);
                        check_finite(v, &powers);
                    }
                }
            }
        }
    };
}

width_tests!(decimal32, Decimal32, u32, D32);
width_tests!(decimal64, Decimal64, u64, D64);
width_tests!(decimal128, Decimal128, u128, D128);

fn decimal32_exhaustive_finite(neg: bool) {
    let started = Instant::now();
    let mut count = 0_u64;
    for coeff in 0..10_000_000_u32 {
        let zeros = decimal32_zero_count(coeff);
        for exp in -101..=90 {
            let v = Finite {
                neg,
                coeff: u128::from(coeff),
                exp,
            };
            let value = Decimal32::from_bits(refbid::encode(v, Format::D32) as u32).unwrap();
            let expected = [true, false].map(|fraction_only| {
                refbid::encode(decimal32_reference(v, fraction_only, zeros), Format::D32)
            });
            decimal32::check_bits(value, expected);
            count += 1;
        }
        if (coeff + 1).is_multiple_of(1_000_000) {
            std::eprintln!(
                "Decimal32 trailing zeros neg={neg}: {count} cohorts, {:?}",
                started.elapsed()
            );
        }
    }
    assert_eq!(count, 1_920_000_000);
}

// Each exhaustive test is single-threaded. Run the positive and negative
// halves with --test-threads=2, never concurrently with timing measurements.
// Display, key/hash and serde are checked by the generated tests above;
// the exhaustive tests establish exact bits, numeric/sign preservation,
// idempotence and both compositions, without allocating strings per cohort.
#[test]
#[ignore = "all 1,920,000,000 nonnegative Decimal32 coefficient/exponent cohorts"]
fn decimal32_exhaustive_trailing_zero_methods_positive() {
    decimal32_exhaustive_finite(false);
}

#[test]
#[ignore = "all 1,920,000,000 negative Decimal32 coefficient/exponent cohorts"]
fn decimal32_exhaustive_trailing_zero_methods_negative() {
    decimal32_exhaustive_finite(true);
}

#[test]
#[ignore = "all 268,435,456 Decimal32 infinity/NaN bit patterns, including rejection"]
fn decimal32_exhaustive_trailing_zero_special_encodings() {
    let started = Instant::now();
    let mut counts = [0_u64; 3];
    for sign in [0, 1_u32 << 31] {
        for magnitude in 0x7800_0000..=0x7fff_ffff {
            let bits = sign | magnitude;
            if decimal32::check_special(bits, false) {
                if bits & 0x7c00_0000 == 0x7c00_0000 {
                    counts[1] += 1;
                } else {
                    counts[0] += 1;
                }
            } else {
                counts[2] += 1;
            }
        }
    }
    assert_eq!(counts, [2, 4_000_000, 264_435_454]);
    std::eprintln!(
        "Decimal32 trailing-zero special patterns: {:?}",
        started.elapsed()
    );
}
