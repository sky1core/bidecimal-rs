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
            "same_exp_distinct" => {
                let x = rng.spec(digits);
                let mut y = rng.spec(digits);
                y.exp = x.exp;
                if x.coeff == y.coeff && x.neg == y.neg {
                    y.neg = !y.neg;
                }
                (x, y)
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
    for distribution in ["same", "same_exp_distinct", "cohort", "zero", "random"] {
        // Isolate the new fixture so existing benchmark inputs remain unchanged.
        let specs = if distribution == "same_exp_distinct" {
            let mut fixture_rng = SplitMix64(SEED ^ 0x7361_6d65_6578_7000 ^ u64::from(T::DIGITS));
            pairs(&mut fixture_rng, distribution, T::DIGITS)
        } else {
            pairs(rng, distribution, T::DIGITS)
        };
        let values: Vec<(T, T)> = specs.iter().map(|(x, y)| (x.parse(), y.parse())).collect();
        let keys: Vec<(Key<T>, Key<T>)> = values.iter().map(|&(a, b)| (key(a), key(b))).collect();
        for ((x, y), &(a, b)) in specs.iter().zip(&values) {
            let order = x.value().cmp(&y.value());
            if distribution == "same_exp_distinct" {
                assert_eq!(x.exp, y.exp);
                assert_ne!(order, Ordering::Equal);
            }
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

fn comparison32(c: &mut Criterion) {
    let mut rng = SplitMix64(SEED);
    for distribution in [
        "fixed_exp",
        "smallexp_pos",
        "cohort_oneway",
        "widegap",
        "fixed_exp_pos",
        "random_pos",
        "widegap_pos",
        "value_zero",
    ] {
        let specs: Vec<(Spec, Spec)> = (0..COUNT)
            .map(|_| {
                if distribution == "value_zero" {
                    let mut a = rng.spec(7);
                    a.neg = rng.below(8) == 0;
                    let b = rng.zero();
                    return if rng.next() & 1 == 0 { (a, b) } else { (b, a) };
                }
                if distribution == "cohort_oneway" {
                    let a = rng.spec(5);
                    return (a, a.form(1 + rng.below(2) as u32));
                }
                let mut a = rng.spec(7);
                let mut b = rng.spec(7);
                match distribution {
                    "fixed_exp" | "fixed_exp_pos" => {
                        a.exp = -4;
                        b.exp = -4;
                    }
                    "smallexp_pos" => {
                        a.exp = -(rng.below(5) as i32);
                        b.exp = -(rng.below(5) as i32);
                        a.neg = false;
                        b.neg = false;
                    }
                    "widegap" | "widegap_pos" => {
                        a.exp = rng.below(111) as i32 - 20;
                        b.exp = rng.below(111) as i32 - 20;
                    }
                    "random_pos" => {}
                    _ => unreachable!(),
                }
                if matches!(distribution, "fixed_exp_pos" | "random_pos" | "widegap_pos") {
                    a.neg = false;
                    b.neg = false;
                }
                (a, b)
            })
            .collect();
        let values: Vec<(Decimal32, Decimal32)> =
            specs.iter().map(|(a, b)| (a.parse(), b.parse())).collect();
        let keys: Vec<_> = values.iter().map(|&(a, b)| (key(a), key(b))).collect();
        for ((a, b), &(x, y)) in specs.iter().zip(&values) {
            let order = a.value().cmp(&b.value());
            assert_eq!(x == y, order == Ordering::Equal);
            assert_eq!(x.partial_cmp(&y), Some(order));
            assert_eq!(key(x).cmp(&key(y)), order);
        }
        per_element(c, &format!("eq_d32_{distribution}"), &values, |(a, b)| {
            a == b
        });
        per_element(c, &format!("cmp_d32_{distribution}"), &values, |(a, b)| {
            a.partial_cmp(b)
        });
        per_element(c, &format!("keycmp_d32_{distribution}"), &keys, |(a, b)| {
            a.cmp(b)
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
    comparison32(c);
}

criterion_group! { name = benches; config = Criterion::default()
.warm_up_time(Duration::from_millis(500)).measurement_time(Duration::from_secs(1)).sample_size(100)
.nresamples(2000);
targets = keys }
criterion_main!(benches);
