//! Instruction counts under Valgrind Cachegrind of key hashing, key
//! comparison, Decimal128 rounded multiplication and division, and `HashSet`
//! lookups. Each benchmark's input set is generated and checked against an
//! independent reference in its setup, outside the measured function, and the
//! measured function returns its input so that freeing it is not measured.

use bidecimal::{Decimal32, Decimal64, Decimal128, OrderedDecimal, Rounding};
use gungraun::{library_benchmark, library_benchmark_group, main};
use std::{
    collections::HashSet,
    hash::{BuildHasher, Hash},
    hint::black_box,
};

#[expect(
    dead_code,
    reason = "shared with the Criterion benchmarks, which use the rest"
)]
#[path = "../../benches/keyed/mod.rs"]
mod keyed;
use keyed::{Build, COUNT, Number, SEED, SIZES, Spec, SplitMix64, build, decoded};

#[expect(
    dead_code,
    reason = "shared by the benchmarks, each of which uses part of it"
)]
#[path = "../../benches/support/mod.rs"]
mod support;

#[path = "../../benches/reference/mod.rs"]
mod reference;
use reference::{floor_fields, floor_quotient, signed};

type Key<T> = OrderedDecimal<T>;
type Pairs<T> = Vec<(T, T)>;

/// The generator for the benchmark `id`, so that each benchmark's input set
/// does not depend on the others.
fn rng(id: &str) -> SplitMix64 {
    let salt = id.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    });
    SplitMix64(SEED ^ salt)
}

/// `COUNT` keys of one distribution, each checked to hash like the canonical
/// member of its cohort.
fn hashed_keys<T: Number>(distribution: &str) -> (Build, Vec<Key<T>>)
where
    Key<T>: Hash,
{
    let mut rng = rng(&format!("keyhash_{}_{distribution}", T::NAME));
    let specs: Vec<Spec> = (0..COUNT)
        .map(|_| match distribution {
            "random" => rng.spec(T::DIGITS),
            "cohort" => rng.spec(T::DIGITS - 2).form(1 + rng.below(2) as u32),
            "zero" => rng.zero().form(rng.below(3) as u32),
            _ => panic!("unknown distribution {distribution}"),
        })
        .collect();
    let keys: Vec<Key<T>> = specs.iter().map(|&s| OrderedDecimal(s.parse())).collect();
    let build = Build::default();
    for (spec, key) in specs.iter().zip(&keys) {
        let base = Spec {
            neg: spec.neg && spec.coeff != 0,
            coeff: spec.canonical().1,
            exp: spec.canonical().2,
        };
        assert_eq!(
            build.hash_one(key),
            build.hash_one(OrderedDecimal(base.parse::<T>())),
            "{spec:?}"
        );
    }
    (build, keys)
}

/// `COUNT` pairs of two members of one cohort, each checked to compare equal
/// as keys.
fn cohort_pairs<T: Number>() -> Pairs<Key<T>>
where
    Key<T>: Ord,
{
    let mut rng = rng(&format!("keycmp_{}_cohort", T::NAME));
    (0..COUNT)
        .map(|_| {
            let x = rng.spec(T::DIGITS - 2);
            let y = x.form(1 + rng.below(2) as u32);
            let (x, y) = if rng.next() & 1 == 0 { (x, y) } else { (y, x) };
            let (a, b) = (
                OrderedDecimal(x.parse::<T>()),
                OrderedDecimal(y.parse::<T>()),
            );
            assert_eq!(a.cmp(&b), x.value().cmp(&y.value()), "{x:?}, {y:?}");
            (a, b)
        })
        .collect()
}

/// `COUNT` pairs of operands of 1 to 34 digits, each checked to give the fields
/// of the product rounded toward negative.
fn floor_products() -> Pairs<Decimal128> {
    let mut rng = rng("mul_d128_floor");
    let digits = <Decimal128 as Number>::DIGITS;
    (0..COUNT)
        .map(|_| {
            let (x, y) = (rng.spec(digits), rng.spec(digits));
            let (a, b) = (x.parse(), y.parse());
            let product = Decimal128::mul_rounded(a, b, Rounding::TowardNegative)
                .unwrap_or_else(|error| panic!("{x:?}, {y:?}: {error:?}"));
            let expected =
                floor_fields(signed(x) * signed(y), x.exp + y.exp, digits, x.neg != y.neg);
            assert_eq!(decoded(product), expected, "{x:?}, {y:?}");
            (a, b)
        })
        .collect()
}

/// `COUNT` pairs of operands of 1 to 34 digits with an inexact quotient, each
/// checked to give the fields of the quotient rounded toward negative.
fn floor_quotients() -> Pairs<Decimal128> {
    let mut rng = rng("div_d128_floor");
    let digits = <Decimal128 as Number>::DIGITS;
    let mut inputs = Vec::with_capacity(COUNT);
    while inputs.len() < COUNT {
        let (x, y) = (rng.spec(digits), rng.spec(digits));
        let Some(expected) = floor_quotient(x, y, digits) else {
            continue;
        };
        let (a, b) = (x.parse(), y.parse());
        let quotient = Decimal128::div_rounded(a, b, Rounding::TowardNegative)
            .unwrap_or_else(|error| panic!("{x:?}, {y:?}: {error:?}"));
        assert_eq!(decoded(quotient), expected, "{x:?}, {y:?}");
        inputs.push((a, b));
    }
    inputs
}

/// A `HashSet` of `SIZES[0]` keys of distinct values and, for each key, a
/// query that is another member of its cohort, checked to all hit.
fn cohort_lookups<T: Number>() -> (HashSet<Key<T>, Build>, Vec<Key<T>>)
where
    Key<T>: Eq + Hash,
{
    let mut rng = rng(&format!("hashset_{}_get_hit_cohort", T::NAME));
    let mut seen = HashSet::new();
    let (mut keys, mut queries) = (Vec::new(), Vec::new());
    while keys.len() < SIZES[0] {
        let base = rng.spec(T::DIGITS - 2);
        if seen.insert(base.canonical()) {
            let form = rng.below(3) as u32;
            keys.push(OrderedDecimal(base.form(form).parse::<T>()));
            queries.push(OrderedDecimal(base.form((form + 1) % 3).parse::<T>()));
        }
    }
    let set: HashSet<Key<T>, Build> = build(&keys);
    assert_eq!(set.len(), SIZES[0]);
    assert!(queries.iter().all(|query| set.contains(query)));
    (set, queries)
}

macro_rules! width {
    ($t:ident, $keyhash:ident, $keycmp:ident, $lookup:ident) => {
        #[library_benchmark]
        #[bench::random(args = ("random"), setup = hashed_keys::<$t>)]
        #[bench::cohort(args = ("cohort"), setup = hashed_keys::<$t>)]
        #[bench::zero(args = ("zero"), setup = hashed_keys::<$t>)]
        fn $keyhash(input: (Build, Vec<Key<$t>>)) -> (Build, Vec<Key<$t>>) {
            for key in &input.1 {
                black_box(input.0.hash_one(black_box(key)));
            }
            input
        }

        #[library_benchmark]
        #[bench::cohort(setup = cohort_pairs::<$t>)]
        fn $keycmp(pairs: Pairs<Key<$t>>) -> Pairs<Key<$t>> {
            for (a, b) in &pairs {
                black_box(black_box(a).cmp(black_box(b)));
            }
            pairs
        }

        #[library_benchmark]
        #[bench::get_hit_cohort(setup = cohort_lookups::<$t>)]
        fn $lookup(
            input: (HashSet<Key<$t>, Build>, Vec<Key<$t>>),
        ) -> (HashSet<Key<$t>, Build>, Vec<Key<$t>>) {
            for query in &input.1 {
                black_box(input.0.contains(black_box(query)));
            }
            input
        }
    };
}
width!(Decimal32, keyhash_d32, keycmp_d32, hashset_d32);
width!(Decimal64, keyhash_d64, keycmp_d64, hashset_d64);
width!(Decimal128, keyhash_d128, keycmp_d128, hashset_d128);

#[library_benchmark]
#[bench::floor(setup = floor_products)]
fn mul_d128(pairs: Pairs<Decimal128>) -> Pairs<Decimal128> {
    for &(a, b) in &pairs {
        let _ = black_box(Decimal128::mul_rounded(
            black_box(a),
            black_box(b),
            Rounding::TowardNegative,
        ));
    }
    pairs
}

#[library_benchmark]
#[bench::floor(setup = floor_quotients)]
fn div_d128(pairs: Pairs<Decimal128>) -> Pairs<Decimal128> {
    for &(a, b) in &pairs {
        let _ = black_box(Decimal128::div_rounded(
            black_box(a),
            black_box(b),
            Rounding::TowardNegative,
        ));
    }
    pairs
}

library_benchmark_group!(
    name = keys,
    benchmarks = [
        keyhash_d32,
        keyhash_d64,
        keyhash_d128,
        keycmp_d32,
        keycmp_d64,
        keycmp_d128
    ]
);
library_benchmark_group!(name = arithmetic, benchmarks = [mul_d128, div_d128]);
library_benchmark_group!(
    name = containers,
    benchmarks = [hashset_d32, hashset_d64, hashset_d128]
);
main!(library_benchmark_groups = keys, arithmetic, containers);
