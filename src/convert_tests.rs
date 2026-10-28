use crate::refbid::{self, Classified, Finite, Format};
use crate::test_support::*;
use crate::{Decimal32, Decimal64, Decimal128};

fn check_narrow(v: Finite, source: Format, target: Format, powers: &Powers) {
    // The oracle receives the independently decoded interchange value.
    let v = decoded(refbid::encode(v, source), source);
    for mode in MODES {
        let actual = match (source, target, mode) {
            (Format::D64, Format::D32, None) => {
                Decimal32::try_from(d64(v)).map(|x| u128::from(x.to_bits()))
            }
            (Format::D64, Format::D32, Some(dir)) => {
                Decimal32::from_d64_rounded(d64(v), dir).map(|x| u128::from(x.to_bits()))
            }
            (Format::D128, Format::D32, None) => {
                Decimal32::try_from(d128(v)).map(|x| u128::from(x.to_bits()))
            }
            (Format::D128, Format::D32, Some(dir)) => {
                Decimal32::from_d128_rounded(d128(v), dir).map(|x| u128::from(x.to_bits()))
            }
            (Format::D128, Format::D64, None) => {
                Decimal64::try_from(d128(v)).map(|x| u128::from(x.to_bits()))
            }
            (Format::D128, Format::D64, Some(dir)) => {
                Decimal64::from_d128_rounded(d128(v), dir).map(|x| u128::from(x.to_bits()))
            }
            _ => unreachable!(),
        }
        .map(|bits| classified(bits, target));
        let expected = directed(narrow(v, target, mode, powers), v.neg, mode, target);
        assert_eq!(
            actual,
            expected,
            "source={v:?}, mode={mode:?}, target digits={}",
            bounds(target).0
        );
    }
}

const PAIRS: [(Format, Format); 3] = [
    (Format::D64, Format::D32),
    (Format::D128, Format::D32),
    (Format::D128, Format::D64),
];

#[test]
fn narrowing_all_coefficient_exponent_classes_bigint() {
    let powers = Powers::new(12287);
    for (source, target) in PAIRS {
        let values = classes(source, target);
        std::eprintln!(
            "narrowing {} -> {}: {} signed class members, three modes",
            bounds(source).0,
            bounds(target).0,
            values.len()
        );
        for v in values {
            check_narrow(v, source, target, &powers);
        }
    }
}

fn random_narrowing(count: u64, powers: &Powers) {
    for (index, (source, target)) in PAIRS.into_iter().enumerate() {
        let mut rng = Rng(0xbe54_66cf_34e9_0c6c ^ index as u64);
        for _ in 0..count {
            check_narrow(rng.value(source), source, target, powers);
        }
    }
}

#[test]
fn narrowing_three_pairs_bigint_random() {
    random_narrowing(2000, &Powers::new(12287));
}

#[test]
#[ignore = "one million values per narrowing pair, exact and both rounding directions"]
fn narrowing_three_pairs_bigint_million() {
    let powers = Powers::new(12287);
    random_narrowing(1_000_000, &powers);
}

#[test]
fn narrowing_largest_finite_and_underflow_boundaries() {
    let powers = Powers::new(12287);
    for (source, target) in PAIRS {
        let (p, lo, hi) = bounds(target);
        for neg in [false, true] {
            for n in [1, 2, 3, 4] {
                let boundary = (10_u128.pow(p) - 1) * 10_u128.pow(n);
                for delta in -3_i128..=3 {
                    check_narrow(
                        Finite {
                            neg,
                            coeff: (boundary as i128 + delta) as u128,
                            exp: hi - n as i32,
                        },
                        source,
                        target,
                        &powers,
                    );
                }
            }
            for exp in lo - 40..=lo + 2 {
                for coeff in [
                    0,
                    1,
                    9,
                    10,
                    11,
                    99,
                    100,
                    101,
                    10_u128.pow(p) - 1,
                    10_u128.pow(p),
                    10_u128.pow(p) + 1,
                ] {
                    check_narrow(Finite { neg, coeff, exp }, source, target, &powers);
                }
            }
        }
    }
}

#[test]
fn widening_keeps_parts_for_all_classes() {
    for v in classes(Format::D32, Format::D32) {
        assert_eq!(
            decoded(Decimal64::from(d32(v)).to_bits() as u128, Format::D64),
            v
        );
        assert_eq!(decoded(Decimal128::from(d32(v)).to_bits(), Format::D128), v);
    }
    for v in classes(Format::D64, Format::D64) {
        assert_eq!(decoded(Decimal128::from(d64(v)).to_bits(), Format::D128), v);
    }
}

#[test]
fn decimal64_widening_ten_million_reference() {
    let mut rng = Rng(0xc0ac_29b7_c97c_50dd);
    for _ in 0..10_000_000 {
        let v = rng.value(Format::D64);
        assert_eq!(decoded(Decimal128::from(d64(v)).to_bits(), Format::D128), v);
    }
}

#[test]
#[ignore = "all 2^32 Decimal32 bit patterns widened to both larger widths"]
fn decimal32_exhaustive_widening_reference() {
    for bits in 0..1_u64 << 32 {
        let reference = refbid::classify(u128::from(bits), Format::D32);
        match (reference, Decimal32::from_bits(bits as u32)) {
            (Classified::Finite(v), Ok(value)) => {
                assert_eq!(
                    decoded(Decimal64::from(value).to_bits() as u128, Format::D64),
                    v
                );
                assert_eq!(decoded(Decimal128::from(value).to_bits(), Format::D128), v);
            }
            (Classified::Finite(_), Err(e)) => panic!("rejected finite {bits:x}: {e}"),
            (special @ (Classified::Infinity | Classified::Nan), Ok(value)) => {
                assert!(
                    canonical_special(u128::from(bits), Format::D32),
                    "accepted noncanonical {bits:x}"
                );
                let nan = matches!(special, Classified::Nan);
                let (neg, payload) = (bits >> 31 == 1, u128::from(bits) & ((1 << 20) - 1));
                assert_eq!(
                    u128::from(Decimal64::from(value).to_bits()),
                    special_bits(Format::D64, nan, neg, payload),
                    "bits={bits:x}"
                );
                assert_eq!(
                    Decimal128::from(value).to_bits(),
                    special_bits(Format::D128, nan, neg, payload),
                    "bits={bits:x}"
                );
            }
            (_, Err(e)) => {
                assert_eq!(e, crate::Error::InvalidInput);
                assert!(
                    !canonical_special(u128::from(bits), Format::D32),
                    "rejected {bits:x}"
                );
            }
            (_, Ok(_)) => panic!("accepted invalid {bits:x}"),
        }
    }
}

#[test]
fn operand_bound_converts_to_decimal128() {
    fn operand<T: crate::Operand>(value: T) -> Decimal128 {
        value.into()
    }
    let a = Decimal32::parse("-1.25").expect("valid");
    let b = Decimal64::parse("3.500").expect("valid");
    let c = Decimal128::parse("7e-3").expect("valid");
    assert_eq!(operand(a).to_bits(), Decimal128::from(a).to_bits());
    assert_eq!(operand(b).to_bits(), Decimal128::from(b).to_bits());
    assert_eq!(operand(c).to_bits(), c.to_bits());
}
