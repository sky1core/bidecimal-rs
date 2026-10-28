use num_bigint::{BigInt, Sign};
use std::time::Instant;
use std::vec::Vec;

use crate::refbid::{self, Finite, Format};
use crate::test_support::*;
use crate::{Error, Rounding};

const DIRECTIONS: [Rounding; 2] = [Rounding::TowardNegative, Rounding::TowardPositive];
const GENERAL_UNITS: [u128; 8] = [2, 3, 7, 25, 125, 999, 1024, 4375];

fn powers() -> Powers {
    Powers::new(13200)
}

fn expected(
    x: Finite,
    step: Finite,
    format: Format,
    dir: Rounding,
    powers: &Powers,
) -> Result<Finite, Error> {
    if step.coeff == 0 {
        return Err(Error::DivisionByZero);
    }
    if step.neg {
        return Err(Error::InvalidInput);
    }
    let base = x.exp.min(step.exp);
    let numerator = integer_at(x, base, powers);
    let denominator = integer_at(step, base, powers);
    let mut quotient = &numerator / &denominator;
    let remainder = &numerator % &denominator;
    if remainder.sign() != Sign::NoSign {
        match dir {
            Rounding::TowardNegative if x.neg => quotient -= 1_u8,
            Rounding::TowardPositive if !x.neg => quotient += 1_u8,
            _ => {}
        }
    }
    round_integer(
        quotient * BigInt::from(step.coeff),
        step.exp,
        x.neg,
        format,
        None,
        powers,
    )
}

fn actual(x: Finite, step: Finite, format: Format, dir: Rounding) -> Result<Finite, Error> {
    match format {
        Format::D32 => d32(x)
            .round_to_multiple(d32(step), dir)
            .map(|v| decoded(u128::from(v.to_bits()), format)),
        Format::D64 => d64(x)
            .round_to_multiple(d64(step), dir)
            .map(|v| decoded(u128::from(v.to_bits()), format)),
        Format::D128 => d128(x)
            .round_to_multiple(d128(step), dir)
            .map(|v| decoded(v.to_bits(), format)),
    }
}

fn check(x: Finite, step: Finite, format: Format, powers: &Powers) {
    let x = decoded(refbid::encode(x, format), format);
    let step = decoded(refbid::encode(step, format), format);
    for dir in DIRECTIONS {
        assert_eq!(
            actual(x, step, format, dir),
            expected(x, step, format, dir, powers),
            "x={x:?}, step={step:?}, precision={}, dir={dir:?}",
            bounds(format).0
        );
    }
}

fn step_exponents(format: Format) -> Vec<i32> {
    let (p, lo, hi) = bounds(format);
    let mut exps: Vec<i32> = if matches!(format, Format::D128) {
        let mut exps: Vec<i32> = (-(p as i32) - 2..=p as i32 + 2).collect();
        exps.extend((lo..=lo + 3).chain(hi - 3..=hi));
        exps.extend((lo..=hi).step_by(997));
        exps
    } else {
        (lo..=hi).collect()
    };
    exps.sort_unstable();
    exps.dedup();
    exps
}

fn steps(format: Format) -> Vec<Finite> {
    let (p, lo, hi) = bounds(format);
    let max = 10_u128.pow(p) - 1;
    let mut result = Vec::new();
    for exp in step_exponents(format) {
        for coeff in [1, 5, 50, 100] {
            result.push(value(coeff, exp));
        }
    }
    for zeros in 1..p {
        for unit in [1, 5] {
            result.push(value(unit * 10_u128.pow(zeros), hi));
        }
    }
    for exp in [lo, -1, 0, 3, hi] {
        for coeff in GENERAL_UNITS.into_iter().chain([max / 3, max - 1, max]) {
            result.push(value(coeff, exp));
        }
    }
    result
}

#[test]
fn multiple_coefficient_exponent_sign_and_step_cohort_classes() {
    let powers = powers();
    for format in FORMATS {
        let stride = if matches!(format, Format::D128) {
            13
        } else {
            1
        };
        let xs: Vec<Finite> = classes(format, format)
            .into_iter()
            .step_by(stride)
            .collect();
        let mut steps = steps(format);
        if matches!(format, Format::D128) {
            steps = steps.into_iter().step_by(5).collect();
        }
        for x in &xs {
            for &step in &steps {
                check(*x, step, format, &powers);
            }
        }
    }
}

#[test]
fn multiple_exact_multiples_carry_overflow_and_signed_zero() {
    let powers = powers();
    for format in FORMATS {
        let (p, lo, hi) = bounds(format);
        let max = 10_u128.pow(p) - 1;
        for neg in [false, true] {
            for x in [
                value(0, lo),
                value(0, 0),
                value(0, hi),
                value(1, -2),
                value(153, -2),
                value(150, -2),
                value(1000, 3),
                value(max, 0),
                value(max, hi),
            ] {
                for step in [
                    value(1, 0),
                    value(5, -2),
                    value(50, -3),
                    value(100, -2),
                    value(5, 0),
                    value(1, 1),
                    value(5, hi),
                    value(3, -2),
                    value(7, 0),
                    value(25, -3),
                    value(max, hi),
                ] {
                    check(Finite { neg, ..x }, step, format, &powers);
                }
            }
        }
        let x = Finite {
            neg: true,
            coeff: 1,
            exp: -2,
        };
        assert_eq!(
            actual(x, value(1, 0), format, Rounding::TowardPositive),
            Ok(Finite {
                neg: true,
                coeff: 0,
                exp: 0,
            })
        );
        for neg in [false, true] {
            let x = Finite {
                neg,
                coeff: max,
                exp: hi,
            };
            let dir = if neg {
                Rounding::TowardNegative
            } else {
                Rounding::TowardPositive
            };
            assert_eq!(actual(x, value(5, hi), format, dir), Err(Error::Overflow));
            assert_eq!(actual(x, value(7, hi), format, dir), Err(Error::Overflow));
            let x = Finite { exp: 0, ..x };
            let result = actual(x, value(5, 0), format, dir).unwrap();
            assert_eq!(result.neg, neg);
            assert_eq!(result.coeff, 10_u128.pow(p - 1));
            assert_eq!(result.exp, 1);
        }
    }
}

#[test]
fn multiple_zero_and_negative_steps_are_rejected_in_order() {
    let powers = powers();
    for format in FORMATS {
        let (p, lo, hi) = bounds(format);
        for exp in [lo, -1, 0, 1, hi] {
            for neg in [false, true] {
                for coeff in [0, 1, 3, 5, 50, 100] {
                    let step = Finite { neg, coeff, exp };
                    if neg || coeff == 0 {
                        for x in [value(0, lo), value(153, -2), value(10_u128.pow(p) - 1, hi)] {
                            check(x, step, format, &powers);
                            for dir in DIRECTIONS {
                                assert_eq!(
                                    actual(x, step, format, dir),
                                    Err(if coeff == 0 {
                                        Error::DivisionByZero
                                    } else {
                                        Error::InvalidInput
                                    })
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn multiple_general_steps_every_scale_gap_and_unrepresentable_multiples() {
    let powers = powers();
    for format in FORMATS {
        let (p, lo, hi) = bounds(format);
        let max = 10_u128.pow(p) - 1;
        let mut gaps: Vec<i32> = (-40..=75).collect();
        gaps.extend([110, 111, 112, 113, 114, 200, 767, 12286, 12287]);
        for gap in gaps {
            for step_coeff in GENERAL_UNITS.into_iter().chain([max / 7, max - 1, max]) {
                if step_coeff > max {
                    continue;
                }
                for step_exp in [lo, -(p as i32), 0, hi - gap.max(0)] {
                    let x_exp = i64::from(step_exp) + i64::from(gap);
                    if !(i64::from(lo)..=i64::from(hi)).contains(&x_exp)
                        || !(lo..=hi).contains(&step_exp)
                    {
                        continue;
                    }
                    for x_coeff in [
                        1,
                        2,
                        step_coeff,
                        step_coeff * (max / step_coeff),
                        max / 2,
                        max - 1,
                        max,
                    ] {
                        for neg in [false, true] {
                            check(
                                Finite {
                                    neg,
                                    coeff: x_coeff,
                                    exp: x_exp as i32,
                                },
                                value(step_coeff, step_exp),
                                format,
                                &powers,
                            );
                        }
                    }
                }
            }
        }
        assert_eq!(
            actual(value(1, 10), value(3, 0), format, Rounding::TowardNegative),
            if p < 10 {
                Err(Error::Inexact)
            } else {
                Ok(value(9_999_999_999, 0))
            }
        );
    }
    let x = value(7, 6000);
    for (step, expected) in [
        (value(7168, -6000), Ok(value(7 * 10_u128.pow(33), 5967))),
        (value(1 << 100, -6000), Ok(value(7 * 10_u128.pow(33), 5967))),
        (value(3, -6000), Err(Error::Inexact)),
    ] {
        assert_eq!(
            actual(x, step, Format::D128, Rounding::TowardNegative),
            expected
        );
        check(x, step, Format::D128, &powers);
    }
}

#[test]
fn multiple_carry_below_and_at_range_end_and_precision_limited_exponents() {
    let powers = powers();
    for format in FORMATS {
        let (p, lo, hi) = bounds(format);
        let max = 10_u128.pow(p) - 1;
        for neg in [false, true] {
            let dir = if neg {
                Rounding::TowardNegative
            } else {
                Rounding::TowardPositive
            };
            for (x_exp, step_exp, expected) in [
                (hi, hi, Err(Error::Overflow)),
                (
                    hi - 1,
                    hi - 1,
                    Ok(Finite {
                        neg,
                        coeff: 10_u128.pow(p - 1),
                        exp: hi,
                    }),
                ),
            ] {
                let x = Finite {
                    neg,
                    coeff: max,
                    exp: x_exp,
                };
                assert_eq!(
                    actual(x, value(5, step_exp), format, dir),
                    expected,
                    "x={x:?}"
                );
                check(x, value(5, step_exp), format, &powers);
            }
        }
        for coeff in [1, 7, 123, 10_u128.pow(p - 1), max] {
            for shift in [1, 2, p as i32 - 1, p as i32, p as i32 + 1, 40] {
                for step_exp in [lo, 0, hi - shift] {
                    let x_exp = step_exp + shift;
                    if !(lo..=hi).contains(&x_exp) || !(lo..=hi).contains(&step_exp) {
                        continue;
                    }
                    for unit in [1, 5, 3, 7] {
                        for neg in [false, true] {
                            check(
                                Finite {
                                    neg,
                                    coeff,
                                    exp: x_exp,
                                },
                                value(unit, step_exp),
                                format,
                                &powers,
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn multiple_division_exponent_boundaries_every_zero_count_and_both_units() {
    let powers = powers();
    for format in FORMATS {
        let (p, lo, hi) = bounds(format);
        let max = 10_u128.pow(p) - 1;
        let longest = i64::from(hi) - i64::from(lo) + i64::from(p) - 1 + 1;
        let mut gaps = std::vec![1, 2, i64::from(p) - 1, i64::from(p), i64::from(p) + 1];
        gaps.extend([
            16,
            17,
            18,
            19,
            20,
            21,
            34,
            35,
            36,
            37,
            38,
            39,
            767,
            longest - 1,
            longest,
        ]);
        gaps.retain(|g| (1..=longest).contains(g));
        gaps.sort_unstable();
        gaps.dedup();
        let mut exercised = std::collections::BTreeSet::new();
        for &gap in &gaps {
            for unit in [1_u128, 5] {
                for zeros in 0..p {
                    let five = i64::from(unit == 5);
                    for step_exp in [lo, -(zeros as i32), hi - zeros as i32, hi] {
                        let stripped_exp = i64::from(step_exp) + i64::from(zeros);
                        let x_exp = stripped_exp + five - gap;
                        if !(i64::from(lo)..=i64::from(hi)).contains(&x_exp)
                            || !(lo..=hi).contains(&step_exp)
                        {
                            continue;
                        }
                        exercised.insert(gap);
                        let step = value(unit * 10_u128.pow(zeros), step_exp);
                        for coeff in [0, 1, 4, 5, 6, 9, 10, 11, 49, 50, 51, max / 2, max - 1, max] {
                            for neg in [false, true] {
                                let x = Finite {
                                    neg,
                                    coeff,
                                    exp: x_exp as i32,
                                };
                                check(x, step, format, &powers);
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(exercised, gaps.into_iter().collect());
    }
}

#[test]
fn multiple_zero_values_of_both_signs_keep_sign_at_step_exponent() {
    let powers = powers();
    for format in FORMATS {
        let (_, lo, hi) = bounds(format);
        for step in steps(format) {
            for exp in [lo, lo + 1, -1, 0, 1, hi - 1, hi] {
                for neg in [false, true] {
                    let x = Finite { neg, coeff: 0, exp };
                    check(x, step, format, &powers);
                    for dir in DIRECTIONS {
                        assert_eq!(
                            actual(x, step, format, dir),
                            Ok(Finite {
                                neg,
                                coeff: 0,
                                exp: step.exp,
                            }),
                            "x={x:?}, step={step:?}"
                        );
                    }
                }
            }
        }
    }
}

fn random_multiple(count: u64, powers: &Powers) {
    for (index, format) in FORMATS.into_iter().enumerate() {
        let (p, lo, hi) = bounds(format);
        let mut rng = Rng(0x57d3_04c9_e12f_a86b ^ index as u64);
        for _ in 0..count {
            let x = rng.value(format);
            let mut step = rng.value(format);
            step.neg = false;
            match rng.next() % 3 {
                0 => {
                    let zeros = (rng.next() % u64::from(p)) as u32;
                    let unit = if rng.next() & 1 == 0 { 1 } else { 5 };
                    step.coeff = unit * 10_u128.pow(zeros);
                }
                1 => {
                    let offset = (rng.next() % 81) as i32 - 40;
                    step.exp = (x.exp - offset).clamp(lo, hi);
                }
                _ => {}
            }
            if step.coeff == 0 {
                step.coeff = 1;
            }
            check(x, step, format, powers);
        }
    }
}

#[test]
fn multiple_all_widths_bigint_random() {
    random_multiple(2000, &powers());
}

#[test]
#[ignore = "one million value/step pairs per width, both directions"]
fn multiple_all_widths_bigint_million() {
    let started = Instant::now();
    let powers = powers();
    random_multiple(1_000_000, &powers);
    std::eprintln!(
        "round_to_multiple 3 million pairs, 6 million directed checks: {:?}",
        started.elapsed()
    );
}
