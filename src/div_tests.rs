use num_bigint::BigUint;
use std::string::ToString;
use std::time::Instant;
use std::vec::Vec;

use crate::refbid::{self, Finite, Format};
use crate::test_support::*;
use crate::{Decimal32, Decimal64, Decimal128, Error, Operand, Rounding};

fn quotient(
    a: impl Operand,
    b: impl Operand,
    rf: Format,
    mode: Option<Rounding>,
) -> Result<Value, Error> {
    match (rf, mode) {
        (Format::D32, None) => Decimal32::div(a, b).map(|v| u128::from(v.to_bits())),
        (Format::D32, Some(dir)) => {
            Decimal32::div_rounded(a, b, dir).map(|v| u128::from(v.to_bits()))
        }
        (Format::D64, None) => Decimal64::div(a, b).map(|v| u128::from(v.to_bits())),
        (Format::D64, Some(dir)) => {
            Decimal64::div_rounded(a, b, dir).map(|v| u128::from(v.to_bits()))
        }
        (Format::D128, None) => Decimal128::div(a, b).map(|v| v.to_bits()),
        (Format::D128, Some(dir)) => Decimal128::div_rounded(a, b, dir).map(|v| v.to_bits()),
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
        (Format::D32, Format::D32) => quotient(d32(a), d32(b), rf, mode),
        (Format::D32, Format::D64) => quotient(d32(a), d64(b), rf, mode),
        (Format::D32, Format::D128) => quotient(d32(a), d128(b), rf, mode),
        (Format::D64, Format::D32) => quotient(d64(a), d32(b), rf, mode),
        (Format::D64, Format::D64) => quotient(d64(a), d64(b), rf, mode),
        (Format::D64, Format::D128) => quotient(d64(a), d128(b), rf, mode),
        (Format::D128, Format::D32) => quotient(d128(a), d32(b), rf, mode),
        (Format::D128, Format::D64) => quotient(d128(a), d64(b), rf, mode),
        (Format::D128, Format::D128) => quotient(d128(a), d128(b), rf, mode),
    }
}

// The exact quotient is the fraction A*10^(qa-qb) / B. The result exponent is
// the least x >= lo whose scaled integer part is below 10^p; an exact result
// then moves toward the preferred exponent while the coefficient ends in zero.
fn reference(
    a: Finite,
    b: Finite,
    rf: Format,
    mode: Option<Rounding>,
    powers: &Powers,
) -> Result<Value, Error> {
    let neg = a.neg != b.neg;
    if b.coeff == 0 {
        return Ok(if a.coeff == 0 {
            Value::Nan
        } else {
            Value::Infinity(neg)
        });
    }
    directed(reference_finite(a, b, rf, mode, powers), neg, mode, rf)
}

fn reference_finite(
    a: Finite,
    b: Finite,
    rf: Format,
    mode: Option<Rounding>,
    powers: &Powers,
) -> Result<Finite, Error> {
    let (p, lo, hi) = bounds(rf);
    let neg = a.neg != b.neg;
    let pref = a.exp - b.exp;
    if a.coeff == 0 {
        return Ok(Finite {
            neg,
            coeff: 0,
            exp: pref.clamp(lo, hi),
        });
    }
    let n = BigUint::from(a.coeff);
    let d = BigUint::from(b.coeff);
    let at = |x: i32| {
        let (num, den) = if pref >= x {
            (&n * &*powers.pow(i64::from(pref - x)), d.clone())
        } else {
            (n.clone(), &d * &*powers.pow(i64::from(x - pref)))
        };
        (&num / &den, &num % &den)
    };
    let limit = powers.get(i64::from(p));
    let mut x = lo.max(pref + powers.decimal_len(&n) - powers.decimal_len(&d) - p as i32 - 1);
    let (mut q, mut r) = at(x);
    while q >= *limit {
        x += 1;
        (q, r) = at(x);
    }
    let zero = BigUint::from(0_u8);
    if r == zero {
        let target = pref.clamp(lo, hi);
        while x < target && &q % 10_u8 == zero {
            q /= 10_u8;
            x += 1;
        }
    } else {
        match mode {
            // x is the least exponent at which the integer part is below
            // 10^p, so |a/b| < 10^(p+x) <= MAX for x < hi; for x = hi the
            // nonzero remainder exceeds MAX exactly when q has p nines.
            None => {
                let exceeds = x > hi || (x == hi && q == limit - 1_u8);
                return Err(if exceeds {
                    Error::Overflow
                } else {
                    Error::Inexact
                });
            }
            Some(dir) => {
                if (dir == Rounding::TowardPositive) != neg {
                    q += 1_u8;
                    if q == *limit {
                        q /= 10_u8;
                        x += 1;
                    }
                }
            }
        }
    }
    if x > hi {
        return Err(Error::Overflow);
    }
    Ok(Finite {
        neg,
        coeff: u128::try_from(q).unwrap(),
        exp: x,
    })
}

fn check_into(a: Finite, af: Format, b: Finite, bf: Format, rf: Format, powers: &Powers) {
    let a = decoded(refbid::encode(a, af), af);
    let b = decoded(refbid::encode(b, bf), bf);
    for mode in MODES {
        assert_eq!(
            actual(a, af, b, bf, rf, mode),
            reference(a, b, rf, mode, powers),
            "a={a:?} ({}), b={b:?} ({}), result precision {}, mode={mode:?}",
            bounds(af).0,
            bounds(bf).0,
            bounds(rf).0
        );
    }
}

fn check(a: Finite, af: Format, b: Finite, bf: Format, powers: &Powers) {
    for rf in FORMATS {
        check_into(a, af, b, bf, rf, powers);
    }
}

fn signed_pairs(a: Finite, af: Format, b: Finite, bf: Format, rf: Format, powers: &Powers) {
    for (an, bn) in [(false, false), (false, true), (true, false), (true, true)] {
        check_into(
            Finite { neg: an, ..a },
            af,
            Finite { neg: bn, ..b },
            bf,
            rf,
            powers,
        );
    }
}

fn exponent_pair(af: Format, bf: Format, diff: i32) -> Option<(i32, i32)> {
    let (_, alo, ahi) = bounds(af);
    let (_, blo, bhi) = bounds(bf);
    let low = alo.max(diff + blo);
    let high = ahi.min(diff + bhi);
    (low <= high).then(|| {
        let ae = 0.clamp(low, high);
        (ae, ae - diff)
    })
}

// Exponent differences that place a quotient of magnitude about
// 10^(diff + lead) at each result range transition: overflow above
// 10^(qmax+P), the least full-precision exponent qmin+P-1, total underflow
// below 10^qmin, the zero exponent clamps, and the operand range ends.
fn exponent_differences(af: Format, bf: Format, rf: Format, lead: i32) -> Vec<i32> {
    let (_, alo, ahi) = bounds(af);
    let (_, blo, bhi) = bounds(bf);
    let (rp, rlo, rhi) = bounds(rf);
    let rp = rp as i32;
    let mut diffs = std::vec![-1, 0, 1, rp];
    for delta in -1..=1 {
        diffs.extend([
            alo - bhi + delta,
            ahi - blo + delta,
            rlo + delta,
            rhi + delta,
            rhi + rp - lead + delta,
            rlo + rp - 1 - lead + delta,
            rlo - lead + delta,
        ]);
    }
    diffs.retain(|&diff| exponent_pair(af, bf, diff).is_some());
    diffs.sort_unstable();
    diffs.dedup();
    diffs
}

fn decimal_len(c: u128) -> i32 {
    if c == 0 { 0 } else { c.ilog10() as i32 + 1 }
}

fn cross_coefficient_classes(af: Format, bf: Format, stride: usize, powers: &Powers) {
    let acs = coefficients(bounds(af).0);
    let bcs = coefficients(bounds(bf).0);
    for (i, &ac) in acs.iter().enumerate() {
        for &bc in bcs.iter().skip(i % stride).step_by(stride) {
            for rf in FORMATS {
                for diff in exponent_differences(af, bf, rf, decimal_len(ac) - decimal_len(bc)) {
                    let (ae, be) = exponent_pair(af, bf, diff).unwrap();
                    signed_pairs(value(ac, ae), af, value(bc, be), bf, rf, powers);
                }
            }
        }
    }
}

// About 400 coefficient class pairs per operand width pair; the offset keeps
// every class of both operands. The ignored test below takes every pair.
#[test]
fn div_all_width_pairs_coefficient_exponent_sign_classes() {
    let powers = Powers::new(13200);
    for af in FORMATS {
        for bf in FORMATS {
            let pairs = coefficients(bounds(af).0).len() * coefficients(bounds(bf).0).len();
            cross_coefficient_classes(af, bf, (pairs / 400).max(1), &powers);
        }
    }
}

#[test]
#[ignore = "every coefficient class pair for every operand width pair"]
fn div_every_coefficient_class_pair() {
    let started = Instant::now();
    let powers = Powers::new(13200);
    for af in FORMATS {
        for bf in FORMATS {
            cross_coefficient_classes(af, bf, 1, &powers);
        }
    }
    std::eprintln!("div every class pair: {:?}", started.elapsed());
}

#[test]
fn div_zero_operands_signs_and_exponent_clamps() {
    let powers = Powers::new(13200);
    for af in FORMATS {
        for bf in FORMATS {
            let (_, alo, ahi) = bounds(af);
            let (_, blo, bhi) = bounds(bf);
            for ae in [alo, -1, 0, 1, ahi] {
                for be in [blo, -1, 0, 1, bhi] {
                    for ac in [0, 1, 7] {
                        for bc in [0, 1, 3] {
                            for rf in FORMATS {
                                signed_pairs(value(ac, ae), af, value(bc, be), bf, rf, &powers);
                            }
                        }
                    }
                }
            }
        }
    }
    let zero: Decimal64 = "0".parse().unwrap();
    let one: Decimal64 = "1".parse().unwrap();
    let shown =
        |r: Result<Decimal64, Error>| r.map(|v| classified(u128::from(v.to_bits()), Format::D64));
    assert_eq!(shown(Decimal64::div(zero, zero)), Ok(Value::Nan));
    assert_eq!(shown(Decimal64::div(one, zero)), Ok(Value::Infinity(false)));
    assert_eq!(
        shown(Decimal64::div(-one, -zero)),
        Ok(Value::Infinity(false))
    );
    assert_eq!(shown(Decimal64::div(one, -zero)), Ok(Value::Infinity(true)));
    for dir in [Rounding::TowardNegative, Rounding::TowardPositive] {
        assert_eq!(
            shown(Decimal64::div_rounded(-zero, zero, dir)),
            Ok(Value::Nan)
        );
        assert_eq!(
            shown(Decimal64::div_rounded(one, -zero, dir)),
            Ok(Value::Infinity(true))
        );
    }
    let q = Decimal64::div(
        "0.00".parse::<Decimal64>().unwrap(),
        "-5".parse::<Decimal64>().unwrap(),
    );
    assert_eq!(q.map(|v| v.to_string()), Ok("-0.00".into()));
}

#[test]
fn div_documented_examples() {
    let d32 = |s: &str| s.parse::<Decimal32>().unwrap();
    let text = |r: Result<Decimal32, Error>| r.map(|v| v.to_string());
    assert_eq!(
        text(Decimal32::div(d32("1.00"), d32("4"))),
        Ok("0.25".into())
    );
    assert_eq!(text(Decimal32::div(d32("1"), d32("8"))), Ok("0.125".into()));
    assert_eq!(
        text(Decimal32::div(d32("6.00"), d32("2"))),
        Ok("3.00".into())
    );
    assert_eq!(
        text(Decimal32::div(d32("1"), d32("3"))),
        Err(Error::Inexact)
    );
    assert_eq!(
        text(Decimal32::div_rounded(
            d32("1"),
            d32("3"),
            Rounding::TowardPositive
        )),
        Ok("0.3333334".into())
    );
    assert_eq!(
        text(Decimal32::div_rounded(
            d32("-1"),
            d32("3"),
            Rounding::TowardPositive
        )),
        Ok("-0.3333333".into())
    );
    assert_eq!(
        text(Decimal32::div_rounded(
            d32("2"),
            d32("3"),
            Rounding::TowardNegative
        )),
        Ok("0.6666666".into())
    );
}

// Quotient digit paths of the scaled division: every divisor digit length,
// hence every chunk size 4..=37; the largest scale k = P+33 (one-digit
// dividend, 34-digit divisor); the k = 0 path (dividend at least P digits
// longer than the divisor); exact quotients with removable zeros; terminating
// quotients of powers of two and five through and past the precision; and the
// precision carry when rounding up a quotient of P nines.
#[test]
fn div_scale_paths_terminating_quotients_and_precision_carry() {
    let powers = Powers::new(13200);
    let mut dividends = std::vec![1_u128, 3, 9, 10_u128.pow(33), 10_u128.pow(34) - 1];
    let mut divisors = Vec::new();
    for n in 1..=34 {
        let power = 10_u128.pow(n);
        divisors.extend([power / 10, power / 10 + 1, power - 1, power / 10 * 7 + 3]);
    }
    for m in 0..=112 {
        divisors.push(1_u128 << m);
        if m <= 48 {
            divisors.push(5_u128.pow(m));
        }
    }
    for p in [7, 16, 34] {
        let max = 10_u128.pow(p) - 1;
        dividends.extend([max, 2 * max + 1, max - 1, 10_u128.pow(p - 1)]);
        divisors.extend([2, max, max - 1]);
    }
    dividends.retain(|&c| c < 10_u128.pow(34));
    divisors.retain(|&c| c < 10_u128.pow(34));
    for &ac in &dividends {
        for &bc in &divisors {
            for rf in FORMATS {
                for diff in [-1, 0, 2] {
                    let (ae, be) = exponent_pair(Format::D128, Format::D128, diff).unwrap();
                    signed_pairs(
                        value(ac, ae),
                        Format::D128,
                        value(bc, be),
                        Format::D128,
                        rf,
                        &powers,
                    );
                }
            }
        }
    }
}

#[test]
fn div_range_boundaries_overflow_underflow_and_cohorts() {
    let powers = Powers::new(13200);
    for af in FORMATS {
        for bf in FORMATS {
            let (ap, alo, ahi) = bounds(af);
            let max = 10_u128.pow(ap) - 1;
            for rf in FORMATS {
                let (_, rlo, rhi) = bounds(rf);
                for ac in [1, 5, max, max - 1, 10_u128.pow(ap - 1)] {
                    for bc in [1, 2, 3, 10, 9, 11] {
                        for ae in [alo, alo + 1, rlo, rlo + 1, rhi - 1, rhi, ahi - 1, ahi] {
                            if !(alo..=ahi).contains(&ae) {
                                continue;
                            }
                            for be in [-2, -1, 0, 1, 2] {
                                signed_pairs(value(ac, ae), af, value(bc, be), bf, rf, &powers);
                            }
                        }
                    }
                }
            }
        }
    }
}

fn near_zero(rng: &mut Rng, format: Format) -> Finite {
    Finite {
        exp: (rng.next() % 81) as i32 - 40,
        ..rng.value(format)
    }
}

fn random_div(count: u64, powers: &Powers) {
    for (ai, af) in FORMATS.into_iter().enumerate() {
        for (bi, bf) in FORMATS.into_iter().enumerate() {
            let mut rng = Rng(0x6a09_e667_f3bc_c908 ^ (ai * 3 + bi) as u64);
            for _ in 0..count {
                check(rng.value(af), af, rng.value(bf), bf, powers);
                check(
                    near_zero(&mut rng, af),
                    af,
                    near_zero(&mut rng, bf),
                    bf,
                    powers,
                );
            }
        }
    }
}

#[test]
fn div_all_width_pairs_bigint_random() {
    random_div(1000, &Powers::new(13200));
}

#[test]
#[ignore = "150000 pairs of each of two exponent distributions per operand width pair, three result widths, all three modes"]
fn div_all_width_pairs_bigint_million() {
    let started = Instant::now();
    let powers = Powers::new(13200);
    random_div(150_000, &powers);
    std::eprintln!(
        "div 2.7 million pairs, 24.3 million directed/exact checks: {:?}",
        started.elapsed()
    );
}
