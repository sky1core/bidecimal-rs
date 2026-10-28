use num_bigint::BigUint;
use std::time::Instant;

use crate::refbid::{self, Classified, Finite, Format};
use crate::test_support::*;
use crate::{Decimal32, Error, Rounding};

const DIRECTIONS: [Rounding; 2] = [Rounding::TowardNegative, Rounding::TowardPositive];
const EXHAUSTIVE_TARGETS: [i32; 15] = [
    -101, -12, -11, -10, -9, -8, -7, -6, -5, -4, -3, -2, -1, 0, 90,
];

fn reference(
    v: Finite,
    format: Format,
    exp: i32,
    dir: Rounding,
    powers: &Powers,
) -> Result<Finite, Error> {
    let (p, lo, hi) = bounds(format);
    if !(lo..=hi).contains(&exp) {
        return Err(Error::InvalidInput);
    }
    let c = BigUint::from(v.coeff);
    let (num, den) = if v.exp >= exp {
        // Any nonzero coefficient times 10^P is outside this format.
        if v.coeff != 0 && v.exp - exp >= p as i32 {
            return Err(Error::InvalidInput);
        }
        (c * powers.get(i64::from(v.exp - exp)), BigUint::from(1_u8))
    } else {
        (c, powers.get(i64::from(exp - v.exp)).clone())
    };
    let mut integer = &num / &den;
    if &num % &den != BigUint::from(0_u8)
        && matches!(
            (v.neg, dir),
            (true, Rounding::TowardNegative) | (false, Rounding::TowardPositive)
        )
    {
        integer += 1_u8;
    }
    if integer >= *powers.get(i64::from(p)) {
        return Err(Error::InvalidInput);
    }
    Ok(Finite {
        neg: v.neg,
        coeff: u128::try_from(integer).unwrap(),
        exp,
    })
}

fn actual(v: Finite, format: Format, exp: i32, dir: Rounding) -> Result<Finite, Error> {
    match format {
        Format::D32 => d32(v).quantize(exp, dir).map(|v| u128::from(v.to_bits())),
        Format::D64 => d64(v).quantize(exp, dir).map(|v| u128::from(v.to_bits())),
        Format::D128 => d128(v).quantize(exp, dir).map(|v| v.to_bits()),
    }
    .map(|bits| decoded(bits, format))
}

fn check(v: Finite, format: Format, exp: i32, powers: &Powers) {
    let v = decoded(refbid::encode(v, format), format);
    for dir in DIRECTIONS {
        assert_eq!(
            actual(v, format, exp, dir),
            reference(v, format, exp, dir, powers),
            "v={v:?}, P={}, q={exp}, dir={dir:?}",
            bounds(format).0
        );
    }
}

#[test]
fn quantize_all_width_coefficient_exponent_target_sign_classes() {
    let powers = Powers::new(12287);
    for format in FORMATS {
        let (p, lo, hi) = bounds(format);
        let target_classes: std::vec::Vec<_> = classes(format, format)
            .into_iter()
            .filter(|c| c.coeff == 1 && !c.neg)
            .map(|c| c.exp)
            .collect();
        for v in classes(format, format) {
            match format {
                Format::D32 | Format::D64 => {
                    for q in lo..=hi {
                        check(v, format, q, &powers);
                    }
                }
                Format::D128 => {
                    let mut targets = target_classes.clone();
                    for delta in -(p as i32) - 1..=p as i32 + 1 {
                        targets.push(v.exp + delta);
                    }
                    targets.retain(|q| (lo..=hi).contains(q));
                    targets.sort_unstable();
                    targets.dedup();
                    for q in targets {
                        check(v, format, q, &powers);
                    }
                }
            }
        }
        if matches!(format, Format::D128) {
            for coeff in [0, 1, 10_u128.pow(p - 1), 10_u128.pow(p) - 1] {
                for exp in [lo, lo + 1, -1, 0, 1, hi - 1, hi] {
                    for neg in [false, true] {
                        for q in lo..=hi {
                            check(Finite { neg, coeff, exp }, format, q, &powers);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn quantize_zero_all_targets_invalid_targets_and_precision_edges() {
    let powers = Powers::new(12287);
    for format in FORMATS {
        let (p, lo, hi) = bounds(format);
        for neg in [false, true] {
            for exp in [lo, 0, hi] {
                for q in lo..=hi {
                    check(Finite { neg, coeff: 0, exp }, format, q, &powers);
                }
            }
            for coeff in coefficients(p) {
                for exp in [lo, 0, hi] {
                    for q in [lo - 1, hi + 1, i32::MIN, i32::MAX] {
                        check(Finite { neg, coeff, exp }, format, q, &powers);
                    }
                }
                for q in -(p as i32) - 1..=1 {
                    check(Finite { neg, coeff, exp: 0 }, format, q, &powers);
                }
            }
            for q in [1, p as i32, p as i32 + 1, 38, 39, hi] {
                check(
                    Finite {
                        neg,
                        coeff: 10_u128.pow(p) - 1,
                        exp: 0,
                    },
                    format,
                    q,
                    &powers,
                );
            }
        }
    }
}

// This reference uses only native u64 arithmetic. A decoded Decimal32 has
// c < 10^7. Powers are formed only for gaps <= 6, products < 10^13 < 2^64;
// every larger scale-up is rejected, every larger divisor gives (0,c).
fn native_reference(v: Finite, exp: i32, dir: Rounding) -> Result<Finite, Error> {
    assert!(v.coeff < 10_000_000 && (-101..=90).contains(&v.exp));
    if !(-101..=90).contains(&exp) {
        return Err(Error::InvalidInput);
    }
    let c = v.coeff as u64;
    let coefficient = if c == 0 {
        0
    } else if exp <= v.exp {
        let gap = (v.exp - exp) as u32;
        if gap >= 7 {
            return Err(Error::InvalidInput);
        }
        let product = c.checked_mul(10_u64.checked_pow(gap).unwrap()).unwrap();
        if product >= 10_000_000 {
            return Err(Error::InvalidInput);
        }
        product
    } else {
        let gap = (exp - v.exp) as u32;
        let (q, r) = if gap >= 7 {
            (0, c)
        } else {
            let divisor = 10_u64.checked_pow(gap).unwrap();
            (c / divisor, c % divisor)
        };
        q.checked_add(u64::from(
            r != 0
                && matches!(
                    (v.neg, dir),
                    (true, Rounding::TowardNegative) | (false, Rounding::TowardPositive)
                ),
        ))
        .unwrap()
    };
    Ok(Finite {
        neg: v.neg,
        coeff: u128::from(coefficient),
        exp,
    })
}

fn native_matches_bigint() {
    let powers = Powers::new(191);
    for v in classes(Format::D32, Format::D32) {
        let v = decoded(refbid::encode(v, Format::D32), Format::D32);
        for q in EXHAUSTIVE_TARGETS {
            for dir in DIRECTIONS {
                assert_eq!(
                    native_reference(v, q, dir),
                    reference(v, Format::D32, q, dir, &powers)
                );
            }
        }
    }
}

#[test]
fn quantize_decimal32_native_reference_matches_bigint_all_classes() {
    native_matches_bigint();
}

#[test]
#[ignore = "all 2^32 patterns, fifteen target exponents, both directions"]
fn quantize_decimal32_exhaustive_reference() {
    let start = Instant::now();
    native_matches_bigint();
    for bits in 0..1_u64 << 32 {
        match refbid::classify(u128::from(bits), Format::D32) {
            Classified::Finite(v) => {
                let x = Decimal32::from_bits(bits as u32).unwrap();
                for q in EXHAUSTIVE_TARGETS {
                    for dir in DIRECTIONS {
                        let expected = native_reference(v, q, dir)
                            .map(|v| refbid::encode(v, Format::D32) as u32);
                        assert_eq!(
                            x.quantize(q, dir).map(|v| v.to_bits()),
                            expected,
                            "bits={bits:x}, q={q}, dir={dir:?}"
                        );
                    }
                }
            }
            special @ (Classified::Infinity | Classified::Nan)
                if canonical_special(u128::from(bits), Format::D32) =>
            {
                let x = Decimal32::from_bits(bits as u32).unwrap();
                let expected = if matches!(special, Classified::Nan) {
                    special_bits(
                        Format::D32,
                        true,
                        bits >> 31 == 1,
                        u128::from(bits) & ((1 << 20) - 1),
                    )
                } else {
                    special_bits(Format::D32, true, false, 0)
                };
                for q in EXHAUSTIVE_TARGETS {
                    for dir in DIRECTIONS {
                        assert_eq!(
                            x.quantize(q, dir).map(|v| u128::from(v.to_bits())),
                            Ok(expected),
                            "bits={bits:x}, q={q}, dir={dir:?}"
                        );
                    }
                }
            }
            _ => assert_eq!(
                Decimal32::from_bits(bits as u32).err(),
                Some(Error::InvalidInput)
            ),
        }
    }
    std::eprintln!(
        "Decimal32 quantize 2^32 patterns x 15 targets x 2 directions: {:?}",
        start.elapsed()
    );
}

fn random_quantize(count: u64, powers: &Powers) {
    for (i, format) in FORMATS.into_iter().enumerate() {
        let (_, lo, hi) = bounds(format);
        let mut rng = Rng(0x8f03_42a1_58eb_d923 ^ i as u64);
        for _ in 0..count {
            let v = rng.value(format);
            let q = lo + (rng.next() % (hi - lo + 1) as u64) as i32;
            check(v, format, q, powers);
        }
    }
}

#[test]
fn quantize_all_widths_bigint_random() {
    random_quantize(2000, &Powers::new(12287));
}

#[test]
#[ignore = "one million values per width, both directions"]
fn quantize_all_widths_bigint_million() {
    let started = Instant::now();
    let powers = Powers::new(12287);
    random_quantize(1_000_000, &powers);
    std::eprintln!(
        "quantize 3 million values, both directions: {:?}",
        started.elapsed()
    );
}
