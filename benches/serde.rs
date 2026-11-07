//! Serialization through `serde_json` and `Display` with flags, from short
//! text through the limit of the stack text that serialization writes in one
//! piece to the longest finite text.

use bidecimal::{Decimal64, Decimal128};
use core::fmt::{self, Write};
use criterion::{Criterion, criterion_group, criterion_main};
use serde::{Serialize, de::DeserializeOwned};
use std::{hint::black_box, time::Duration};

#[expect(
    dead_code,
    reason = "shared by the benchmarks, each of which uses part of it"
)]
mod support;
use support::{Bid, decode_bid};

/// 34 distinct-looking digits for the boundary cases.
const DIGITS: &str = "1234567890123456789012345678901234";
const NINES: &str = "9999999999999999999999999999999999";

/// Longest text, sign included, that serialization writes on the stack:
/// `TEXT_LEN` in `src/display.rs`, which `scripts/test_bench_ab.py` keeps
/// equal to this.
const STACK_TEXT: usize = 112;

trait Value: Copy + fmt::Display + fmt::Debug + PartialEq + Serialize + DeserializeOwned {
    fn parse(text: &str) -> Self;
    fn fields(self) -> Bid;
}
macro_rules! value {
    ($($t:ident $width:literal),*) => {$(
        impl Value for $t {
            fn parse(text: &str) -> Self {
                $t::parse(text).unwrap_or_else(|e| panic!("{text}: {e:?}"))
            }
            fn fields(self) -> Bid {
                decode_bid(self.to_bits() as u128, $width).expect("finite")
            }
        }
    )*};
}
value!(Decimal64 64, Decimal128 128);

/// The text of `fields`, written from the sign, coefficient, and exponent:
/// the digits followed by `exp` zeros, or with a point `-exp` places from the
/// right; a zero with a positive exponent is `0`.
fn reference(fields: &Bid) -> String {
    let digits = fields.coeff.to_string();
    let magnitude = if fields.exp >= 0 {
        if fields.coeff == 0 {
            digits
        } else {
            digits + &"0".repeat(fields.exp as usize)
        }
    } else {
        let fraction = fields.exp.unsigned_abs() as usize;
        if digits.len() > fraction {
            let (integer, fraction) = digits.split_at(digits.len() - fraction);
            format!("{integer}.{fraction}")
        } else {
            format!("0.{}{digits}", "0".repeat(fraction - digits.len()))
        }
    };
    if fields.negative {
        format!("-{magnitude}")
    } else {
        magnitude
    }
}

/// Unsigned text of `len` bytes with the 34 boundary digits after `0.`.
fn unsigned(len: usize) -> String {
    format!("0.{}{DIGITS}", "0".repeat(len - 2 - DIGITS.len()))
}

/// Checks the text of `value` against the reference, as `Display`, padded,
/// and as a JSON string that reads back as the same number, then measures
/// serialization into a reused buffer and `Display` with and without padding.
fn case<T: Value>(c: &mut Criterion, name: &str, text: &str, len: usize) {
    let value = T::parse(text);
    case_value(c, name, value, len);
}

fn case_value<T: Value>(c: &mut Criterion, name: &str, value: T, len: usize) {
    let expected = reference(&value.fields());
    assert_eq!(expected.len(), len, "{name}");
    assert_eq!(value.to_string(), expected, "{name}");
    let width = len + 2;
    let padded = format!("{expected:>width$}");
    assert_eq!(format!("{value:>width$}"), padded, "{name}");
    let json = serde_json::to_string(&value).unwrap();
    assert_eq!(json, format!("\"{expected}\""), "{name}");
    let back: T = serde_json::from_str(&json).unwrap();
    assert_eq!(back, value, "{name}");

    let mut bytes = Vec::with_capacity(2 * len + 16);
    let mut text = String::with_capacity(2 * len + 16);
    let mut group = c.benchmark_group(name);
    group.bench_function("json", |b| {
        b.iter(|| {
            bytes.clear();
            serde_json::to_writer(&mut bytes, black_box(&value)).unwrap();
            black_box(bytes.len())
        })
    });
    group.bench_function("display_padded", |b| {
        b.iter(|| {
            text.clear();
            write!(text, "{:>width$}", black_box(&value)).unwrap();
            black_box(text.len())
        })
    });
    group.bench_function("display", |b| {
        b.iter(|| {
            text.clear();
            write!(text, "{}", black_box(&value)).unwrap();
            black_box(text.len())
        })
    });
    group.finish();
}

fn bench(c: &mut Criterion) {
    // Short text: digits alone, with a point, with zeros after the point,
    // a negative zero, and a coefficient above `u64::MAX`.
    case::<Decimal64>(c, "d64_integer", "12345678", 8);
    case::<Decimal64>(c, "d64_point_neg", "-123.45", 7);
    case::<Decimal64>(c, "d64_small", "0.001", 5);
    case::<Decimal128>(c, "d128_point", "1234567.891", 11);
    case::<Decimal128>(c, "d128_zero_neg", "-0.00", 5);
    case::<Decimal128>(c, "d128_wide_neg", "-12345678901234567890123.4567", 29);
    // Large coefficients written at 64 and 65 bytes, with and without sign.
    for (exp, len) in [(30, 64), (-62, 64), (-63, 65)] {
        let tag = if exp < 0 {
            format!("m{}", -exp)
        } else {
            format!("p{exp}")
        };
        case_value(
            c,
            &format!("d128_nines_{tag}"),
            Decimal128::from_coefficient_and_exponent(NINES.parse().unwrap(), exp).unwrap(),
            len,
        );
        case_value(
            c,
            &format!("d128_nines_{tag}_neg"),
            -Decimal128::from_coefficient_and_exponent(NINES.parse().unwrap(), exp).unwrap(),
            len + 1,
        );
    }
    // Both sides of the stack text limit, with and without sign.
    for len in [STACK_TEXT - 1, STACK_TEXT, STACK_TEXT + 1] {
        let text = unsigned(len);
        case::<Decimal128>(c, &format!("d128_len{len}"), &text, len);
        case::<Decimal128>(
            c,
            &format!("d128_len{len}_neg"),
            &format!("-{}", unsigned(len - 1)),
            len,
        );
    }
    // Long text: many zeros after the point, the smallest exponent, and the
    // largest integer.
    case::<Decimal128>(
        c,
        "d128_nines_m200",
        &reference(&Bid {
            negative: false,
            coeff: NINES.parse().unwrap(),
            exp: -200,
        }),
        202,
    );
    case::<Decimal128>(c, "d128_min_exp", &format!("0.{}1", "0".repeat(6175)), 6178);
    case_value(c, "d128_max", Decimal128::MAX, 6145);
}

criterion_group! { name = benches; config = Criterion::default()
.warm_up_time(Duration::from_millis(500)).measurement_time(Duration::from_secs(1)).sample_size(100)
.nresamples(2000);
targets = bench }
criterion_main!(benches);
