use crate::support::{Bid, decode_bid};
use bidecimal::{Decimal32, Decimal64, Decimal128};
use criterion::{BatchSize, Criterion, SamplingMode, Throughput};
use num_bigint::BigInt;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fmt,
    hash::{BuildHasherDefault, DefaultHasher, Hash},
    hint::black_box,
};

pub const COUNT: usize = 8192;
pub const SEED: u64 = 0x243f_6a88_85a3_08d3;
pub const SIZES: [usize; 2] = [1024, 65536];
const OFFSET: i32 = 40;

pub type Build = BuildHasherDefault<DefaultHasher>;

pub struct SplitMix64(pub u64);
impl SplitMix64 {
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut x = self.0;
        x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        x ^ (x >> 31)
    }
    pub fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    pub fn spec(&mut self, max_digits: u32) -> Spec {
        let digits = 1 + self.below(u64::from(max_digits)) as u32;
        let low = 10_u128.pow(digits - 1);
        let bits = (u128::from(self.next()) << 64) | u128::from(self.next());
        Spec {
            neg: self.next() & 1 == 1,
            coeff: low + bits % (9 * low),
            exp: self.below(41) as i32 - 20,
        }
    }
    pub fn zero(&mut self) -> Spec {
        Spec {
            neg: self.next() & 1 == 1,
            coeff: 0,
            exp: self.below(41) as i32 - 20,
        }
    }
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i as u64 + 1) as usize;
            items.swap(i, j);
        }
    }
}

/// A finite value as sign, coefficient, and exponent, independent of the
/// library: `canonical` and `value` are the reference for equality and order.
#[derive(Clone, Copy, Debug)]
pub struct Spec {
    pub neg: bool,
    pub coeff: u128,
    pub exp: i32,
}
impl Spec {
    /// `self` parsed, checked to have exactly its sign, coefficient, and
    /// exponent, so that each benchmark measures the representations its name
    /// says.
    pub fn parse<T: Number>(self) -> T {
        let sign = if self.neg { "-" } else { "" };
        let value = T::parse(&format!("{sign}{}e{}", self.coeff, self.exp));
        assert_eq!(decoded(value), self.fields(), "{self:?}");
        value
    }
    pub fn fields(self) -> Bid {
        Bid {
            negative: self.neg,
            coeff: self.coeff,
            exp: self.exp,
        }
    }
    /// The member of this value's cohort `k` exponent steps lower; for zero,
    /// odd `k` also flips the sign.
    pub fn form(self, k: u32) -> Spec {
        Spec {
            neg: self.neg != (self.coeff == 0 && k % 2 == 1),
            coeff: self.coeff * 10_u128.pow(k),
            exp: self.exp - k as i32,
        }
    }
    pub fn canonical(self) -> (bool, u128, i32) {
        if self.coeff == 0 {
            return (false, 0, 0);
        }
        let (mut coeff, mut exp) = (self.coeff, self.exp);
        while coeff % 10 == 0 {
            coeff /= 10;
            exp += 1;
        }
        (self.neg, coeff, exp)
    }
    pub fn value(self) -> BigInt {
        let magnitude = BigInt::from(self.coeff) * BigInt::from(10).pow((self.exp + OFFSET) as u32);
        if self.neg { -magnitude } else { magnitude }
    }
}

pub trait Number: Copy + PartialEq + PartialOrd + fmt::Debug {
    const NAME: &'static str;
    const DIGITS: u32;
    const WIDTH: u32;
    fn parse(text: &str) -> Self;
    fn bits(self) -> u128;
}
macro_rules! number {
    ($($t:ident $name:literal $digits:literal $width:literal),*) => {$(
        impl Number for $t {
            const NAME: &'static str = $name;
            const DIGITS: u32 = $digits;
            const WIDTH: u32 = $width;
            fn parse(text: &str) -> Self {
                $t::parse(text).unwrap_or_else(|e| panic!("{text}: {e:?}"))
            }
            fn bits(self) -> u128 {
                self.to_bits() as u128
            }
        }
    )*};
}
number!(Decimal32 "d32" 7 32, Decimal64 "d64" 16 64, Decimal128 "d128" 34 128);

/// The sign, coefficient, and exponent of the finite `value`, decoded from its
/// encoding independently of the library.
pub fn decoded<T: Number>(value: T) -> Bid {
    decode_bid(value.bits(), T::WIDTH).unwrap_or_else(|| panic!("not finite: {value:?}"))
}

/// Container key sequences over `n` distinct values, each stored in a random
/// cohort member: `keys` in generation order with `values[i]` the reference
/// value of `keys[i]`, the same keys sorted, reversed, and shuffled, the first
/// `n / 2` values each in two cohort members shuffled, another cohort member of
/// every key, and `n` values outside the keys.
pub struct Workload<T> {
    pub keys: Vec<T>,
    pub values: Vec<BigInt>,
    pub sorted: Vec<T>,
    pub reverse: Vec<T>,
    pub same: Vec<T>,
    pub duplicated: Vec<T>,
    pub cohort: Vec<T>,
    pub missing: Vec<T>,
}
impl<T: Number> Workload<T> {
    pub fn new(rng: &mut SplitMix64, n: usize) -> Self {
        let mut seen = HashSet::new();
        let first = rng.zero();
        seen.insert(first.canonical());
        let mut bases = vec![first];
        while bases.len() < 2 * n {
            let spec = rng.spec(T::DIGITS - 2);
            if seen.insert(spec.canonical()) {
                bases.push(spec);
            }
        }
        let (key_bases, miss_bases) = bases.split_at(n);
        let forms: Vec<u32> = (0..n).map(|_| rng.below(3) as u32).collect();
        let member = |shift: u32| -> Vec<T> {
            key_bases
                .iter()
                .zip(&forms)
                .map(|(base, k)| base.form((k + shift) % 3).parse())
                .collect()
        };
        let keys = member(0);
        let values: Vec<BigInt> = key_bases.iter().map(|base| base.value()).collect();
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by(|&i, &j| values[i].cmp(&values[j]));
        let sorted: Vec<T> = order.iter().map(|&i| keys[i]).collect();
        let reverse = sorted.iter().rev().copied().collect();
        let mut same = keys.clone();
        rng.shuffle(&mut same);
        let mut cohort = member(1);
        let mut duplicated: Vec<T> = keys[..n / 2]
            .iter()
            .chain(&cohort[..n / 2])
            .copied()
            .collect();
        rng.shuffle(&mut duplicated);
        rng.shuffle(&mut cohort);
        let mut missing: Vec<T> = miss_bases
            .iter()
            .map(|base| base.form(rng.below(3) as u32).parse())
            .collect();
        rng.shuffle(&mut missing);
        Self {
            keys,
            values,
            sorted,
            reverse,
            same,
            duplicated,
            cohort,
            missing,
        }
    }
}
impl<T: Copy> Workload<T> {
    pub fn map<K>(&self, f: impl Fn(T) -> K) -> Workload<K> {
        let map = |items: &[T]| items.iter().map(|&v| f(v)).collect();
        Workload {
            keys: map(&self.keys),
            values: self.values.clone(),
            sorted: map(&self.sorted),
            reverse: map(&self.reverse),
            same: map(&self.same),
            duplicated: map(&self.duplicated),
            cohort: map(&self.cohort),
            missing: map(&self.missing),
        }
    }
}

pub trait Container<K>: Clone {
    const NAME: &'static str;
    fn empty() -> Self;
    fn put(&mut self, key: K, index: u32);
    fn find(&self, key: &K) -> Option<u32>;
    fn take(&mut self, key: &K) -> bool;
    fn size(&self) -> usize;
}
impl<K: Ord + Clone> Container<K> for BTreeSet<K> {
    const NAME: &'static str = "btreeset";
    fn empty() -> Self {
        Self::new()
    }
    fn put(&mut self, key: K, _: u32) {
        self.insert(key);
    }
    fn find(&self, key: &K) -> Option<u32> {
        self.contains(key).then_some(0)
    }
    fn take(&mut self, key: &K) -> bool {
        self.remove(key)
    }
    fn size(&self) -> usize {
        self.len()
    }
}
impl<K: Ord + Clone> Container<K> for BTreeMap<K, u32> {
    const NAME: &'static str = "btreemap";
    fn empty() -> Self {
        Self::new()
    }
    fn put(&mut self, key: K, index: u32) {
        self.insert(key, index);
    }
    fn find(&self, key: &K) -> Option<u32> {
        self.get(key).copied()
    }
    fn take(&mut self, key: &K) -> bool {
        self.remove(key).is_some()
    }
    fn size(&self) -> usize {
        self.len()
    }
}
impl<K: Eq + Hash + Clone> Container<K> for HashSet<K, Build> {
    const NAME: &'static str = "hashset";
    fn empty() -> Self {
        Self::with_hasher(Build::default())
    }
    fn put(&mut self, key: K, _: u32) {
        self.insert(key);
    }
    fn find(&self, key: &K) -> Option<u32> {
        self.contains(key).then_some(0)
    }
    fn take(&mut self, key: &K) -> bool {
        self.remove(key)
    }
    fn size(&self) -> usize {
        self.len()
    }
}
impl<K: Eq + Hash + Clone> Container<K> for HashMap<K, u32, Build> {
    const NAME: &'static str = "hashmap";
    fn empty() -> Self {
        Self::with_hasher(Build::default())
    }
    fn put(&mut self, key: K, index: u32) {
        self.insert(key, index);
    }
    fn find(&self, key: &K) -> Option<u32> {
        self.get(key).copied()
    }
    fn take(&mut self, key: &K) -> bool {
        self.remove(key).is_some()
    }
    fn size(&self) -> usize {
        self.len()
    }
}

pub fn build<K: Copy, C: Container<K>>(sequence: &[K]) -> C {
    let mut container = C::empty();
    for (i, &key) in sequence.iter().enumerate() {
        container.put(key, i as u32);
    }
    container
}

pub fn per_element<I, O>(c: &mut Criterion, name: &str, inputs: &[I], op: impl Fn(&I) -> O) {
    let mut group = c.benchmark_group(name);
    group.sampling_mode(SamplingMode::Flat);
    group.throughput(Throughput::Elements(inputs.len() as u64));
    group.bench_function("set", |bencher| {
        bencher.iter(|| {
            for input in inputs {
                black_box(op(black_box(input)));
            }
        })
    });
    group.finish();
}

/// Checks the container against the reference counts, then measures building
/// it from empty in each key order, lookups of the same bits, of other cohort
/// members, and of absent values, and removal of every key.
pub fn container<K: Copy, C: Container<K>>(c: &mut Criterion, width: &str, w: &Workload<K>) {
    let n = w.keys.len();
    let name = format!("{}_{width}_n{n}", C::NAME);
    for sequence in [&w.sorted, &w.reverse, &w.same] {
        assert_eq!(build::<K, C>(sequence).size(), n, "{name}");
    }
    assert_eq!(
        build::<K, C>(&w.duplicated).size(),
        n / 2,
        "{name}: duplicated"
    );
    let full: C = build(&w.keys);
    assert!(
        w.same.iter().all(|q| full.find(q).is_some()),
        "{name}: same"
    );
    assert!(
        w.cohort.iter().all(|q| full.find(q).is_some()),
        "{name}: cohort"
    );
    assert!(
        w.missing.iter().all(|q| full.find(q).is_none()),
        "{name}: missing"
    );
    let mut emptied = full.clone();
    assert!(w.same.iter().all(|q| emptied.take(q)), "{name}: remove");
    assert_eq!(emptied.size(), 0, "{name}: removed");

    let mut group = c.benchmark_group(&name);
    group.sampling_mode(SamplingMode::Flat);
    group.throughput(Throughput::Elements(n as u64));
    for (id, sequence) in [
        ("insert_sorted", &w.sorted),
        ("insert_reverse", &w.reverse),
        ("insert_random", &w.same),
        ("insert_dup50", &w.duplicated),
    ] {
        group.bench_function(id, |bencher| {
            bencher.iter(|| build::<K, C>(black_box(sequence)).size())
        });
    }
    for (id, queries) in [
        ("get_hit_same", &w.same),
        ("get_hit_cohort", &w.cohort),
        ("get_miss", &w.missing),
    ] {
        group.bench_function(id, |bencher| {
            bencher.iter(|| {
                for query in queries {
                    black_box(full.find(black_box(query)));
                }
            })
        });
    }
    group.bench_function("remove", |bencher| {
        bencher.iter_batched_ref(
            || full.clone(),
            |container| {
                for query in &w.same {
                    black_box(container.take(black_box(query)));
                }
            },
            BatchSize::LargeInput,
        )
    });
    group.finish();
}

/// Checks that a map built from `keys` with index values iterates in strictly
/// increasing reference value.
pub fn assert_ordered<K: Ord + Copy>(name: &str, w: &Workload<K>) {
    let map: BTreeMap<K, u32> = build(&w.keys);
    let indices: Vec<usize> = map.values().map(|&i| i as usize).collect();
    assert!(
        indices.windows(2).all(|p| w.values[p[0]] < w.values[p[1]]),
        "{name}: order"
    );
}
