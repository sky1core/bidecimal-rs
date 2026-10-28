use bidecimal::{Decimal32, Decimal64, Decimal128, Rounding};
use core::fmt::{self, Write};
use criterion::{Criterion, SamplingMode, Throughput, criterion_group, criterion_main};
use rust_decimal::{Decimal, RoundingStrategy};
use std::{
    collections::VecDeque,
    hint::black_box,
    str::FromStr,
    time::{Duration, Instant},
};

pub(crate) mod support;
use support::{Exact, Numeric, numeric_equal, preflight};

impl Numeric for rust_decimal::Decimal {
    fn exact(&self) -> Option<Exact> {
        Some(Exact::new(
            num_bigint::BigInt::from(self.mantissa()),
            -(self.scale() as i32),
        ))
    }
}
const COUNT: usize = 4096;
const SEED: u64 = 0x243f_6a88_85a3_08d3;

struct SplitMix64(u64);
impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut x = self.0;
        x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        x ^ (x >> 31)
    }
    fn value(&mut self, min_digits: u32, max_digits: u32, scale: u32) -> String {
        let digits = min_digits + (self.next() % u64::from(max_digits - min_digits + 1)) as u32;
        let low = 10_u128.pow(digits - 1);
        let bits = (u128::from(self.next()) << 64) | u128::from(self.next());
        let coeff = low + bits % (9 * low);
        let sign = if self.next() & 1 == 0 { "" } else { "-" };
        let text = format!("{coeff:0width$}", width = scale as usize + 1);
        let split = text.len() - scale as usize;
        if scale == 0 {
            format!("{sign}{text}")
        } else {
            format!("{sign}{}.{}", &text[..split], &text[split..])
        }
    }
}

#[derive(Clone, Debug)]
struct Input {
    a: String,
    b: String,
    d: String,
    e: String,
    compare: String,
    compare_left: String,
    c: String,
    b_wide: String,
    step: String,
}

fn inputs() -> Vec<Input> {
    let mut rng = SplitMix64(SEED);
    (0..COUNT * 2)
        .map(|i| {
            let wide = i >= COUNT;
            let a_scale = (rng.next() % 5) as u32;
            let a = rng.value(if wide { 7 } else { 1 }, 7, a_scale);
            let b = if i % 32 == 0 {
                "0.00000000".to_owned()
            } else {
                rng.value(1, 16, 8)
            };
            let (d, e) = loop {
                let a = rng.value(1, 16, 8);
                let b = rng.value(1, 16, 8);
                let magnitude = |s: &str| s.replace('.', "").parse::<i128>().unwrap().abs();
                if magnitude(&a) + magnitude(&b) <= 9_999_999_999_999_999 {
                    break (a, b);
                }
            };
            let compare_left = if i % 4 == 2 {
                if a.contains('.') {
                    a.clone()
                } else {
                    format!("{a}.0")
                }
            } else {
                b.clone()
            };
            let compare = match i % 4 {
                0 => b.clone(),
                1 => a.clone(),
                2 => format!("{compare_left}0"),
                _ => d.clone(),
            };
            let scale = if i % 2 == 0 { 8 } else { 12 };
            let c = if wide {
                rng.value(29, 34, scale)
            } else {
                rng.value(1, 28, scale)
            };
            let b_wide = if wide {
                rng.value(23, 27, 8)
            } else {
                rng.value(1, 21, 8)
            };
            let step_scale = (rng.next() % 5) as usize;
            let unit = if rng.next() & 1 == 0 { "1" } else { "5" };
            let step = if step_scale == 0 {
                unit.to_owned()
            } else {
                format!("0.{}{unit}", "0".repeat(step_scale - 1))
            };
            Input {
                a,
                b,
                d,
                e,
                compare,
                compare_left,
                c,
                b_wide,
                step,
            }
        })
        .collect()
}

struct StackBuffer {
    bytes: [u8; 128],
    len: usize,
}
impl StackBuffer {
    fn new() -> Self {
        Self {
            bytes: [0; 128],
            len: 0,
        }
    }
}
impl Write for StackBuffer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let end = self.len.checked_add(s.len()).ok_or(fmt::Error)?;
        self.bytes
            .get_mut(self.len..end)
            .ok_or(fmt::Error)?
            .copy_from_slice(s.as_bytes());
        self.len = end;
        Ok(())
    }
}
fn format_stack(value: impl fmt::Display) -> (StackBuffer, fmt::Result) {
    let mut buffer = StackBuffer::new();
    let result = write!(&mut buffer, "{value}");
    (buffer, result)
}

fn time_set<T, O>(inputs: &[T], operation: &impl Fn(&T) -> O) -> Duration {
    let start = Instant::now();
    for input in inputs {
        black_box(operation(black_box(input)));
    }
    start.elapsed()
}

#[expect(
    clippy::too_many_arguments,
    reason = "typed preparation and operations are separate from exact partition and preflight"
)]
fn paired<A, B, O, P>(
    c: &mut Criterion,
    name: &str,
    source: &[Input],
    prepare: impl Fn(&Input) -> A,
    prepare_base: impl Fn(&Input) -> B,
    ours: impl Fn(&A) -> O,
    baseline: impl Fn(&B) -> P,
    equal: impl Fn(&O, &P) -> bool,
    eligible: impl Fn(&Input) -> bool,
    verified: impl Fn(&Input, &O) -> bool,
) {
    let mut a = Vec::new();
    let mut b = Vec::new();
    let mut outside = Vec::new();
    let mut mismatches = Vec::new();
    for input in source {
        let left = prepare(input);
        let x = ours(&left);
        if !eligible(input) {
            assert!(
                verified(input, &x),
                "{name}: invalid outside result for {input:?}"
            );
            outside.push(left);
            continue;
        }
        let right = prepare_base(input);
        if !preflight(
            &format!("{name}: {input:?}"),
            &x,
            || baseline(&right),
            |x| verified(input, x),
            |x, y| equal(x, y),
        ) {
            mismatches.push(format!("{input:?}"));
            continue;
        }
        a.push(left);
        b.push(right);
    }
    if let Some(directory) = std::env::var_os("PERF_REMOVALS_DIR") {
        std::fs::write(
            std::path::Path::new(&directory).join(format!("{name}.txt")),
            mismatches.join("\n"),
        )
        .unwrap();
    }
    for input in &mismatches {
        println!("REMOVED {name}: {input}");
    }
    println!(
        "CHECK {name}: generated={} inside={} comparison={} standalone={} differing={}",
        source.len(),
        a.len() + mismatches.len(),
        a.len(),
        outside.len(),
        mismatches.len()
    );
    assert_eq!(a.len() + outside.len() + mismatches.len(), source.len());
    assert!(!a.is_empty());
    let mut group = c.benchmark_group(name);
    group.throughput(Throughput::Elements(a.len() as u64));
    group.sampling_mode(SamplingMode::Flat);
    let baseline_first = std::env::var("PERF_BASELINE_FIRST").is_ok_and(|v| v == "1");
    let mut samples = VecDeque::with_capacity(100);
    group.bench_function("paired", |bencher| {
        bencher.iter_custom(|iterations| {
            let mut ours_time = Duration::ZERO;
            let mut baseline_time = Duration::ZERO;
            let mut baseline_next = baseline_first;
            for _ in 0..iterations {
                if baseline_next {
                    baseline_time += time_set(&b, &baseline);
                    ours_time += time_set(&a, &ours);
                } else {
                    ours_time += time_set(&a, &ours);
                    baseline_time += time_set(&b, &baseline);
                }
                baseline_next = !baseline_next;
            }
            if samples.len() == 100 {
                samples.pop_front();
            }
            samples.push_back((iterations, ours_time.as_nanos(), baseline_time.as_nanos()));
            ours_time + baseline_time
        });
    });
    group.finish();
    if let Some(directory) = std::env::var_os("PERF_RAW_DIR") {
        let column = |index| {
            samples
                .iter()
                .map(|&(iters, ours_ns, baseline_ns)| match index {
                    0 => u128::from(iters).to_string(),
                    1 => ours_ns.to_string(),
                    2 => baseline_ns.to_string(),
                    _ => unreachable!(),
                })
                .collect::<Vec<_>>()
                .join(",")
        };
        std::fs::write(
            std::path::Path::new(&directory).join(format!("{name}.json")),
            format!(
                "{{\"iterations\":[{}],\"bidecimal_ns\":[{}],\"rust_decimal_ns\":[{}]}}",
                column(0),
                column(1),
                column(2)
            ),
        )
        .unwrap();
    }
    if !outside.is_empty() {
        let mut group = c.benchmark_group(format!("wide_{name}"));
        group.sampling_mode(SamplingMode::Flat);
        group.throughput(Throughput::Elements(outside.len() as u64));
        group.bench_function("bidecimal", |bencher| {
            bencher.iter(|| {
                for input in &outside {
                    black_box(ours(black_box(input)));
                }
            })
        });
        group.finish();
    }
}

fn paths(c: &mut Criterion) {
    let data = inputs();
    println!("WORKLOAD seed={SEED:#x} count={}", data.len());
    macro_rules! numeric {
        ($name:expr, [$($field:ident),+], $expected:expr, $prepare:expr, $prepare_base:expr, $ours:expr, $base:expr) => {
            paired(c, $name, &data, $prepare, $prepare_base, $ours, $base,
                |a, b| numeric_equal(a, b),
                |input| $(Exact::parse(&input.$field).inside() &&)+ ($expected)(input).inside(),
                |input, output| output.exact().is_some_and(|v| v == ($expected)(input)));
        };
    }
    numeric!(
        "parse32",
        [a],
        |x: &Input| Exact::parse(&x.a),
        |x| x.a.clone(),
        |x| x.a.clone(),
        |s| Decimal32::parse(s),
        |s| Decimal::from_str(s)
    );
    numeric!(
        "parse64",
        [b],
        |x: &Input| Exact::parse(&x.b),
        |x| x.b.clone(),
        |x| x.b.clone(),
        |s| Decimal64::parse(s),
        |s| Decimal::from_str(s)
    );
    numeric!(
        "parse128",
        [c],
        |x: &Input| Exact::parse(&x.c),
        |x| x.c.clone(),
        |x| x.c.clone(),
        |s| Decimal128::parse(s),
        |s| Decimal::from_str(s)
    );
    macro_rules! formatting {
        ($name:expr, $ty:ty, $field:ident) => {
            paired(
                c,
                $name,
                &data,
                |x| <$ty>::parse(&x.$field).unwrap(),
                |x| Decimal::from_str(&x.$field).unwrap(),
                |&x| format_stack(x),
                |&x| format_stack(x),
                |a, b| {
                    a.1.is_ok()
                        && b.1.is_ok()
                        && Exact::parse(std::str::from_utf8(&a.0.bytes[..a.0.len]).unwrap())
                            == Exact::parse(std::str::from_utf8(&b.0.bytes[..b.0.len]).unwrap())
                },
                |x| Exact::parse(&x.$field).inside(),
                |x, output| {
                    output.1.is_ok()
                        && Exact::parse(
                            std::str::from_utf8(&output.0.bytes[..output.0.len]).unwrap(),
                        ) == Exact::parse(&x.$field)
                },
            );
        };
    }
    formatting!("format32", Decimal32, a);
    formatting!("format64", Decimal64, b);
    formatting!("format128", Decimal128, c);
    macro_rules! addsub {
        ($name:expr, $ty:ty, $right:ty, $field:ident, $op:ident, $checked:ident) => {
            numeric!(
                $name,
                [d, $field],
                |x: &Input| Exact::parse(&x.d).$op(&Exact::parse(&x.$field)),
                |x| (
                    Decimal64::parse(&x.d).unwrap(),
                    <$right>::parse(&x.$field).unwrap()
                ),
                |x| (
                    Decimal::from_str(&x.d).unwrap(),
                    Decimal::from_str(&x.$field).unwrap()
                ),
                |&(a, b)| <$ty>::$op(a, b),
                |&(a, b)| a.$checked(b)
            );
        };
    }
    addsub!("add64", Decimal64, Decimal64, e, add, checked_add);
    addsub!("sub64", Decimal64, Decimal64, e, sub, checked_sub);
    addsub!("add_mixed", Decimal128, Decimal128, c, add, checked_add);
    addsub!("sub_mixed", Decimal128, Decimal128, c, sub, checked_sub);
    macro_rules! multiplication {
        ($name:expr, $a:ty, $b:ty, $field:ident) => {
            numeric!(
                $name,
                [a, $field],
                |x: &Input| Exact::parse(&x.a).mul(&Exact::parse(&x.$field)),
                |x| (<$a>::parse(&x.a).unwrap(), <$b>::parse(&x.$field).unwrap()),
                |x| (
                    Decimal::from_str(&x.a).unwrap(),
                    Decimal::from_str(&x.$field).unwrap()
                ),
                |&(a, b)| Decimal128::mul(a, b),
                |&(a, b)| a.checked_mul(b)
            );
        };
    }
    multiplication!("mul64x64", Decimal64, Decimal64, b);
    multiplication!("mul32x128", Decimal32, Decimal128, b_wide);
    for (suffix, dir, strategy) in [
        (
            "floor",
            Rounding::TowardNegative,
            RoundingStrategy::ToNegativeInfinity,
        ),
        (
            "ceil",
            Rounding::TowardPositive,
            RoundingStrategy::ToPositiveInfinity,
        ),
    ] {
        macro_rules! multiple {
            ($ty:ty, $name:expr) => {
                numeric!(
                    $name,
                    [a, step],
                    |x: &Input| Exact::parse(&x.a)
                        .rounded_multiple(&Exact::parse(&x.step), dir == Rounding::TowardPositive),
                    |x| (<$ty>::parse(&x.a).unwrap(), <$ty>::parse(&x.step).unwrap()),
                    |x| (
                        Decimal::from_str(&x.a).unwrap(),
                        Decimal::from_str(&x.step).unwrap()
                    ),
                    |&(p, t)| p.round_to_multiple(t, dir),
                    |&(p, t)| {
                        let q = p.checked_div(t)?;
                        let i = if dir == Rounding::TowardNegative {
                            q.floor()
                        } else {
                            q.ceil()
                        };
                        i.checked_mul(t)
                    }
                );
            };
        }
        multiple!(Decimal64, &format!("multiple64_{suffix}"));
        multiple!(Decimal32, &format!("multiple32_{suffix}"));
        numeric!(
            &format!("quantize_{suffix}"),
            [c],
            |x: &Input| Exact::parse(&x.c)
                .rounded_multiple(&Exact::parse("0.01"), dir == Rounding::TowardPositive),
            |x| Decimal128::parse(&x.c).unwrap(),
            |x| Decimal::from_str(&x.c).unwrap(),
            |&x| x.quantize(-2, dir),
            |&x| x.round_dp_with_strategy(2, strategy)
        );
    }
    macro_rules! comparison {
        ($suffix:expr, $ty:ty, $left:ident, $right:ident) => {
            for equality in [false, true] {
                paired(
                    c,
                    &format!(
                        "{}{suffix}",
                        if equality { "eq" } else { "cmp" },
                        suffix = $suffix
                    ),
                    &data,
                    |x| {
                        (
                            <$ty>::parse(&x.$left).unwrap(),
                            <$ty>::parse(&x.$right).unwrap(),
                        )
                    },
                    |x| {
                        (
                            Decimal::from_str(&x.$left).unwrap(),
                            Decimal::from_str(&x.$right).unwrap(),
                        )
                    },
                    |&(a, b)| {
                        if equality {
                            (a == b) as i8
                        } else {
                            a.partial_cmp(&b).map_or(2, |order| order as i8)
                        }
                    },
                    |&(a, b)| {
                        if equality {
                            (a == b) as i8
                        } else {
                            a.cmp(&b) as i8
                        }
                    },
                    |a, b| a == b,
                    |x| Exact::parse(&x.$left).inside() && Exact::parse(&x.$right).inside(),
                    |x, output| {
                        let order = Exact::parse(&x.$left).numeric_cmp(&Exact::parse(&x.$right));
                        *output
                            == if equality {
                                (order == std::cmp::Ordering::Equal) as i8
                            } else {
                                order as i8
                            }
                    },
                );
            }
        };
    }
    comparison!("64", Decimal64, compare_left, compare);
    comparison!("128", Decimal128, c, d);
}

criterion_group! { name = benches; config = Criterion::default()
.warm_up_time(Duration::from_secs(2)).measurement_time(Duration::from_secs(5)).sample_size(100);
targets = paths }
criterion_main!(benches);

#[cfg(test)]
pub fn check_workload_coverage() {
    let data = inputs();
    for field in [0, 1] {
        let digits: std::collections::BTreeSet<_> = data
            .iter()
            .map(|x| {
                (if field == 0 { &x.d } else { &x.e })
                    .replace('.', "")
                    .trim_start_matches('-')
                    .trim_start_matches('0')
                    .len()
            })
            .collect();
        assert_eq!(digits, (1..=16).collect());
    }
    let mut cohorts = 0;
    let mut unequal_exponents = 0;
    let mut a_scales = std::collections::BTreeSet::new();
    for x in &data {
        let a = Decimal64::parse(&x.compare_left).unwrap();
        let b = Decimal64::parse(&x.compare).unwrap();
        let left = support::decode_bid(a.to_bits() as u128, 64).unwrap();
        let right = support::decode_bid(b.to_bits() as u128, 64).unwrap();
        unequal_exponents += usize::from(left.exp != right.exp);
        cohorts += usize::from(a.to_bits() != b.to_bits() && left.exact() == right.exact());
        a_scales.insert(x.a.split_once('.').map_or(0, |(_, f)| f.len()));
        let a = Decimal64::parse(&x.d).unwrap();
        let b = Decimal64::parse(&x.e).unwrap();
        assert!(Decimal64::add(a, b).is_ok());
        assert!(Decimal64::sub(a, b).is_ok());
        assert!(Decimal32::parse(&x.a).is_ok());
    }
    assert!(cohorts >= COUNT / 2);
    assert!(unequal_exponents >= COUNT);
    assert_eq!(a_scales, (0..=4).collect());
}
