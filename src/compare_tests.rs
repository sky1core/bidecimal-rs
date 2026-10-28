use crate::refbid::{Finite, Format};
use crate::test_support::*;
use crate::{Decimal32, Decimal64, Decimal128, OrderedDecimal};
use core::cmp::Ordering;

#[derive(Clone, Copy)]
enum Value {
    D32(Decimal32),
    D64(Decimal64),
    D128(Decimal128),
}
impl Value {
    fn new(v: Finite, f: Format) -> Self {
        match f {
            Format::D32 => Self::D32(d32(v)),
            Format::D64 => Self::D64(d64(v)),
            Format::D128 => Self::D128(d128(v)),
        }
    }
    fn hash(self) -> u64 {
        match self {
            Self::D32(v) => hash(OrderedDecimal(v)),
            Self::D64(v) => hash(OrderedDecimal(v)),
            Self::D128(v) => hash(OrderedDecimal(v)),
        }
    }
    fn check(self, other: Self, expected: Ordering) {
        macro_rules! pair {
            ($a:expr, $b:expr) => {{
                assert_eq!($a == $b, expected == Ordering::Equal);
                assert_eq!($a.partial_cmp(&$b), Some(expected));
                assert_eq!($a < $b, expected == Ordering::Less);
                assert_eq!($a > $b, expected == Ordering::Greater);
            }};
        }
        match (self, other) {
            (Self::D32(a), Self::D32(b)) => {
                pair!(a, b);
                assert_eq!(OrderedDecimal(a).cmp(&OrderedDecimal(b)), expected);
            }
            (Self::D32(a), Self::D64(b)) => pair!(a, b),
            (Self::D32(a), Self::D128(b)) => pair!(a, b),
            (Self::D64(a), Self::D32(b)) => pair!(a, b),
            (Self::D64(a), Self::D64(b)) => {
                pair!(a, b);
                assert_eq!(OrderedDecimal(a).cmp(&OrderedDecimal(b)), expected);
            }
            (Self::D64(a), Self::D128(b)) => pair!(a, b),
            (Self::D128(a), Self::D32(b)) => pair!(a, b),
            (Self::D128(a), Self::D64(b)) => pair!(a, b),
            (Self::D128(a), Self::D128(b)) => {
                pair!(a, b);
                assert_eq!(OrderedDecimal(a).cmp(&OrderedDecimal(b)), expected);
            }
        }
        if expected == Ordering::Equal {
            assert_eq!(self.hash(), other.hash());
        }
    }
}

#[test]
fn decimal32_all_class_pairs_bigint_comparison() {
    let powers = Powers::new(250);
    let values: std::vec::Vec<_> = classes(Format::D32, Format::D32)
        .into_iter()
        .map(|v| {
            (
                d32(v),
                integer_at(v, -101, &powers),
                hash(OrderedDecimal(d32(v))),
            )
        })
        .collect();
    std::eprintln!(
        "Decimal32 comparison: {} values, {} ordered pairs",
        values.len(),
        values.len() * values.len()
    );
    for (a, av, ah) in &values {
        for (b, bv, bh) in &values {
            let expected = av.cmp(bv);
            assert_eq!(OrderedDecimal(*a).cmp(&OrderedDecimal(*b)), expected);
            assert_eq!(a.partial_cmp(b), Some(expected));
            assert_eq!(a == b, expected == Ordering::Equal);
            if expected == Ordering::Equal {
                assert_eq!(ah, bh);
            }
        }
    }
}

fn random_comparisons(count: u64, powers: &Powers) {
    for (i, a_format) in FORMATS.into_iter().enumerate() {
        for (j, b_format) in FORMATS.into_iter().enumerate() {
            let mut rng = Rng(0x082e_fa98_ec4e_6c89 ^ (i * 3 + j) as u64);
            for _ in 0..count {
                let a = rng.value(a_format);
                let b = rng.value(b_format);
                let expected = compare(a, b, powers);
                Value::new(a, a_format).check(Value::new(b, b_format), expected);
            }
        }
    }
}

#[test]
fn all_nine_width_pairs_bigint_random() {
    random_comparisons(2000, &Powers::new(12287));
}

#[test]
#[ignore = "one million comparisons for each of nine width pairs"]
fn all_nine_width_pairs_bigint_million() {
    let powers = Powers::new(12287);
    random_comparisons(1_000_000, &powers);
}

#[test]
fn cohort_hashes_cross_width_and_zero_signs() {
    for format in FORMATS {
        let (p, lo, _) = bounds(format);
        for v in classes(format, format) {
            let a = Value::new(v, format);
            for other_format in FORMATS {
                let (op, olo, ohi) = bounds(other_format);
                if v.coeff < 10_u128.pow(op) && (olo..=ohi).contains(&v.exp) {
                    a.check(Value::new(v, other_format), Ordering::Equal);
                }
            }
            if v.coeff != 0 {
                let n = v.coeff.ilog10() + 1;
                for shift in 0..=p - n {
                    if v.exp - shift as i32 >= lo {
                        let cohort = Finite {
                            coeff: v.coeff * 10_u128.pow(shift),
                            exp: v.exp - shift as i32,
                            ..v
                        };
                        a.check(Value::new(cohort, format), Ordering::Equal);
                    }
                }
            }
            macro_rules! signs {
                ($value:expr) => {{
                    let value = $value;
                    assert_eq!(value.is_zero(), v.coeff == 0);
                    assert_eq!(value.is_positive(), !v.neg && v.coeff != 0);
                    assert_eq!(value.is_negative(), v.neg && v.coeff != 0);
                }};
            }
            match format {
                Format::D32 => signs!(d32(v)),
                Format::D64 => signs!(d64(v)),
                Format::D128 => signs!(d128(v)),
            }
            if v.coeff == 0 {
                for other_format in FORMATS {
                    let (_, olo, ohi) = bounds(other_format);
                    for exp in [olo, 0, ohi] {
                        a.check(
                            Value::new(
                                Finite {
                                    neg: !v.neg,
                                    coeff: 0,
                                    exp,
                                },
                                other_format,
                            ),
                            Ordering::Equal,
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn comparison_exponent_gap_digit_guard_and_sign_classes() {
    let powers = Powers::new(12287);
    for fa in FORMATS {
        for fb in FORMATS {
            let (pa, la, ha) = bounds(fa);
            let (pb, lb, hb) = bounds(fb);
            for ca in coefficients(pa) {
                for cb in coefficients(pb) {
                    for gap in [0, 1, 33, 34, 35, 36, 38, 39, 70, 200, 12000] {
                        for base in [la.max(lb), 0, ha.min(hb) - gap] {
                            if !(la..=ha).contains(&(base + gap)) || !(lb..=hb).contains(&base) {
                                continue;
                            }
                            for na in [false, true] {
                                for nb in [false, true] {
                                    let a = Finite {
                                        neg: na,
                                        coeff: ca,
                                        exp: base + gap,
                                    };
                                    let b = Finite {
                                        neg: nb,
                                        coeff: cb,
                                        exp: base,
                                    };
                                    let expected = compare(a, b, &powers);
                                    Value::new(a, fa).check(Value::new(b, fb), expected);
                                    Value::new(b, fb).check(Value::new(a, fa), expected.reverse());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn comparison_alignment_boundaries_all_width_pairs() {
    let powers = Powers::new(12287);
    for fa in FORMATS {
        for fb in FORMATS {
            let (pa, la, ha) = bounds(fa);
            let (pb, lb, hb) = bounds(fb);
            let ma = 10_u128.pow(pa) - 1;
            let mb = 10_u128.pow(pb) - 1;
            for gap in (0..=39).chain([70, 200, 12287]) {
                let scale = 10_u128.checked_pow(gap as u32);
                let overflow_boundary = scale.map(|s| u128::MAX / s);
                let mut ac = std::vec![0, 1, 9, 10, ma - 1, ma];
                if let Some(boundary) = overflow_boundary {
                    for c in [boundary, boundary.saturating_add(1)] {
                        if c <= ma {
                            ac.push(c);
                        }
                    }
                }
                for ca in ac {
                    let mut bc = std::vec![0, 1, 9, 10, mb - 1, mb];
                    if let Some(aligned) = scale.and_then(|s| ca.checked_mul(s)) {
                        for c in [
                            aligned.saturating_sub(1),
                            aligned,
                            aligned.saturating_add(1),
                        ] {
                            if c <= mb {
                                bc.push(c);
                            }
                        }
                    }
                    for cb in bc {
                        for base in [la.max(lb), 0, ha.min(hb) - gap] {
                            if !(la..=ha).contains(&(base + gap)) || !(lb..=hb).contains(&base) {
                                continue;
                            }
                            for na in [false, true] {
                                for nb in [false, true] {
                                    let a = Finite {
                                        neg: na,
                                        coeff: ca,
                                        exp: base + gap,
                                    };
                                    let b = Finite {
                                        neg: nb,
                                        coeff: cb,
                                        exp: base,
                                    };
                                    let expected = compare(a, b, &powers);
                                    let av = Value::new(a, fa);
                                    let bv = Value::new(b, fb);
                                    av.check(bv, expected);
                                    bv.check(av, expected.reverse());
                                    av.check(av, Ordering::Equal);
                                    bv.check(bv, Ordering::Equal);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn comparison_extreme_exponents_all_width_pairs() {
    let powers = Powers::new(12287);
    for fa in FORMATS {
        for fb in FORMATS {
            let (pa, la, ha) = bounds(fa);
            let (pb, lb, hb) = bounds(fb);
            for qa in [la, la + 1, ha - 1, ha] {
                for qb in [lb, lb + 1, hb - 1, hb] {
                    for ca in [0, 1, 10_u128.pow(pa) - 1] {
                        for cb in [0, 1, 10_u128.pow(pb) - 1] {
                            for na in [false, true] {
                                for nb in [false, true] {
                                    let a = Finite {
                                        neg: na,
                                        coeff: ca,
                                        exp: qa,
                                    };
                                    let b = Finite {
                                        neg: nb,
                                        coeff: cb,
                                        exp: qb,
                                    };
                                    let expected = compare(a, b, &powers);
                                    Value::new(a, fa).check(Value::new(b, fb), expected);
                                    Value::new(b, fb).check(Value::new(a, fa), expected.reverse());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn comparison_decimal128_canonical_storage_boundary() {
    use crate::Error;
    use crate::refbid::{Classified, classify};

    let powers = Powers::new(12287);
    for exp in [-6176, -1, 0, 6111] {
        for neg in [false, true] {
            let sign = u128::from(neg) << 127;
            let exponent = ((exp + 6176) as u128) << 113;
            let coefficients = [
                0,
                1,
                (1_u128 << 64) - 1,
                1_u128 << 64,
                (1_u128 << 112) - 1,
                1_u128 << 112,
                10_u128.pow(34) - 1,
                10_u128.pow(34),
                (1_u128 << 113) - 1,
            ];
            let bits = coefficients
                .into_iter()
                .map(|c| sign | exponent | c)
                .chain([24_u128, 29].into_iter().map(|g| sign | (g << 122)));
            for bits in bits {
                match classify(bits, Format::D128) {
                    Classified::Finite(a) => {
                        let value = Value::D128(Decimal128::from_bits(bits).unwrap());
                        value.check(value, Ordering::Equal);
                        for fb in FORMATS {
                            let (p, lo, hi) = bounds(fb);
                            for coeff in [0, 1, 10_u128.pow(p) - 1] {
                                for exp in [lo, 0, hi] {
                                    for neg in [false, true] {
                                        let b = Finite { neg, coeff, exp };
                                        let expected = compare(a, b, &powers);
                                        value.check(Value::new(b, fb), expected);
                                        Value::new(b, fb).check(value, expected.reverse());
                                    }
                                }
                            }
                        }
                    }
                    Classified::NonCanonical => {
                        assert_eq!(Decimal128::from_bits(bits), Err(Error::InvalidInput));
                    }
                    other => panic!("unexpected boundary class: {other:?}"),
                }
            }
        }
    }
}
