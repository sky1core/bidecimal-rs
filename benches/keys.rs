use bidecimal::{Decimal32, Decimal64, Decimal128, OrderedDecimal};
use criterion::{Criterion, criterion_group, criterion_main};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, HashSet},
    hash::{BuildHasher, Hash},
    time::Duration,
};

mod keyed;
use keyed::{
    Build, COUNT, Number, SEED, SIZES, Spec, SplitMix64, Workload, assert_ordered, container,
    per_element,
};

#[expect(
    dead_code,
    reason = "shared by the benchmarks, each of which uses part of it"
)]
mod support;

type Key<T> = OrderedDecimal<T>;
fn key<T>(value: T) -> Key<T> {
    OrderedDecimal(value)
}

fn pairs(rng: &mut SplitMix64, distribution: &str, digits: u32) -> Vec<(Spec, Spec)> {
    (0..COUNT)
        .map(|_| match distribution {
            "same" => {
                let x = rng.spec(digits - 2);
                (x, x)
            }
            "cohort" => {
                let x = rng.spec(digits - 2);
                let y = x.form(1 + rng.below(2) as u32);
                if rng.next() & 1 == 0 { (x, y) } else { (y, x) }
            }
            "zero" => (rng.zero(), rng.zero()),
            _ => (rng.spec(digits), rng.spec(digits)),
        })
        .collect()
}

fn width<T: Number>(c: &mut Criterion, rng: &mut SplitMix64)
where
    Key<T>: Ord + Hash + Copy,
{
    for distribution in ["same", "cohort", "zero", "random"] {
        let specs = pairs(rng, distribution, T::DIGITS);
        let values: Vec<(T, T)> = specs.iter().map(|(x, y)| (x.parse(), y.parse())).collect();
        let keys: Vec<(Key<T>, Key<T>)> = values.iter().map(|&(a, b)| (key(a), key(b))).collect();
        for ((x, y), &(a, b)) in specs.iter().zip(&values) {
            let order = x.value().cmp(&y.value());
            assert_eq!(a == b, order == Ordering::Equal, "{a:?} == {b:?}");
            assert_eq!(a.partial_cmp(&b), Some(order), "{a:?} <=> {b:?}");
            assert_eq!(key(a).cmp(&key(b)), order, "key {a:?} <=> {b:?}");
        }
        let suffix = format!("{}_{distribution}", T::NAME);
        per_element(c, &format!("eq_{suffix}"), &values, |(a, b)| a == b);
        per_element(c, &format!("cmp_{suffix}"), &values, |(a, b)| {
            a.partial_cmp(b)
        });
        per_element(c, &format!("keycmp_{suffix}"), &keys, |(a, b)| a.cmp(b));
    }
    let build = Build::default();
    for distribution in ["random", "cohort", "zero"] {
        let specs: Vec<Spec> = (0..COUNT)
            .map(|_| match distribution {
                "random" => rng.spec(T::DIGITS),
                "cohort" => rng.spec(T::DIGITS - 2).form(1 + rng.below(2) as u32),
                _ => rng.zero().form(rng.below(3) as u32),
            })
            .collect();
        let keys: Vec<Key<T>> = specs.iter().map(|s| key(s.parse())).collect();
        for (spec, k) in specs.iter().zip(&keys) {
            let base = Spec {
                neg: spec.neg && spec.coeff != 0,
                coeff: spec.canonical().1,
                exp: spec.canonical().2,
            };
            assert_eq!(
                build.hash_one(k),
                build.hash_one(key(base.parse::<T>())),
                "{spec:?}"
            );
        }
        per_element(
            c,
            &format!("keyhash_{}_{distribution}", T::NAME),
            &keys,
            |k| build.hash_one(k),
        );
    }
    for n in SIZES {
        let w = Workload::<T>::new(rng, n).map(key);
        assert_ordered(T::NAME, &w);
        container::<_, BTreeMap<_, u32>>(c, T::NAME, &w);
        container::<_, HashSet<_, Build>>(c, T::NAME, &w);
    }
}

fn cross<S: Number + PartialEq<L> + PartialOrd<L>, L: Number>(
    c: &mut Criterion,
    rng: &mut SplitMix64,
) {
    for distribution in ["cohort", "random"] {
        let specs = pairs(rng, distribution, S::DIGITS);
        let values: Vec<(S, L)> = specs.iter().map(|(x, y)| (x.parse(), y.parse())).collect();
        for ((x, y), &(a, b)) in specs.iter().zip(&values) {
            let order = x.value().cmp(&y.value());
            assert_eq!(a == b, order == Ordering::Equal, "{a:?} == {b:?}");
            assert_eq!(a.partial_cmp(&b), Some(order), "{a:?} <=> {b:?}");
        }
        let suffix = format!("{}_{}_{distribution}", S::NAME, L::NAME);
        per_element(c, &format!("eq_{suffix}"), &values, |(a, b)| a == b);
        per_element(c, &format!("cmp_{suffix}"), &values, |(a, b)| {
            a.partial_cmp(b)
        });
    }
}

fn keys(c: &mut Criterion) {
    println!("WORKLOAD seed={SEED:#x} count={COUNT} sizes={SIZES:?}");
    let mut rng = SplitMix64(SEED);
    width::<Decimal32>(c, &mut rng);
    width::<Decimal64>(c, &mut rng);
    width::<Decimal128>(c, &mut rng);
    cross::<Decimal32, Decimal64>(c, &mut rng);
    cross::<Decimal32, Decimal128>(c, &mut rng);
    cross::<Decimal64, Decimal128>(c, &mut rng);
}

criterion_group! { name = benches; config = Criterion::default()
.warm_up_time(Duration::from_millis(500)).measurement_time(Duration::from_secs(1)).sample_size(100)
.nresamples(2000);
targets = keys }
criterion_main!(benches);
