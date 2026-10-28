use bidecimal::{Decimal32, Decimal64, Decimal128, Error, Rounding};
use criterion::{Criterion, criterion_group, criterion_main};
use num_bigint::BigInt;
use std::time::Duration;

#[expect(
    dead_code,
    reason = "shared with the keys benchmark, which uses the rest"
)]
mod keyed;
use keyed::{COUNT, Number, SEED, Spec, SplitMix64, decoded, per_element};

#[expect(
    dead_code,
    reason = "shared by the benchmarks, each of which uses part of it"
)]
mod support;
use support::{Bid, Exact};

mod reference;
use reference::{floor_fields, floor_quotient, signed};

trait Divide: Number {
    fn quotient(self, other: Self) -> Result<Self, Error>;
    fn quotient_floor(self, other: Self) -> Result<Self, Error>;
}
macro_rules! divide {
    ($($t:ident),*) => {$(
        impl Divide for $t {
            fn quotient(self, other: Self) -> Result<Self, Error> {
                $t::div(self, other)
            }
            fn quotient_floor(self, other: Self) -> Result<Self, Error> {
                $t::div_rounded(self, other, Rounding::TowardNegative)
            }
        }
    )*};
}
divide!(Decimal32, Decimal64, Decimal128);

fn product(q: Spec, b: Spec) -> Spec {
    Spec {
        neg: q.neg != b.neg,
        coeff: q.coeff * b.coeff,
        exp: q.exp + b.exp,
    }
}

/// The fields of `x / y` rounded toward negative to `digits` significant
/// digits, and whether they are the exact quotient, which takes the exponent
/// closest to the preferred `x.exp - y.exp`.
fn quotient_fields(x: Spec, y: Spec, digits: u32) -> (Bid, bool) {
    if let Some(floor) = floor_quotient(x, y, digits) {
        return (floor, false);
    }
    let divisor = BigInt::from(y.coeff);
    let (mut coeff, mut exp) = (signed(x), x.exp - y.exp);
    while &coeff % &divisor != BigInt::ZERO {
        coeff *= 10;
        exp -= 1;
    }
    coeff /= divisor;
    if y.neg {
        coeff = -coeff;
    }
    let floor = floor_fields(coeff.clone(), exp, digits, false);
    let exact = floor.exact() == Exact::new(coeff, exp);
    (floor, exact)
}

/// Exact quotients `q × b ÷ b` and random operand pairs, unrounded and rounded.
fn division<T: Divide>(c: &mut Criterion, rng: &mut SplitMix64) {
    let half = T::DIGITS / 2;
    let specs: Vec<(Spec, Spec)> = (0..COUNT)
        .map(|_| (rng.spec(half), rng.spec(T::DIGITS - half)))
        .collect();
    let pairs: Vec<(T, T)> = specs
        .iter()
        .map(|&(q, b)| (product(q, b).parse(), b.parse()))
        .collect();
    for (&(q, _), &(a, b)) in specs.iter().zip(&pairs) {
        assert_eq!(a.quotient(b).map(decoded), Ok(q.fields()), "{a:?} / {b:?}");
    }
    per_element(c, &format!("div_{}_exact", T::NAME), &pairs, |&(a, b)| {
        a.quotient(b)
    });
    let specs: Vec<(Spec, Spec)> = (0..COUNT)
        .map(|_| (rng.spec(T::DIGITS), rng.spec(T::DIGITS)))
        .collect();
    let random: Vec<(T, T)> = specs.iter().map(|&(a, b)| (a.parse(), b.parse())).collect();
    for (&(x, y), &(a, b)) in specs.iter().zip(&random) {
        let (floor, exact) = quotient_fields(x, y, T::DIGITS);
        let expected = if exact {
            Ok(&floor)
        } else {
            Err(&Error::Inexact)
        };
        assert_eq!(
            a.quotient(b).map(decoded).as_ref(),
            expected,
            "{a:?} / {b:?}"
        );
        assert_eq!(a.quotient_floor(b).map(decoded), Ok(floor), "{a:?} / {b:?}");
    }
    per_element(c, &format!("div_{}_random", T::NAME), &random, |&(a, b)| {
        a.quotient(b)
    });
    per_element(
        c,
        &format!("divfloor_{}_random", T::NAME),
        &random,
        |&(a, b)| a.quotient_floor(b),
    );
}

macro_rules! conversions {
    ($c:expr, $rng:expr, $($small:ident $large:ident $rounded:ident),*) => {$(
        let name = format!("{}_{}", <$small as Number>::NAME, <$large as Number>::NAME);
        let specs: Vec<Spec> = (0..COUNT).map(|_| $rng.spec(<$small as Number>::DIGITS)).collect();
        let small: Vec<$small> = specs.iter().map(|&x| x.parse()).collect();
        let fitting: Vec<$large> = small.iter().map(|&v| $large::from(v)).collect();
        for (&x, &v) in specs.iter().zip(&fitting) {
            assert_eq!(decoded(v), x.fields(), "{name}: widen {x:?}");
            assert_eq!($small::try_from(v).map(decoded), Ok(x.fields()), "{name}: narrow {x:?}");
        }
        let specs: Vec<Spec> = (0..COUNT).map(|_| $rng.spec(<$large as Number>::DIGITS)).collect();
        let wide: Vec<$large> = specs.iter().map(|&x| x.parse()).collect();
        for (&x, &v) in specs.iter().zip(&wide) {
            let floor = floor_fields(signed(x), x.exp, <$small as Number>::DIGITS, false);
            assert_eq!(
                $small::$rounded(v, Rounding::TowardNegative).map(decoded),
                Ok(floor),
                "{name}: narrow rounded {x:?}"
            );
        }
        per_element($c, &format!("widen_{name}"), &small, |&v| $large::from(v));
        per_element($c, &format!("narrow_{name}"), &fitting, |&v| $small::try_from(v));
        per_element($c, &format!("narrowfloor_{name}"), &wide, |&v| {
            $small::$rounded(v, Rounding::TowardNegative)
        });
    )*};
}

fn finite(c: &mut Criterion) {
    println!("WORKLOAD seed={SEED:#x} count={COUNT}");
    let mut rng = SplitMix64(SEED ^ 0x0f17_17e5);
    division::<Decimal32>(c, &mut rng);
    division::<Decimal64>(c, &mut rng);
    division::<Decimal128>(c, &mut rng);
    conversions!(
        c,
        rng,
        Decimal32 Decimal64 from_d64_rounded,
        Decimal32 Decimal128 from_d128_rounded,
        Decimal64 Decimal128 from_d128_rounded
    );
}

criterion_group! { name = benches; config = Criterion::default()
.warm_up_time(Duration::from_secs(1)).measurement_time(Duration::from_secs(2)).sample_size(100);
targets = finite }
criterion_main!(benches);
