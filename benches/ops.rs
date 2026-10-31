use bidecimal::{Decimal32, Decimal64, Decimal128, Error, Rounding};
use core::fmt::{self, Write};
use criterion::{Criterion, SamplingMode, Throughput, criterion_group, criterion_main};
use num_bigint::BigInt;
use std::{cmp::Ordering, hint::black_box, time::Duration};

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

#[expect(dead_code, reason = "shared with the instructions benchmark")]
mod reference;
use reference::{floor_div, floor_fields, pow10, signed};

trait Operate: Number + fmt::Display {
    fn sum(self, other: Self) -> Result<Self, Error>;
    fn difference(self, other: Self) -> Result<Self, Error>;
    fn sum_floor(self, other: Self) -> Result<Self, Error>;
    fn difference_floor(self, other: Self) -> Result<Self, Error>;
    fn product(self, other: Self) -> Result<Self, Error>;
    fn product_floor(self, other: Self) -> Result<Self, Error>;
    fn quantized_floor(self, exp: i32) -> Result<Self, Error>;
    fn multiple_floor(self, step: Self) -> Result<Self, Error>;
    fn total_order(&self, other: &Self) -> Ordering;
}
macro_rules! operate {
    ($($t:ident),*) => {$(
        impl Operate for $t {
            fn sum(self, other: Self) -> Result<Self, Error> { $t::add(self, other) }
            fn difference(self, other: Self) -> Result<Self, Error> { $t::sub(self, other) }
            fn sum_floor(self, other: Self) -> Result<Self, Error> {
                $t::add_rounded(self, other, Rounding::TowardNegative)
            }
            fn difference_floor(self, other: Self) -> Result<Self, Error> {
                $t::sub_rounded(self, other, Rounding::TowardNegative)
            }
            fn product(self, other: Self) -> Result<Self, Error> { $t::mul(self, other) }
            fn product_floor(self, other: Self) -> Result<Self, Error> {
                $t::mul_rounded(self, other, Rounding::TowardNegative)
            }
            fn quantized_floor(self, exp: i32) -> Result<Self, Error> {
                self.quantize(exp, Rounding::TowardNegative)
            }
            fn multiple_floor(self, step: Self) -> Result<Self, Error> {
                self.round_to_multiple(step, Rounding::TowardNegative)
            }
            fn total_order(&self, other: &Self) -> Ordering { self.total_cmp(other) }
        }
    )*};
}
operate!(Decimal32, Decimal64, Decimal128);

fn exact(spec: Spec) -> Exact {
    Exact::new(signed(spec), spec.exp)
}

fn plain_text(spec: Spec) -> String {
    let sign = if spec.neg { "-" } else { "" };
    let digits = spec.coeff.to_string();
    if spec.exp >= 0 {
        format!("{sign}{digits}{}", "0".repeat(spec.exp as usize))
    } else {
        let places = spec.exp.unsigned_abs() as usize;
        if places < digits.len() {
            let split = digits.len() - places;
            format!("{sign}{}.{}", &digits[..split], &digits[split..])
        } else {
            format!("{sign}0.{}{digits}", "0".repeat(places - digits.len()))
        }
    }
}

/// The fields of `plain_text(spec)` parsed with `digits` significant digits:
/// the text writes a positive exponent as trailing zeros, so its preferred
/// exponent is 0, and the result takes the representable exponent closest to
/// it.
fn parsed_fields(spec: Spec, digits: u32) -> Bid {
    if spec.exp <= 0 {
        return Bid {
            negative: spec.neg,
            coeff: spec.coeff,
            exp: spec.exp,
        };
    }
    let excess = (spec.coeff.ilog10() + 1 + spec.exp as u32).saturating_sub(digits);
    Bid {
        negative: spec.neg,
        coeff: spec.coeff * 10_u128.pow(spec.exp as u32 - excess),
        exp: excess as i32,
    }
}

fn parsing<T: Operate>(c: &mut Criterion, rng: &mut SplitMix64) {
    let specs: Vec<Spec> = (0..COUNT).map(|_| rng.spec(T::DIGITS)).collect();
    let inputs: Vec<String> = specs.iter().map(|&x| plain_text(x)).collect();
    for (&spec, text) in specs.iter().zip(&inputs) {
        assert_eq!(
            decoded(T::parse(text)),
            parsed_fields(spec, T::DIGITS),
            "parse_{}: {spec:?}, input={text:?}",
            T::NAME
        );
    }
    per_element(c, &format!("parse_{}", T::NAME), &inputs, |s| T::parse(s));
}

struct StackBuffer {
    bytes: [u8; 128],
    len: usize,
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
    let mut buffer = StackBuffer {
        bytes: [0; 128],
        len: 0,
    };
    let result = write!(&mut buffer, "{value}");
    (buffer, result)
}

fn formatting<T: Operate>(c: &mut Criterion, rng: &mut SplitMix64) {
    let specs: Vec<Spec> = (0..COUNT).map(|_| rng.spec(T::DIGITS)).collect();
    let inputs: Vec<T> = specs.iter().map(|&x| x.parse()).collect();
    for (&spec, &value) in specs.iter().zip(&inputs) {
        let (buffer, result) = format_stack(value);
        assert_eq!(result, Ok(()), "format_{}: {spec:?}", T::NAME);
        assert_eq!(
            &buffer.bytes[..buffer.len],
            plain_text(spec).as_bytes(),
            "format_{}: {spec:?}",
            T::NAME
        );
    }
    per_element(c, &format!("format_{}", T::NAME), &inputs, |&x| {
        format_stack(x)
    });
}

fn addition_specs(rng: &mut SplitMix64, left_digits: u32, right_digits: u32) -> (Spec, Spec) {
    let a = rng.spec(left_digits);
    let mut b = rng.spec(right_digits);
    b.exp = a.exp + rng.below(7) as i32 - 3;
    (a, b)
}

/// `x ± y` as an integer coefficient at the smaller exponent, and that exponent.
fn aligned_sum(x: Spec, y: Spec, subtract: bool) -> (BigInt, i32) {
    let exp = x.exp.min(y.exp);
    let scale = |s: Spec| signed(s) * pow10(s.exp - exp);
    let right = if subtract { -scale(y) } else { scale(y) };
    (scale(x) + right, exp)
}

fn check<T: Operate>(result: Result<T, Error>, expected: Bid, input: &str) {
    let value = result.unwrap_or_else(|error| panic!("{input}: {error:?}"));
    assert_eq!(decoded(value), expected, "{input}");
}

fn addition<T: Operate>(c: &mut Criterion, rng: &mut SplitMix64) {
    let specs: Vec<(Spec, Spec)> = (0..COUNT)
        .map(|_| addition_specs(rng, T::DIGITS - 4, T::DIGITS - 4))
        .collect();
    let inputs: Vec<(T, T)> = specs.iter().map(|&(a, b)| (a.parse(), b.parse())).collect();
    for (&(x, y), &(a, b)) in specs.iter().zip(&inputs) {
        for (subtract, result) in [(false, a.sum(b)), (true, a.difference(b))] {
            let (coeff, exp) = aligned_sum(x, y, subtract);
            assert!(coeff.magnitude().to_string().len() <= T::DIGITS as usize);
            let input = format!(
                "{}_{}: {x:?}, {y:?}",
                if subtract { "sub" } else { "add" },
                T::NAME
            );
            check(result, floor_fields(coeff, exp, T::DIGITS, false), &input);
        }
    }
    per_element(c, &format!("add_{}_exact", T::NAME), &inputs, |&(a, b)| {
        a.sum(b)
    });
    per_element(c, &format!("sub_{}_exact", T::NAME), &inputs, |&(a, b)| {
        a.difference(b)
    });

    let specs: Vec<(Spec, Spec)> = (0..COUNT)
        .map(|_| (rng.spec(T::DIGITS), rng.spec(T::DIGITS)))
        .collect();
    let inputs: Vec<(T, T)> = specs.iter().map(|&(a, b)| (a.parse(), b.parse())).collect();
    for (&(x, y), &(a, b)) in specs.iter().zip(&inputs) {
        for (subtract, result) in [(false, a.sum_floor(b)), (true, a.difference_floor(b))] {
            let (coeff, exp) = aligned_sum(x, y, subtract);
            let input = format!(
                "{}_{}_floor: {x:?}, {y:?}",
                if subtract { "sub" } else { "add" },
                T::NAME
            );
            check(result, floor_fields(coeff, exp, T::DIGITS, true), &input);
        }
    }
    per_element(c, &format!("add_{}_floor", T::NAME), &inputs, |&(a, b)| {
        a.sum_floor(b)
    });
    per_element(c, &format!("sub_{}_floor", T::NAME), &inputs, |&(a, b)| {
        a.difference_floor(b)
    });
}

fn product_spec(rng: &mut SplitMix64) -> Spec {
    let places = rng.below(13) as u32;
    let min_left_places = places.saturating_sub(8);
    let left_places =
        min_left_places + rng.below(u64::from(places.min(4) - min_left_places + 1)) as u32;
    let right_places = places - left_places;
    let mut left = rng.spec(7);
    left.exp = -(left_places as i32);
    let mut right = rng.spec(8 + right_places);
    right.exp = -(right_places as i32);
    Spec {
        neg: left.neg != right.neg,
        coeff: left.coeff * right.coeff,
        exp: left.exp + right.exp,
    }
}

fn addition_128(c: &mut Criterion, rng: &mut SplitMix64) {
    let same_exp: Vec<(Spec, Spec)> = (0..COUNT)
        .map(|_| {
            let a = rng.spec(30);
            let mut b = rng.spec(30);
            b.exp = a.exp;
            (a, b)
        })
        .collect();
    let gap_wide: Vec<(Spec, Spec)> = (0..COUNT)
        .map(|_| (product_spec(rng), product_spec(rng)))
        .collect();
    for (name, specs) in [
        ("add_d128_exact_same_exp", same_exp),
        ("add_d128_exact_gap_wide", gap_wide),
    ] {
        let inputs: Vec<(Decimal128, Decimal128)> =
            specs.iter().map(|&(a, b)| (a.parse(), b.parse())).collect();
        for (&(x, y), &(a, b)) in specs.iter().zip(&inputs) {
            let (coeff, exp) = aligned_sum(x, y, false);
            assert!(coeff.magnitude().to_string().len() <= 34);
            check(
                Decimal128::add(a, b),
                floor_fields(coeff, exp, 34, false),
                &format!("{name}: {x:?}, {y:?}"),
            );
        }
        per_element(c, name, &inputs, |&(a, b)| Decimal128::add(a, b));
    }

    let specs: Vec<Spec> = (0..COUNT).map(|_| product_spec(rng)).collect();
    let inputs: Vec<Decimal128> = specs.iter().map(|&x| x.parse()).collect();
    let mut accumulator = Decimal128::ZERO;
    let mut coeff = BigInt::ZERO;
    let mut exp = 0_i32;
    for (i, (&spec, &value)) in specs.iter().zip(&inputs).enumerate() {
        let next_exp = exp.min(spec.exp);
        coeff = coeff * pow10(exp - next_exp) + signed(spec) * pow10(spec.exp - next_exp);
        exp = next_exp;
        assert!(coeff.magnitude().to_string().len() <= 34);
        let result = Decimal128::add(accumulator, value);
        check(
            result,
            floor_fields(coeff.clone(), exp, 34, false),
            &format!("add_d128_accumulate: prefix={}, {spec:?}", i + 1),
        );
        accumulator = result.unwrap();
    }
    let mut group = c.benchmark_group("add_d128_accumulate");
    group.sampling_mode(SamplingMode::Flat);
    group.throughput(Throughput::Elements(COUNT as u64));
    group.bench_function("set", |bencher| {
        bencher.iter(|| {
            black_box(
                black_box(&inputs)
                    .iter()
                    .try_fold(black_box(Decimal128::ZERO), |acc, &value| {
                        Decimal128::add(acc, black_box(value))
                    }),
            )
        })
    });
    group.finish();
}

fn multiplication<T: Operate>(c: &mut Criterion, rng: &mut SplitMix64) {
    let half = T::DIGITS / 2;
    let specs: Vec<(Spec, Spec)> = (0..COUNT)
        .map(|_| (rng.spec(half), rng.spec(T::DIGITS - half)))
        .collect();
    let inputs: Vec<(T, T)> = specs.iter().map(|&(a, b)| (a.parse(), b.parse())).collect();
    for (&(x, y), &(a, b)) in specs.iter().zip(&inputs) {
        let coeff = signed(x) * signed(y);
        assert!(coeff.magnitude().to_string().len() <= T::DIGITS as usize);
        let expected = floor_fields(coeff, x.exp + y.exp, T::DIGITS, x.neg != y.neg);
        check(
            a.product(b),
            expected,
            &format!("mul_{}: {x:?}, {y:?}", T::NAME),
        );
    }
    per_element(c, &format!("mul_{}_exact", T::NAME), &inputs, |&(a, b)| {
        a.product(b)
    });

    let specs: Vec<(Spec, Spec)> = (0..COUNT)
        .map(|_| (rng.spec(T::DIGITS), rng.spec(T::DIGITS)))
        .collect();
    let inputs: Vec<(T, T)> = specs.iter().map(|&(a, b)| (a.parse(), b.parse())).collect();
    for (&(x, y), &(a, b)) in specs.iter().zip(&inputs) {
        let expected = floor_fields(
            signed(x) * signed(y),
            x.exp + y.exp,
            T::DIGITS,
            x.neg != y.neg,
        );
        check(
            a.product_floor(b),
            expected,
            &format!("mul_{}_floor: {x:?}, {y:?}", T::NAME),
        );
    }
    per_element(c, &format!("mul_{}_floor", T::NAME), &inputs, |&(a, b)| {
        a.product_floor(b)
    });
}

fn mixed(c: &mut Criterion, rng: &mut SplitMix64) {
    let specs: Vec<(Spec, Spec)> = (0..COUNT)
        .map(|_| {
            addition_specs(
                rng,
                <Decimal64 as Number>::DIGITS,
                <Decimal128 as Number>::DIGITS - 4,
            )
        })
        .collect();
    let inputs: Vec<(Decimal64, Decimal128)> =
        specs.iter().map(|&(a, b)| (a.parse(), b.parse())).collect();
    let digits = <Decimal128 as Number>::DIGITS;
    for (&(x, y), &(a, b)) in specs.iter().zip(&inputs) {
        let (coeff, exp) = aligned_sum(x, y, false);
        assert!(coeff.magnitude().to_string().len() <= digits as usize);
        let expected = floor_fields(coeff, exp, digits, false);
        check(
            Decimal128::add(a, b),
            expected,
            &format!("add_d64_d128_exact: {x:?}, {y:?}"),
        );
    }
    per_element(c, "add_d64_d128_exact", &inputs, |&(a, b)| {
        Decimal128::add(a, b)
    });

    let specs: Vec<(Spec, Spec)> = (0..COUNT)
        .map(|_| {
            (
                rng.spec(<Decimal32 as Number>::DIGITS),
                rng.spec(<Decimal128 as Number>::DIGITS - <Decimal32 as Number>::DIGITS),
            )
        })
        .collect();
    let inputs: Vec<(Decimal32, Decimal128)> =
        specs.iter().map(|&(a, b)| (a.parse(), b.parse())).collect();
    for (&(x, y), &(a, b)) in specs.iter().zip(&inputs) {
        let coeff = signed(x) * signed(y);
        assert!(coeff.magnitude().to_string().len() <= digits as usize);
        let expected = floor_fields(coeff, x.exp + y.exp, digits, x.neg != y.neg);
        check(
            Decimal128::mul(a, b),
            expected,
            &format!("mul_d32_d128_exact: {x:?}, {y:?}"),
        );
    }
    per_element(c, "mul_d32_d128_exact", &inputs, |&(a, b)| {
        Decimal128::mul(a, b)
    });
}

fn quantize<T: Operate>(c: &mut Criterion, rng: &mut SplitMix64) {
    let specs: Vec<(Spec, i32)> = (0..COUNT)
        .map(|_| {
            let x = rng.spec(T::DIGITS);
            (x, x.exp + 1 + rng.below(u64::from(T::DIGITS - 1)) as i32)
        })
        .collect();
    let inputs: Vec<(T, i32)> = specs.iter().map(|&(x, q)| (x.parse(), q)).collect();
    for (&(x, q), &(a, _)) in specs.iter().zip(&inputs) {
        let coeff = floor_div(&signed(x), &pow10(q - x.exp));
        let expected = Bid {
            negative: x.neg,
            coeff: u128::try_from(coeff.magnitude().clone()).unwrap(),
            exp: q,
        };
        check(
            a.quantized_floor(q),
            expected,
            &format!("quantize_{}: {x:?}, q={q}", T::NAME),
        );
    }
    per_element(
        c,
        &format!("quantize_{}_floor", T::NAME),
        &inputs,
        |&(x, q)| x.quantized_floor(q),
    );
}

fn multiple<T: Operate>(c: &mut Criterion, rng: &mut SplitMix64) {
    let specs: Vec<(Spec, Spec)> = (0..COUNT)
        .map(|_| {
            let x = rng.spec(T::DIGITS - 1);
            let mut step = rng.spec(3);
            step.neg = false;
            step.exp = x.exp + rng.below(3) as i32;
            (x, step)
        })
        .collect();
    let inputs: Vec<(T, T)> = specs
        .iter()
        .map(|&(x, step)| (x.parse(), step.parse()))
        .collect();
    for (&(x, step), &(a, b)) in specs.iter().zip(&inputs) {
        let units = floor_div(
            &signed(x),
            &(BigInt::from(step.coeff) * pow10(step.exp - x.exp)),
        );
        let coeff = units * BigInt::from(step.coeff);
        let expected = floor_fields(coeff, step.exp, T::DIGITS, x.neg);
        check(
            a.multiple_floor(b),
            expected,
            &format!("multiple_{}: {x:?}, step={step:?}", T::NAME),
        );
    }
    per_element(
        c,
        &format!("multiple_{}_floor", T::NAME),
        &inputs,
        |&(x, step)| x.multiple_floor(step),
    );
}

fn total_order(x: Spec, y: Spec) -> Ordering {
    exact(x).numeric_cmp(&exact(y)).then_with(|| {
        y.neg.cmp(&x.neg).then_with(|| {
            if x.neg {
                y.exp.cmp(&x.exp)
            } else {
                x.exp.cmp(&y.exp)
            }
        })
    })
}

fn comparison<T: Operate>(c: &mut Criterion, rng: &mut SplitMix64) {
    let specs: Vec<(Spec, Spec)> = (0..COUNT)
        .map(|i| {
            if i < COUNT / 2 {
                let x = if i % 32 == 0 {
                    rng.zero()
                } else {
                    rng.spec(T::DIGITS - 2)
                };
                let y = x.form(1 + rng.below(2) as u32);
                if rng.next() & 1 == 0 { (x, y) } else { (y, x) }
            } else {
                (rng.spec(T::DIGITS), rng.spec(T::DIGITS))
            }
        })
        .collect();
    let inputs: Vec<(T, T)> = specs.iter().map(|&(a, b)| (a.parse(), b.parse())).collect();
    for (&(x, y), &(a, b)) in specs.iter().zip(&inputs) {
        assert_eq!(
            a.total_order(&b),
            total_order(x, y),
            "total_cmp_{}: {x:?}, {y:?}",
            T::NAME
        );
    }
    per_element(c, &format!("total_cmp_{}", T::NAME), &inputs, |(a, b)| {
        a.total_order(b)
    });
}

fn width<T: Operate>(c: &mut Criterion, rng: &mut SplitMix64) {
    parsing::<T>(c, rng);
    formatting::<T>(c, rng);
    addition::<T>(c, rng);
    multiplication::<T>(c, rng);
    quantize::<T>(c, rng);
    multiple::<T>(c, rng);
    comparison::<T>(c, rng);
}

fn ops(c: &mut Criterion) {
    println!("WORKLOAD seed={SEED:#x} count={COUNT}");
    let mut rng = SplitMix64(SEED ^ 0x0a55_0e75);
    width::<Decimal32>(c, &mut rng);
    width::<Decimal64>(c, &mut rng);
    width::<Decimal128>(c, &mut rng);
    mixed(c, &mut rng);
    addition_128(c, &mut rng);
}

criterion_group! { name = benches; config = Criterion::default()
.warm_up_time(Duration::from_secs(1)).measurement_time(Duration::from_secs(2)).sample_size(100);
targets = ops }
criterion_main!(benches);
