use crate::refbid::{self, Finite, Format};
use crate::test_support::*;
use crate::{Decimal32, Decimal64, Decimal128, Error, Operand, Rounding};
use std::time::Instant;

fn actual(
    a: impl Operand,
    b: impl Operand,
    target: Format,
    subtract: bool,
    dir: Option<Rounding>,
) -> Result<Value, Error> {
    macro_rules! run {
        ($ty:ident) => {
            match (subtract, dir) {
                (false, None) => $ty::add(a, b),
                (false, Some(dir)) => $ty::add_rounded(a, b, dir),
                (true, None) => $ty::sub(a, b),
                (true, Some(dir)) => $ty::sub_rounded(a, b, dir),
            }
            .map(|v| classified(v.to_bits() as u128, target))
        };
    }
    match target {
        Format::D32 => run!(Decimal32),
        Format::D64 => run!(Decimal64),
        Format::D128 => run!(Decimal128),
    }
}

fn check(a: Finite, af: Format, b: Finite, bf: Format, target: Format, powers: &Powers) {
    let a = decoded(refbid::encode(a, af), af);
    let b = decoded(refbid::encode(b, bf), bf);
    let exp = a.exp.min(b.exp);
    let ai = integer_at(a, exp, powers);
    let bi = integer_at(b, exp, powers);
    for subtract in [false, true] {
        let exact = if subtract { &ai - &bi } else { &ai + &bi };
        for mode in MODES {
            let b_neg = b.neg != subtract;
            let zero_neg = if a.coeff == 0 && b.coeff == 0 && a.neg == b_neg {
                a.neg
            } else {
                mode == Some(Rounding::TowardNegative)
            };
            let expected = directed(
                round_integer(exact.clone(), exp, zero_neg, target, mode, powers),
                exact.sign() == num_bigint::Sign::Minus,
                mode,
                target,
            );
            macro_rules! with_b {
                ($av:expr) => {
                    match bf {
                        Format::D32 => actual($av, d32(b), target, subtract, mode),
                        Format::D64 => actual($av, d64(b), target, subtract, mode),
                        Format::D128 => actual($av, d128(b), target, subtract, mode),
                    }
                };
            }
            let result = match af {
                Format::D32 => with_b!(d32(a)),
                Format::D64 => with_b!(d64(a)),
                Format::D128 => with_b!(d128(a)),
            };
            assert_eq!(
                result,
                expected,
                "a={a:?} ({}), b={b:?} ({}), target={}, sub={subtract}, mode={mode:?}",
                bounds(af).0,
                bounds(bf).0,
                bounds(target).0
            );
        }
    }
}

fn signed_pairs(a: Finite, af: Format, b: Finite, bf: Format, target: Format, powers: &Powers) {
    for an in [false, true] {
        for bn in [false, true] {
            check(
                Finite { neg: an, ..a },
                af,
                Finite { neg: bn, ..b },
                bf,
                target,
                powers,
            );
        }
    }
}

fn exponents(af: Format, bf: Format, gap: i32) -> Option<(i32, i32)> {
    let (_, alo, ahi) = bounds(af);
    let (_, blo, bhi) = bounds(bf);
    let low = blo.max(alo - gap);
    let high = bhi.min(ahi - gap);
    (low <= high).then(|| {
        let b = 0.clamp(low, high);
        (b + gap, b)
    })
}

#[test]
fn addsub_all_27_coefficient_gap_sign_classes() {
    let powers = Powers::new(12400);
    for af in FORMATS {
        let ac = coefficients(bounds(af).0);
        for bf in FORMATS {
            let bc = coefficients(bounds(bf).0);
            for target in FORMATS {
                for reverse in [false, true] {
                    let mut gaps: std::vec::Vec<i32> = (0..=39).collect();
                    gaps.extend([70, 200, 12000]);
                    let max_gap = if reverse {
                        bounds(bf).2 - bounds(af).1
                    } else {
                        bounds(af).2 - bounds(bf).1
                    };
                    for gap in &mut gaps {
                        *gap = (*gap).min(max_gap);
                    }
                    gaps.sort_unstable();
                    gaps.dedup();
                    for gap in gaps {
                        let (ae, be) = if reverse {
                            let (be, ae) = exponents(bf, af, gap).unwrap();
                            (ae, be)
                        } else {
                            exponents(af, bf, gap).unwrap()
                        };
                        for i in 0..ac.len().max(bc.len()) {
                            signed_pairs(
                                Finite {
                                    neg: false,
                                    coeff: ac[i % ac.len()],
                                    exp: ae,
                                },
                                af,
                                Finite {
                                    neg: false,
                                    coeff: bc[(i + bc.len() / 2) % bc.len()],
                                    exp: be,
                                },
                                bf,
                                target,
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
fn addsub_exact_compression_boundary_and_trailing_zeros() {
    let powers = Powers::new(12400);
    for af in FORMATS {
        for bf in FORMATS {
            for target in FORMATS {
                for n in 1..=bounds(af).0 {
                    for z in 0..bounds(bf).0 {
                        for boundary in [37, 38] {
                            for removed in [0, z] {
                                let gap = boundary - n + removed;
                                let (ae, be) = exponents(af, bf, gap as i32).unwrap();
                                let a = Finite {
                                    neg: false,
                                    coeff: 10_u128.pow(n - 1),
                                    exp: ae,
                                };
                                let b = Finite {
                                    neg: false,
                                    coeff: 10_u128.pow(z),
                                    exp: be,
                                };
                                signed_pairs(a, af, b, bf, target, &powers);
                                signed_pairs(b, bf, a, af, target, &powers);
                            }
                        }
                    }
                }
                for gap in [3, 4, 37, 38, 39, 70] {
                    let (ae, be) = exponents(af, bf, gap).unwrap();
                    for bc in [1, 10_u128.pow(bounds(bf).0) - 1] {
                        signed_pairs(
                            Finite {
                                neg: false,
                                coeff: 1,
                                exp: ae,
                            },
                            af,
                            Finite {
                                neg: false,
                                coeff: bc,
                                exp: be,
                            },
                            bf,
                            target,
                            &powers,
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn addsub_zero_cohorts_and_self_operations() {
    let powers = Powers::new(12400);
    for af in FORMATS {
        for bf in FORMATS {
            for target in FORMATS {
                for ae in [bounds(af).1, 0, bounds(af).2] {
                    for be in [bounds(bf).1, 0, bounds(bf).2] {
                        signed_pairs(
                            Finite {
                                neg: false,
                                coeff: 0,
                                exp: ae,
                            },
                            af,
                            Finite {
                                neg: false,
                                coeff: 0,
                                exp: be,
                            },
                            bf,
                            target,
                            &powers,
                        );
                    }
                }
                let p = bounds(af).0.min(bounds(bf).0);
                for coeff in coefficients(p) {
                    for exp in [
                        bounds(af).1.max(bounds(bf).1),
                        0,
                        bounds(af).2.min(bounds(bf).2),
                    ] {
                        let a = Finite {
                            neg: false,
                            coeff,
                            exp,
                        };
                        signed_pairs(a, af, a, bf, target, &powers);
                        for z in 1..p {
                            if coeff < 10_u128.pow(p - z) && exp - z as i32 >= bounds(bf).1 {
                                signed_pairs(
                                    a,
                                    af,
                                    Finite {
                                        coeff: coeff * 10_u128.pow(z),
                                        exp: exp - z as i32,
                                        ..a
                                    },
                                    bf,
                                    target,
                                    &powers,
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
fn addsub_largest_finite_underflow_and_cancellation() {
    let powers = Powers::new(12400);
    for target in FORMATS {
        let (p, lo, hi) = bounds(target);
        for af in FORMATS {
            for bf in FORMATS {
                let (ap, alo, ahi) = bounds(af);
                let (bp, blo, bhi) = bounds(bf);
                for exp in [lo - 40, lo - 2, lo - 1, lo, lo + 1, hi - 1, hi, hi + 1] {
                    for ac in [1, 10_u128.pow(p.min(ap)) - 1, 10_u128.pow(ap) - 1] {
                        if !(alo..=ahi).contains(&exp) {
                            continue;
                        }
                        for be in [exp - 39, exp - 4, exp - 1, exp, exp + 1] {
                            if !(blo..=bhi).contains(&be) {
                                continue;
                            }
                            for bc in [0, 1, 9, 10_u128.pow(bp) - 1] {
                                signed_pairs(
                                    Finite {
                                        neg: false,
                                        coeff: ac,
                                        exp,
                                    },
                                    af,
                                    Finite {
                                        neg: false,
                                        coeff: bc,
                                        exp: be,
                                    },
                                    bf,
                                    target,
                                    &powers,
                                );
                            }
                        }
                    }
                }
                for extra in 0..=3_u32 {
                    let exp = hi - extra as i32;
                    let ac = (10_u128.pow(p) - 1) * 10_u128.pow(extra);
                    if ac >= 10_u128.pow(ap)
                        || !(alo..=ahi).contains(&exp)
                        || !(blo..=bhi).contains(&exp)
                    {
                        continue;
                    }
                    for delta in 0..=3 {
                        signed_pairs(
                            Finite {
                                neg: false,
                                coeff: ac - delta,
                                exp,
                            },
                            af,
                            Finite {
                                neg: false,
                                coeff: 1 + delta,
                                exp,
                            },
                            bf,
                            target,
                            &powers,
                        );
                    }
                }
            }
        }
    }
}

fn random_addsub(count: u64, powers: &Powers) {
    for (ai, af) in FORMATS.into_iter().enumerate() {
        for (bi, bf) in FORMATS.into_iter().enumerate() {
            for (ti, target) in FORMATS.into_iter().enumerate() {
                let mut rng = Rng(0x36f9_856a_2201_cfb7 ^ (ai * 9 + bi * 3 + ti) as u64);
                for _ in 0..count {
                    check(rng.value(af), af, rng.value(bf), bf, target, powers);
                }
            }
        }
    }
}

#[test]
fn addsub_all_27_bigint_random() {
    random_addsub(2000, &Powers::new(12400));
}

#[test]
#[ignore = "one million pairs per each of 27 width combinations, both operations and all three modes"]
fn addsub_all_27_bigint_million() {
    let started = Instant::now();
    let powers = Powers::new(12400);
    random_addsub(1_000_000, &powers);
    std::eprintln!(
        "add/sub 27 million pairs, 162 million directed/exact checks: {:?}",
        started.elapsed()
    );
}

#[test]
fn addsub_equal_exponent_exact_path_boundaries() {
    let powers = Powers::new(12400);
    for af in FORMATS {
        let (ap, alo, ahi) = bounds(af);
        let amax = 10_u128.pow(ap) - 1;
        for bf in FORMATS {
            let (bp, blo, bhi) = bounds(bf);
            let bmax = 10_u128.pow(bp) - 1;
            for target in FORMATS {
                let (p, lo, hi) = bounds(target);
                let mut pairs = std::vec![(0, 0), (0, bmax), (amax, 0), (amax, bmax)];
                for c in [1, amax.min(bmax)] {
                    pairs.push((c, c));
                }
                for total in [10_u128.pow(p) - 1, 10_u128.pow(p), 10_u128.pow(p) + 1] {
                    let ac = amax.min(total);
                    let bc = total - ac;
                    if bc <= bmax {
                        pairs.push((ac, bc));
                    }
                }
                for exp in [
                    alo.max(blo),
                    ahi.min(bhi),
                    lo - 1,
                    lo,
                    lo + 1,
                    -8,
                    0,
                    hi - 1,
                    hi,
                    hi + 1,
                ] {
                    if !(alo..=ahi).contains(&exp) || !(blo..=bhi).contains(&exp) {
                        continue;
                    }
                    for &(ac, bc) in &pairs {
                        signed_pairs(
                            Finite {
                                neg: false,
                                coeff: ac,
                                exp,
                            },
                            af,
                            Finite {
                                neg: false,
                                coeff: bc,
                                exp,
                            },
                            bf,
                            target,
                            &powers,
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn addsub_near_exponent_exact_path_boundaries() {
    let powers = Powers::new(12400);
    assert!((10_u128.pow(34) - 1) * 10_001 < (1_u128 << 127));
    assert!((10_u128.pow(34) - 1).checked_mul(100_001).is_none());
    for af in FORMATS {
        let (ap, alo, ahi) = bounds(af);
        let amax = 10_u128.pow(ap) - 1;
        for bf in FORMATS {
            let (bp, blo, bhi) = bounds(bf);
            let bmax = 10_u128.pow(bp) - 1;
            for target in FORMATS {
                let (p, lo, hi) = bounds(target);
                for gap in -5_i32..=5 {
                    if gap == 0 {
                        continue;
                    }
                    let scale_a = 10_u128.pow(gap.max(0) as u32);
                    let scale_b = 10_u128.pow((-gap).max(0) as u32);
                    let mut pairs = std::vec![(0, 0), (0, bmax), (amax, 0), (amax, bmax)];
                    for ac in [1, (amax / scale_b).min(bmax / scale_a)] {
                        pairs.push((ac * scale_b, ac * scale_a));
                    }
                    for total in [10_u128.pow(p) - 1, 10_u128.pow(p), 10_u128.pow(p) + 1] {
                        if gap > 0 {
                            let ac = amax.min(total / scale_a);
                            let bc = total - ac * scale_a;
                            if bc <= bmax {
                                pairs.push((ac, bc));
                            }
                        } else {
                            let bc = bmax.min(total / scale_b);
                            let ac = total - bc * scale_b;
                            if ac <= amax {
                                pairs.push((ac, bc));
                            }
                        }
                    }
                    let source_low = (alo - gap.max(0)).max(blo - (-gap).max(0));
                    let source_high = (ahi - gap.max(0)).min(bhi - (-gap).max(0));
                    for exp in [
                        source_low,
                        source_high,
                        lo - 1,
                        lo,
                        lo + 1,
                        -12,
                        -8,
                        0,
                        hi - 1,
                        hi,
                        hi + 1,
                    ] {
                        let ae = exp + gap.max(0);
                        let be = exp + (-gap).max(0);
                        if !(alo..=ahi).contains(&ae) || !(blo..=bhi).contains(&be) {
                            continue;
                        }
                        for &(ac, bc) in &pairs {
                            signed_pairs(
                                Finite {
                                    neg: false,
                                    coeff: ac,
                                    exp: ae,
                                },
                                af,
                                Finite {
                                    neg: false,
                                    coeff: bc,
                                    exp: be,
                                },
                                bf,
                                target,
                                &powers,
                            );
                        }
                    }
                }
            }
        }
    }
}
