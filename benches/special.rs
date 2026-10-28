use bidecimal::{Decimal32, Decimal64, Decimal128, Error, OrderedDecimal};
use criterion::{Criterion, criterion_group, criterion_main};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    hash::{BuildHasher, Hash},
    time::Duration,
};

#[expect(
    dead_code,
    reason = "shared with the keys benchmark, which uses the rest"
)]
mod keyed;
use keyed::{Build, COUNT, Container, Number, SEED, SplitMix64, Workload, build, per_element};

#[expect(
    dead_code,
    reason = "shared by the benchmarks, each of which uses part of it"
)]
mod support;

const CONTAINER_SIZE: usize = 1024;
const CONTAINER_SPECIALS: usize = 72;
/// Name, percentage of operand slots holding NaN or an infinity, and whether
/// those slots form one leading block instead of being scattered.
const MIXES: [(&str, u64, bool); 6] = [
    ("p0", 0, false),
    ("p1", 1, false),
    ("p10", 10, false),
    ("p50", 50, false),
    ("p100", 100, false),
    ("p10_block", 10, true),
];

trait Special: Number {
    fn specials() -> Vec<Self>;
    fn sum(self, other: Self) -> Result<Self, Error>;
    fn product(self, other: Self) -> Result<Self, Error>;
    fn quotient(self, other: Self) -> Result<Self, Error>;
    fn nan(self) -> bool;
    fn finite(self) -> bool;
    fn infinite(self) -> bool;
    fn sign_negative(self) -> bool;
    /// NaN, the signed infinity, the signed zero, or ±1 for other finite values.
    fn image(self) -> f64;
}
macro_rules! special {
    ($($t:ident $bits:ty),*) => {$(
        impl Special for $t {
            fn specials() -> Vec<Self> {
                let nan: $bits = 0b11111 << (<$bits>::BITS - 6);
                let signaling: $bits = 1 << (<$bits>::BITS - 7);
                let sign: $bits = 1 << (<$bits>::BITS - 1);
                [nan, sign | nan, nan | signaling, nan | 123, sign | nan | signaling | 7]
                    .into_iter()
                    .map(|bits| $t::from_bits(bits).unwrap())
                    .chain([$t::INFINITY, $t::NEG_INFINITY])
                    .collect()
            }
            fn sum(self, other: Self) -> Result<Self, Error> {
                $t::add(self, other)
            }
            fn product(self, other: Self) -> Result<Self, Error> {
                $t::mul(self, other)
            }
            fn quotient(self, other: Self) -> Result<Self, Error> {
                $t::div(self, other)
            }
            fn nan(self) -> bool {
                self.is_nan()
            }
            fn finite(self) -> bool {
                self.is_finite()
            }
            fn infinite(self) -> bool {
                self.is_infinite()
            }
            fn sign_negative(self) -> bool {
                self.is_sign_negative()
            }
            fn image(self) -> f64 {
                let image = if self.is_nan() {
                    return f64::NAN;
                } else if self.is_infinite() {
                    f64::INFINITY
                } else if self.is_zero() {
                    0.0
                } else {
                    1.0
                };
                if self.is_sign_negative() { -image } else { image }
            }
        }
    )*};
}
special!(Decimal32 u32, Decimal64 u64, Decimal128 u128);

fn class(x: f64) -> (u8, bool) {
    let kind = if x.is_nan() {
        return (0, false);
    } else if x.is_infinite() {
        1
    } else if x == 0.0 {
        2
    } else {
        3
    };
    (kind, x.is_sign_negative())
}

fn result_class<T: Special>(result: &Result<T, Error>) -> (u8, bool) {
    class(
        result
            .as_ref()
            .map_or_else(|e| panic!("{e:?}"), |v| v.image()),
    )
}

fn rank(x: f64) -> u8 {
    if x.is_nan() {
        3
    } else if x == f64::INFINITY {
        2
    } else if x == f64::NEG_INFINITY {
        0
    } else {
        1
    }
}

/// Checks every pair with a replaced operand against binary64 on the operand
/// images, then measures.
fn mix<T: Special>(c: &mut Criterion, rng: &mut SplitMix64)
where
    OrderedDecimal<T>: Ord + Hash,
{
    let finite: Vec<(T, T)> = (0..COUNT)
        .map(|_| (rng.spec(T::DIGITS).parse(), rng.spec(T::DIGITS).parse()))
        .collect();
    let draws: Vec<u64> = (0..2 * COUNT).map(|_| rng.next()).collect();
    let specials = T::specials();
    let build = Build::default();
    let hash = |v: T| build.hash_one(OrderedDecimal(v));
    for (name, percent, block) in MIXES {
        let slot = |i: usize, v: T| {
            let replaced = if block {
                (i as u64) * 100 < 2 * COUNT as u64 * percent
            } else {
                draws[i] % 100 < percent
            };
            if replaced {
                specials[((draws[i] >> 32) % specials.len() as u64) as usize]
            } else {
                v
            }
        };
        let values: Vec<(T, T)> = finite
            .iter()
            .enumerate()
            .map(|(i, &(a, b))| (slot(2 * i, a), slot(2 * i + 1, b)))
            .collect();
        for &(a, b) in &values {
            if a.finite() && b.finite() {
                continue;
            }
            let (ia, ib) = (a.image(), b.image());
            assert_eq!(a == b, ia == ib, "{a:?} == {b:?}");
            assert_eq!(a.partial_cmp(&b), ia.partial_cmp(&ib), "{a:?} <=> {b:?}");
            assert_eq!(result_class(&a.sum(b)), class(ia + ib), "{a:?} + {b:?}");
            assert_eq!(result_class(&a.product(b)), class(ia * ib), "{a:?} * {b:?}");
            assert_eq!(
                result_class(&b.quotient(a)),
                class(ib / ia),
                "{b:?} / {a:?}"
            );
            assert_eq!(
                OrderedDecimal(a).cmp(&OrderedDecimal(b)),
                rank(ia).cmp(&rank(ib)),
                "key {a:?} <=> {b:?}"
            );
            if a.nan() {
                assert_eq!(hash(a), hash(specials[0]), "hash {a:?}");
            }
        }
        let keys: Vec<(OrderedDecimal<T>, OrderedDecimal<T>)> = values
            .iter()
            .map(|&(a, b)| (OrderedDecimal(a), OrderedDecimal(b)))
            .collect();
        let suffix = format!("{}_{name}", T::NAME);
        per_element(c, &format!("mix_eq_{suffix}"), &values, |(a, b)| a == b);
        per_element(c, &format!("mix_cmp_{suffix}"), &values, |(a, b)| {
            a.partial_cmp(b)
        });
        per_element(c, &format!("mix_add_{suffix}"), &values, |&(a, b)| a.sum(b));
        per_element(c, &format!("mix_mul_{suffix}"), &values, |&(a, b)| {
            a.product(b)
        });
        per_element(c, &format!("mix_div_{suffix}"), &values, |&(a, b)| {
            b.quotient(a)
        });
        per_element(c, &format!("mix_keycmp_{suffix}"), &keys, |(a, b)| a.cmp(b));
        per_element(c, &format!("mix_keyhash_{suffix}"), &keys, |(a, _)| {
            build.hash_one(a)
        });
    }
}

/// Finite keys in random cohort members plus every NaN and infinity encoding
/// of `Special::specials`, shuffled: one key per finite value, one per signed
/// infinity, and one for all NaNs.
fn special_container<T: Special, C: Container<OrderedDecimal<T>>>(
    c: &mut Criterion,
    sequence: &[OrderedDecimal<T>],
) where
    OrderedDecimal<T>: Ord,
{
    let name = format!("special_{}_{}", C::NAME, T::NAME);
    let full: C = build(sequence);
    assert_eq!(full.size(), CONTAINER_SIZE + 3, "{name}");
    assert!(
        sequence.iter().all(|q| full.find(q).is_some()),
        "{name}: hit"
    );
    let mut group = c.benchmark_group(&name);
    group.sampling_mode(criterion::SamplingMode::Flat);
    group.throughput(criterion::Throughput::Elements(sequence.len() as u64));
    group.bench_function("insert_random", |bencher| {
        bencher.iter(|| build::<_, C>(std::hint::black_box(sequence)).size())
    });
    group.bench_function("get_hit", |bencher| {
        bencher.iter(|| {
            for query in sequence {
                std::hint::black_box(full.find(std::hint::black_box(query)));
            }
        })
    });
    group.finish();
}

fn width<T: Special>(c: &mut Criterion, rng: &mut SplitMix64)
where
    OrderedDecimal<T>: Ord + Hash + Copy,
{
    mix::<T>(c, rng);
    let w = Workload::<T>::new(rng, CONTAINER_SIZE);
    let specials = T::specials();
    let mut sequence: Vec<OrderedDecimal<T>> = w
        .same
        .iter()
        .copied()
        .chain((0..CONTAINER_SPECIALS).map(|i| specials[i % specials.len()]))
        .map(OrderedDecimal)
        .collect();
    rng.shuffle(&mut sequence);
    let map: BTreeMap<OrderedDecimal<T>, u32> = build(&sequence);
    let first = map.keys().next().map(|k| k.0);
    let last = map.keys().next_back().map(|k| k.0);
    assert!(
        first.is_some_and(|v| v.infinite() && v.sign_negative()),
        "first {first:?}"
    );
    assert!(last.is_some_and(|v| v.nan()), "last {last:?}");
    let finite = map.keys().filter(|k| k.0.finite()).count();
    assert_eq!(finite, CONTAINER_SIZE, "finite keys");
    special_container::<T, BTreeSet<_>>(c, &sequence);
    special_container::<T, BTreeMap<_, u32>>(c, &sequence);
    special_container::<T, HashSet<_, Build>>(c, &sequence);
    special_container::<T, HashMap<_, u32, Build>>(c, &sequence);
}

fn special(c: &mut Criterion) {
    println!(
        "WORKLOAD seed={SEED:#x} count={COUNT} container={CONTAINER_SIZE}+{CONTAINER_SPECIALS}"
    );
    let mut rng = SplitMix64(SEED ^ 0x05be_c1a1);
    width::<Decimal32>(c, &mut rng);
    width::<Decimal64>(c, &mut rng);
    width::<Decimal128>(c, &mut rng);
}

criterion_group! { name = benches; config = Criterion::default()
.warm_up_time(Duration::from_secs(1)).measurement_time(Duration::from_secs(2)).sample_size(100);
targets = special }
criterion_main!(benches);
