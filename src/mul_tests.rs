use num_bigint::{BigInt, BigUint, Sign};
use std::time::Instant;
use std::vec::Vec;

use crate::refbid::{self, Finite, Format};
use crate::test_support::*;
use crate::{Decimal32, Decimal64, Decimal128, Error, Operand, Rounding};

fn product(
    a: impl Operand,
    b: impl Operand,
    rf: Format,
    mode: Option<Rounding>,
) -> Result<Value, Error> {
    match (rf, mode) {
        (Format::D32, None) => Decimal32::mul(a, b).map(|v| u128::from(v.to_bits())),
        (Format::D32, Some(dir)) => {
            Decimal32::mul_rounded(a, b, dir).map(|v| u128::from(v.to_bits()))
        }
        (Format::D64, None) => Decimal64::mul(a, b).map(|v| u128::from(v.to_bits())),
        (Format::D64, Some(dir)) => {
            Decimal64::mul_rounded(a, b, dir).map(|v| u128::from(v.to_bits()))
        }
        (Format::D128, None) => Decimal128::mul(a, b).map(|v| v.to_bits()),
        (Format::D128, Some(dir)) => Decimal128::mul_rounded(a, b, dir).map(|v| v.to_bits()),
    }
    .map(|bits| classified(bits, rf))
}

fn actual(
    a: Finite,
    af: Format,
    b: Finite,
    bf: Format,
    rf: Format,
    mode: Option<Rounding>,
) -> Result<Value, Error> {
    match (af, bf) {
        (Format::D32, Format::D32) => product(d32(a), d32(b), rf, mode),
        (Format::D32, Format::D64) => product(d32(a), d64(b), rf, mode),
        (Format::D32, Format::D128) => product(d32(a), d128(b), rf, mode),
        (Format::D64, Format::D32) => product(d64(a), d32(b), rf, mode),
        (Format::D64, Format::D64) => product(d64(a), d64(b), rf, mode),
        (Format::D64, Format::D128) => product(d64(a), d128(b), rf, mode),
        (Format::D128, Format::D32) => product(d128(a), d32(b), rf, mode),
        (Format::D128, Format::D64) => product(d128(a), d64(b), rf, mode),
        (Format::D128, Format::D128) => product(d128(a), d128(b), rf, mode),
    }
}

fn check(a: Finite, af: Format, b: Finite, bf: Format, powers: &Powers) {
    for rf in FORMATS {
        check_into(a, af, b, bf, rf, powers);
    }
}

fn check_into(a: Finite, af: Format, b: Finite, bf: Format, rf: Format, powers: &Powers) {
    let a = decoded(refbid::encode(a, af), af);
    let b = decoded(refbid::encode(b, bf), bf);
    let neg = a.neg ^ b.neg;
    let exact = BigInt::from_biguint(
        if neg { Sign::Minus } else { Sign::Plus },
        BigUint::from(a.coeff) * BigUint::from(b.coeff),
    );
    for mode in MODES {
        let expected = directed(
            round_integer(exact.clone(), a.exp + b.exp, neg, rf, mode, powers),
            neg,
            mode,
            rf,
        );
        assert_eq!(
            actual(a, af, b, bf, rf, mode),
            expected,
            "a={a:?} ({}), b={b:?} ({}), result precision {}, mode={mode:?}",
            bounds(af).0,
            bounds(bf).0,
            bounds(rf).0
        );
    }
}

fn signed_pairs(a: Finite, af: Format, b: Finite, bf: Format, powers: &Powers) {
    for an in [false, true] {
        for bn in [false, true] {
            check(
                Finite { neg: an, ..a },
                af,
                Finite { neg: bn, ..b },
                bf,
                powers,
            );
        }
    }
}

fn exponent_pair(af: Format, bf: Format, sum: i32) -> Option<(i32, i32)> {
    let (_, alo, ahi) = bounds(af);
    let (_, blo, bhi) = bounds(bf);
    let low = alo.max(sum - bhi);
    let high = ahi.min(sum - blo);
    (low <= high).then(|| {
        let ae = 0.clamp(low, high);
        (ae, sum - ae)
    })
}

fn exponent_sums(af: Format, bf: Format, rf: Format, product_digits: i32) -> Vec<i32> {
    let (_, alo, ahi) = bounds(af);
    let (_, blo, bhi) = bounds(bf);
    let (rp, rlo, rhi) = bounds(rf);
    let mut sums = Vec::new();
    for delta in -3..=3 {
        sums.extend([
            alo + blo + delta,
            ahi + bhi + delta,
            rlo + delta,
            rlo - product_digits + delta,
            rhi + delta,
            rhi + rp as i32 - product_digits + delta,
        ]);
    }
    sums.extend(-product_digits - 1..=product_digits + 1);
    sums.retain(|&sum| exponent_pair(af, bf, sum).is_some());
    sums.sort_unstable();
    sums.dedup();
    sums
}

fn cross_coefficient_classes(af: Format, bf: Format, stride: usize, powers: &Powers) {
    let acs = coefficients(bounds(af).0);
    let bcs = coefficients(bounds(bf).0);
    for (i, &ac) in acs.iter().enumerate() {
        for &bc in bcs.iter().skip(i % stride).step_by(stride) {
            let product = BigUint::from(ac) * BigUint::from(bc);
            let n = if product == BigUint::from(0_u8) {
                0
            } else {
                product.to_str_radix(10).len() as i32
            };
            for rf in FORMATS {
                for sum in exponent_sums(af, bf, rf, n) {
                    let (ae, be) = exponent_pair(af, bf, sum).unwrap();
                    for (an, bn) in [(false, false), (false, true), (true, false), (true, true)] {
                        check_into(
                            Finite {
                                neg: an,
                                ..value(ac, ae)
                            },
                            af,
                            Finite {
                                neg: bn,
                                ..value(bc, be)
                            },
                            bf,
                            rf,
                            powers,
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn mul_all_width_pairs_coefficient_exponent_sign_classes() {
    let powers = Powers::new(13200);
    for af in FORMATS {
        for bf in FORMATS {
            let both_128 = matches!((af, bf), (Format::D128, Format::D128));
            cross_coefficient_classes(af, bf, if both_128 { 3 } else { 1 }, &powers);
        }
    }
}

#[test]
#[ignore = "every coefficient class pair of two Decimal128 operands, about two minutes"]
fn mul_decimal128_pair_every_coefficient_class_pair() {
    cross_coefficient_classes(Format::D128, Format::D128, 1, &Powers::new(13200));
}

#[test]
fn mul_limb_boundaries_operand_order_and_every_reachable_shift() {
    let powers = Powers::new(13200);
    let base = 10_u128.pow(17);
    let max = 10_u128.pow(34) - 1;
    let limbs = [0, 1, 9, base / 2, base - 1];
    let mut coeffs = std::vec![];
    for high in limbs {
        for low in limbs {
            coeffs.push(high * base + low);
        }
    }
    coeffs.extend([max, (1_u128 << 64) - 1, 1_u128 << 64, (1_u128 << 64) + 1]);
    for &ac in &coeffs {
        for &bc in &coeffs {
            for sum in [0, -6177, 6111] {
                let (ae, be) = exponent_pair(Format::D128, Format::D128, sum).unwrap();
                signed_pairs(
                    value(ac, ae),
                    Format::D128,
                    value(bc, be),
                    Format::D128,
                    &powers,
                );
            }
        }
    }
    for digits in 34..=68_u32 {
        let m = (digits - 1) / 2;
        let n = digits - 1 - m;
        for (ac, bc) in [
            (10_u128.pow(m), 10_u128.pow(n)),
            (10_u128.pow(m) + 1, 9 * 10_u128.pow(n) + 3),
            (10_u128.pow(m) * 3, 3 * 10_u128.pow(n) + 1),
            (10_u128.pow(m + 1) - 1, 10_u128.pow(n + 1) - 1),
        ] {
            if ac > max || bc > max {
                continue;
            }
            let product = BigUint::from(ac) * BigUint::from(bc);
            let n = product.to_str_radix(10).len() as u32;
            assert!((digits..=digits + 1).contains(&n));
            for (af, bf) in [
                (Format::D128, Format::D128),
                (Format::D64, Format::D128),
                (Format::D128, Format::D64),
            ] {
                if ac >= 10_u128.pow(bounds(af).0) || bc >= 10_u128.pow(bounds(bf).0) {
                    continue;
                }
                for sum in [0, -6177, -6176 - n as i32, 6111 + 34 - n as i32] {
                    if let Some((ae, be)) = exponent_pair(af, bf, sum) {
                        signed_pairs(value(ac, ae), af, value(bc, be), bf, &powers);
                    }
                }
            }
        }
    }
}

#[test]
fn mul_source_exponent_classes_and_equal_sum_decompositions() {
    fn exponents(format: Format) -> Vec<i32> {
        let (p, lo, hi) = bounds(format);
        let mut exps: Vec<i32> = (-(p as i32) - 1..=p as i32 + 1).collect();
        exps.extend(lo..=lo + 3);
        exps.extend(hi - 3..=hi);
        exps
    }
    let powers = Powers::new(13200);
    for af in FORMATS {
        for bf in FORMATS {
            for ae in exponents(af) {
                for be in exponents(bf) {
                    for ac in [0, 1, 10_u128.pow(bounds(af).0) - 1] {
                        for bc in [0, 1, 10_u128.pow(bounds(bf).0) - 1] {
                            signed_pairs(value(ac, ae), af, value(bc, be), bf, &powers);
                            let (ae2, be2) = exponent_pair(af, bf, ae + be).unwrap();
                            signed_pairs(value(ac, ae2), af, value(bc, be2), bf, &powers);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn mul_max_coefficient_carry_overflow_underflow_and_cohorts() {
    let powers = Powers::new(13200);
    let max = 10_u128.pow(34) - 1;
    let carry_a = 9_999_999;
    let carry_b = (10_u128.pow(35) - 1) / carry_a;
    assert_eq!(
        BigUint::from(carry_a) * BigUint::from(carry_b),
        BigUint::from(10_u128.pow(35) - 1)
    );
    for af in FORMATS {
        for bf in FORMATS {
            for ae in [bounds(af).1, 0, bounds(af).2] {
                for be in [bounds(bf).1, 0, bounds(bf).2] {
                    for (ac, bc) in [(0, 0), (0, 1), (1, 0)] {
                        signed_pairs(value(ac, ae), af, value(bc, be), bf, &powers);
                    }
                }
            }
            for z in 0..bounds(af).0 {
                signed_pairs(
                    value(10_u128.pow(z), -(z as i32)),
                    af,
                    value(1, 0),
                    bf,
                    &powers,
                );
            }
            for z in 0..bounds(bf).0 {
                signed_pairs(
                    value(1, 0),
                    af,
                    value(10_u128.pow(z), -(z as i32)),
                    bf,
                    &powers,
                );
            }
            if matches!(bf, Format::D128) {
                for sum in [
                    -6574, -6178, -6177, -6176, -6175, 6110, 6111, 6112, 6113, 6144, 6145, 6480,
                ] {
                    let Some((ae, be)) = exponent_pair(af, bf, sum) else {
                        continue;
                    };
                    for ac in [1, 2, 10, carry_a, 10_u128.pow(bounds(af).0) - 1] {
                        for bc in [1, 5, 10, max / 2, max / 2 + 1, max - 1, max, carry_b] {
                            signed_pairs(value(ac, ae), af, value(bc, be), bf, &powers);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn mul_exact_product_capacity_widening_and_exponent_boundaries() {
    const {
        let max = 10_u128.pow(34) - 1;
        let limb = 10_u128.pow(17) - 1;
        assert!(max < (1_u128 << 113));
        assert!(max >> 64 < (1_u128 << 49));
        assert!((u64::MAX as u128) * (u64::MAX as u128) < u128::MAX);
        assert!(limb * limb < 10_u128.pow(34));
        assert!(2 * limb * limb + limb < (1_u128 << 127));
        assert!(limb * limb + (2 * limb * limb + limb) / 10_u128.pow(17) < 10_u128.pow(34));
        assert!(2 * (<crate::layout::W128 as crate::layout::Width>::LAYOUT.qmin as i64) == -12352);
        assert!(2 * (<crate::layout::W128 as crate::layout::Width>::LAYOUT.qmax as i64) == 12222);
    }
    let powers = Powers::new(13200);
    let limit = 10_u128.pow(34);
    assert_eq!(3 * ((limit - 1) / 3), limit - 1);
    assert_eq!(2 * (limit / 2), limit);
    assert_eq!(101 * ((limit + 1) / 101), limit + 1);
    assert_eq!(
        BigUint::from(65536_u32) * BigUint::from(1_u128 << 112),
        BigUint::from(1_u8) << 128
    );
    for af in FORMATS {
        for bf in FORMATS {
            let max_a = 10_u128.pow(bounds(af).0) - 1;
            let max_b = 10_u128.pow(bounds(bf).0) - 1;
            let mut pairs = std::vec![(0, 0), (0, max_b), (max_a, 0), (1, 1), (max_a, max_b),];
            if matches!(af, Format::D128) || matches!(bf, Format::D128) {
                pairs.extend([
                    (3, (limit - 1) / 3),
                    (2, limit / 2),
                    (101, (limit + 1) / 101),
                    (65535, u128::MAX / 65535),
                ]);
                for a in [1, 2, 3, 101, 65535, 65536, max_a] {
                    let last = (limit - 1) / a;
                    for b in [last - 1, last, last + 1] {
                        if b <= max_b {
                            pairs.push((a, b));
                        }
                    }
                }
                for b in [(1_u128 << 112) - 1, 1_u128 << 112, (1_u128 << 112) + 1] {
                    pairs.push((65536, b));
                }
                for a in [1, 2, 3, 65536, max_a] {
                    for b in [(1_u128 << 64) - 1, 1_u128 << 64, (1_u128 << 64) + 1] {
                        pairs.push((a, b));
                    }
                    let carry_boundary = (1_u128 << 64).div_ceil(a);
                    for b in [carry_boundary - 1, carry_boundary, carry_boundary + 1] {
                        pairs.push((a, b));
                    }
                }
            }
            let swapped: Vec<_> = pairs.iter().map(|&(a, b)| (b, a)).collect();
            pairs.extend(swapped);
            pairs.retain(|&(a, b)| a <= max_a && b <= max_b);
            for (ac, bc) in pairs {
                for sum in [
                    bounds(af).1 + bounds(bf).1,
                    -6177,
                    -6176,
                    -6175,
                    -399,
                    -398,
                    -102,
                    -101,
                    0,
                    90,
                    91,
                    369,
                    370,
                    6110,
                    6111,
                    6112,
                    bounds(af).2 + bounds(bf).2,
                ] {
                    if let Some((ae, be)) = exponent_pair(af, bf, sum) {
                        signed_pairs(value(ac, ae), af, value(bc, be), bf, &powers);
                    }
                }
            }
        }
    }
}

fn random_mul(count: u64, powers: &Powers) {
    for (ai, af) in FORMATS.into_iter().enumerate() {
        for (bi, bf) in FORMATS.into_iter().enumerate() {
            let mut rng = Rng(0x3c87_4e91_a526_0bd2 ^ (ai * 3 + bi) as u64);
            for _ in 0..count {
                check(rng.value(af), af, rng.value(bf), bf, powers);
            }
        }
    }
}

#[test]
fn mul_all_width_pairs_bigint_random() {
    random_mul(2000, &Powers::new(13200));
}

#[test]
#[ignore = "300000 pairs per each of nine operand width pairs, three result widths, all three modes"]
fn mul_all_width_pairs_bigint_million() {
    let started = Instant::now();
    let powers = Powers::new(13200);
    random_mul(300_000, &powers);
    std::eprintln!(
        "mul 2.7 million pairs, 24.3 million directed/exact checks: {:?}",
        started.elapsed()
    );
}
