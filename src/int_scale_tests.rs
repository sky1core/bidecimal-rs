use std::format;
use std::vec::Vec;

use crate::refbid::{self, Classified, Format};
use crate::test_support::*;
use crate::{Decimal32, Decimal64, Decimal128, Error, Rounding};

fn int_bounds(format: Format) -> (i128, i128) {
    match format {
        Format::D32 => (i128::from(i32::MIN), i128::from(i32::MAX)),
        Format::D64 => (i128::from(i64::MIN), i128::from(i64::MAX)),
        Format::D128 => (i128::MIN, i128::MAX),
    }
}

fn construct(int: i128, scale: i32, format: Format, mode: Option<Rounding>) -> Result<u128, Error> {
    macro_rules! at_width {
        ($decimal:ty, $int:ty) => {{
            let int = <$int>::try_from(int).unwrap();
            match mode {
                None => <$decimal>::from_mantissa_and_scale(int, scale),
                Some(dir) => <$decimal>::from_mantissa_and_scale_rounded(int, scale, dir),
            }
            .map(|x| x.to_bits() as u128)
        }};
    }
    match format {
        Format::D32 => at_width!(Decimal32, i32),
        Format::D64 => at_width!(Decimal64, i64),
        Format::D128 => at_width!(Decimal128, i128),
    }
}

fn check_creation(int: i128, scale: i32, format: Format, powers: &Powers) {
    let text = format!("{int}E{}", -i64::from(scale));
    for mode in MODES {
        let actual = construct(int, scale, format, mode);
        let parsed = match format {
            Format::D32 => match mode {
                None => Decimal32::parse(&text),
                Some(dir) => Decimal32::parse_rounded(&text, dir),
            }
            .map(|x| x.to_bits() as u128),
            Format::D64 => match mode {
                None => Decimal64::parse(&text),
                Some(dir) => Decimal64::parse_rounded(&text, dir),
            }
            .map(|x| x.to_bits() as u128),
            Format::D128 => match mode {
                None => Decimal128::parse(&text),
                Some(dir) => Decimal128::parse_rounded(&text, dir),
            }
            .map(Decimal128::to_bits),
        };
        assert_eq!(actual, parsed, "P={}, {text}, {mode:?}", bounds(format).0);
        let expected = directed(
            crate::parse_tests::reference(&text, format, mode, powers),
            int < 0,
            mode,
            format,
        );
        assert_eq!(
            actual.map(|bits| classified(bits, format)),
            expected,
            "P={}, {text}, {mode:?}",
            bounds(format).0
        );
    }
}

#[test]
fn int_scale_construction_boundary_classes() {
    let powers = Powers::new(12327);
    for format in FORMATS {
        let (p, lo, hi) = bounds(format);
        let (min, max) = int_bounds(format);
        let mut integers = std::vec![min, min + 1, max - 1, max, 0, 1, -1];
        for n in 1..=max.ilog10() + 1 {
            let power = 10_i128.pow(n - 1);
            for coeff in [
                Some(power - 1),
                Some(power),
                Some(power + 1),
                power.checked_mul(5),
                power.checked_mul(10).map(|n| n - 1),
            ]
            .into_iter()
            .flatten()
            {
                if coeff <= max {
                    integers.extend([coeff, -coeff]);
                }
            }
        }
        for zeros in 1..=max.ilog10() + 1 - p {
            if let Some(boundary) = (10_i128.pow(p) - 1).checked_mul(10_i128.pow(zeros)) {
                for coeff in [boundary - 1, boundary, boundary + 1] {
                    if coeff <= max {
                        integers.extend([coeff, -coeff]);
                    }
                }
            }
        }
        integers.sort_unstable();
        integers.dedup();
        for int in integers {
            let digits = if int == 0 {
                0
            } else {
                int.unsigned_abs().ilog10() + 1
            };
            let mut exps = std::vec![lo - 40, lo - 39, lo - 38, -1, 0, 1];
            for delta in -2..=2 {
                exps.extend([
                    lo + delta,
                    hi + delta,
                    lo - digits as i32 + delta,
                    lo - digits.saturating_sub(p) as i32 + delta,
                    hi + p as i32 - digits as i32 + delta,
                ]);
            }
            exps.sort_unstable();
            exps.dedup();
            let mut scales: Vec<i32> = exps.into_iter().map(|exp| -exp).collect();
            scales.extend([i32::MIN, i32::MAX]);
            for scale in scales {
                check_creation(int, scale, format, &powers);
            }
        }
    }
}

#[test]
fn int_scale_construction_fixed_seed_differential() {
    let powers = Powers::new(12327);
    let mut rng = Rng(0x78ca_1e5c_a1e0_0011);
    for format in FORMATS {
        let (_, lo, hi) = bounds(format);
        for index in 0..2000 {
            let int = match format {
                Format::D32 => i128::from(rng.next() as i32),
                Format::D64 => i128::from(rng.next() as i64),
                Format::D128 => rng.next_u128() as i128,
            };
            let scale = if index % 4 == 0 {
                rng.next() as i32
            } else {
                -(lo - 40 + (rng.next() % (hi - lo + 81) as u64) as i32)
            };
            check_creation(int, scale, format, &powers);
        }
    }
}

fn check_bits(bits: u128, format: Format) {
    let reference = refbid::classify(bits, format);
    macro_rules! at_width {
        ($decimal:ty, $bits:ty) => {
            match <$decimal>::from_bits(bits as $bits) {
                Ok(x) => match reference {
                    Classified::Finite(v) => {
                        let magnitude = i128::try_from(v.coeff).unwrap();
                        let int = if v.neg { -magnitude } else { magnitude };
                        let parts = x.to_mantissa_and_scale().unwrap();
                        assert_eq!(
                            x.to_coefficient_and_exponent()
                                .map(|(c, e)| (i128::from(c), e)),
                            Ok((int, v.exp))
                        );
                        let from_exponent =
                            <$decimal>::from_coefficient_and_exponent(parts.0, v.exp).unwrap();
                        assert_eq!(
                            from_exponent.to_bits(),
                            <$decimal>::from_mantissa_and_scale(parts.0, parts.1)
                                .unwrap()
                                .to_bits()
                        );
                        assert_eq!(x.coefficient().map(i128::from), Ok(int));
                        assert_eq!(x.mantissa(), x.coefficient());
                        assert_eq!(x.exponent(), Ok(v.exp));
                        assert_eq!(x.scale(), Ok(-v.exp));
                        assert_eq!(
                            (i128::from(parts.0), parts.1),
                            (int, -v.exp),
                            "bits={bits:x}"
                        );
                        let restored =
                            <$decimal>::from_mantissa_and_scale(parts.0, parts.1).unwrap();
                        let expected = refbid::encode(
                            crate::refbid::Finite {
                                neg: v.neg && v.coeff != 0,
                                ..v
                            },
                            format,
                        );
                        assert_eq!(restored.to_bits() as u128, expected, "bits={bits:x}");
                        if v.coeff != 0 || !v.neg {
                            assert_eq!(restored.to_bits(), x.to_bits(), "bits={bits:x}");
                        }
                    }
                    Classified::Infinity | Classified::Nan => {
                        assert!(canonical_special(bits, format));
                        assert_eq!(
                            x.to_mantissa_and_scale(),
                            Err(Error::InvalidInput),
                            "bits={bits:x}"
                        );
                        assert_eq!(x.to_coefficient_and_exponent(), Err(Error::InvalidInput));
                        assert_eq!(x.coefficient(), Err(Error::InvalidInput));
                        assert_eq!(x.mantissa(), Err(Error::InvalidInput));
                        assert_eq!(x.exponent(), Err(Error::InvalidInput));
                        assert_eq!(x.scale(), Err(Error::InvalidInput));
                    }
                    Classified::NonCanonical => panic!("accepted noncanonical {bits:x}"),
                },
                Err(e) => {
                    assert_eq!(e, Error::InvalidInput);
                    assert!(
                        matches!(reference, Classified::NonCanonical)
                            || (matches!(reference, Classified::Infinity | Classified::Nan)
                                && !canonical_special(bits, format)),
                        "rejected canonical {bits:x}"
                    );
                }
            }
        };
    }
    match format {
        Format::D32 => at_width!(Decimal32, u32),
        Format::D64 => at_width!(Decimal64, u64),
        Format::D128 => at_width!(Decimal128, u128),
    }
}

#[test]
fn int_scale_decomposition_classes_and_roundtrip() {
    for format in FORMATS {
        let (p, lo, hi) = bounds(format);
        for v in classes(format, format) {
            check_bits(refbid::encode(v, format), format);
        }
        let (k, _, t, _, max) = format.parameters();
        let steering_boundary = 1_u128 << (t + 3);
        for exp in lo..=hi {
            for coeff in [
                0,
                1,
                max,
                steering_boundary - 1,
                steering_boundary,
                steering_boundary + 1,
            ] {
                if coeff > max {
                    continue;
                }
                for neg in [false, true] {
                    check_bits(
                        refbid::encode(crate::refbid::Finite { neg, coeff, exp }, format),
                        format,
                    );
                }
            }
        }
        for neg in [false, true] {
            check_bits(special_bits(format, false, neg, 0), format);
            for payload in [0, 1, 10_u128.pow(p - 1) - 1] {
                let nan = special_bits(format, true, neg, payload);
                check_bits(nan, format);
                check_bits(nan | (1 << (k - 7)), format);
            }
        }
    }
}

#[test]
fn int_scale_decomposition_fixed_seed_bits() {
    let mut rng = Rng(0xb175_0078_5ca1_e001);
    for format in FORMATS {
        for _ in 0..10000 {
            let bits = match format {
                Format::D32 => u128::from(rng.next() as u32),
                Format::D64 => u128::from(rng.next()),
                Format::D128 => rng.next_u128(),
            };
            check_bits(bits, format);
        }
    }
}

#[test]
fn int_scale_documented_contract() {
    macro_rules! at_width {
        ($decimal:ty) => {
            for (text, parts) in [("1.00", (100, 2)), ("1E+5", (1, -5)), ("-1.00", (-100, 2))] {
                let x = <$decimal>::parse(text).unwrap();
                assert_eq!(x.to_mantissa_and_scale(), Ok(parts));
                assert_eq!(
                    <$decimal>::from_mantissa_and_scale(parts.0, parts.1)
                        .unwrap()
                        .to_bits(),
                    x.to_bits()
                );
            }
            let x = <$decimal>::parse("-0.00").unwrap();
            assert!(x.is_sign_negative());
            assert_eq!(x.to_mantissa_and_scale(), Ok((0, 2)));
            assert!(
                !<$decimal>::from_mantissa_and_scale(0, 2)
                    .unwrap()
                    .is_sign_negative()
            );
        };
    }
    at_width!(Decimal32);
    at_width!(Decimal64);
    at_width!(Decimal128);
}

#[test]
#[ignore = "all 2^32 Decimal32 bit patterns for integer/scale decomposition and round trip"]
fn int_scale_decimal32_exhaustive_reference() {
    for bits in 0..1_u64 << 32 {
        check_bits(u128::from(bits), Format::D32);
    }
}

#[test]
fn coefficient_exponent_construction_matches_parser_and_scale() {
    macro_rules! at_width {
        ($decimal:ty, $int:ty) => {
            for coefficient in [
                <$int>::MIN,
                <$int>::MIN + 1,
                -12345,
                -1,
                0,
                1,
                12345,
                <$int>::MAX,
            ] {
                for exponent in [
                    i32::MIN,
                    i32::MIN + 1,
                    i32::MAX,
                    <$decimal>::MIN_EXP - 1,
                    <$decimal>::MIN_EXP,
                    -2,
                    0,
                    2,
                    <$decimal>::MAX_EXP,
                    <$decimal>::MAX_EXP + 1,
                ] {
                    let text = format!("{coefficient}E{exponent}");
                    let exact = <$decimal>::from_coefficient_and_exponent(coefficient, exponent);
                    assert_eq!(
                        exact.map(|x| x.to_bits()),
                        <$decimal>::parse(&text).map(|x| x.to_bits())
                    );
                    if let Some(scale) = exponent.checked_neg() {
                        assert_eq!(
                            exact.map(|x| x.to_bits()),
                            <$decimal>::from_mantissa_and_scale(coefficient, scale)
                                .map(|x| x.to_bits())
                        );
                    }
                    for dir in [Rounding::TowardNegative, Rounding::TowardPositive] {
                        let rounded = <$decimal>::from_coefficient_and_exponent_rounded(
                            coefficient,
                            exponent,
                            dir,
                        );
                        assert_eq!(
                            rounded.map(|x| x.to_bits()),
                            <$decimal>::parse_rounded(&text, dir).map(|x| x.to_bits())
                        );
                        if let Some(scale) = exponent.checked_neg() {
                            assert_eq!(
                                rounded.map(|x| x.to_bits()),
                                <$decimal>::from_mantissa_and_scale_rounded(
                                    coefficient,
                                    scale,
                                    dir
                                )
                                .map(|x| x.to_bits())
                            );
                        }
                    }
                }
            }
        };
    }
    at_width!(Decimal32, i32);
    at_width!(Decimal64, i64);
    at_width!(Decimal128, i128);
}
