use crate::layout::{
    InRange, Unpacked, decode_32, decode_64, decode_128, encode_32, encode_64, encode_128,
};
use crate::refbid::{self, Classified, Finite, Format};
use crate::test_support::{Rng, canonical_special, finite};
use crate::{Decimal32, Decimal64, Decimal128, Error};

fn unpacked(value: Finite) -> Unpacked {
    Unpacked {
        neg: value.neg,
        coeff: value.coeff,
        exp: value.exp,
    }
}

macro_rules! check_width {
    ($check:ident, $roundtrip:ident, $bits:ty, $decimal:ident, $format:ident, $decode:ident, $encode:ident) => {
        fn $check(bits: $bits) -> Classified {
            let reference = refbid::classify(bits as u128, Format::$format);
            let decoded = $decode(bits);
            let public = $decimal::from_bits(bits);
            if let Classified::Finite(value) = reference {
                let actual = decoded.expect("reference accepted encoding");
                assert_eq!(finite(actual), value, "bits={bits:x}");
                assert_eq!(
                    $encode(
                        InRange::checked(actual.neg, actual.coeff, i128::from(actual.exp))
                            .expect("decoded fields are in range")
                    ),
                    bits,
                    "bits={bits:x}"
                );
                assert_eq!(public.map($decimal::to_bits), Ok(bits), "bits={bits:x}");
                let x = public.expect("reference accepted encoding");
                let positive = refbid::encode(
                    Finite {
                        neg: false,
                        ..value
                    },
                    Format::$format,
                );
                let negative = refbid::encode(Finite { neg: true, ..value }, Format::$format);
                let negated = if value.neg { positive } else { negative };
                assert_eq!((-x).to_bits() as u128, negated, "bits={bits:x}");
                assert_eq!(x.abs().to_bits() as u128, positive, "bits={bits:x}");
                assert_eq!(
                    x.copysign($decimal::ZERO).to_bits() as u128,
                    positive,
                    "bits={bits:x}"
                );
                assert_eq!(
                    x.copysign($decimal::MIN).to_bits() as u128,
                    negative,
                    "bits={bits:x}"
                );
                assert_eq!(x.is_sign_negative(), value.neg, "bits={bits:x}");
                assert_eq!(x.is_sign_positive(), !value.neg, "bits={bits:x}");
            } else {
                assert_eq!(
                    decoded.map(finite),
                    Err(Error::InvalidInput),
                    "bits={bits:x}"
                );
                let canonical = matches!(reference, Classified::Infinity | Classified::Nan)
                    && canonical_special(bits as u128, Format::$format);
                assert_eq!(
                    public.map($decimal::to_bits),
                    if canonical {
                        Ok(bits)
                    } else {
                        Err(Error::InvalidInput)
                    },
                    "bits={bits:x}"
                );
                if let Ok(x) = public {
                    assert_eq!(x.is_sign_negative(), bits >> (<$bits>::BITS - 1) == 1);
                    assert_eq!(
                        (-x).to_bits(),
                        bits ^ (1 << (<$bits>::BITS - 1)),
                        "bits={bits:x}"
                    );
                    assert_eq!(
                        x.abs().to_bits(),
                        bits & !(1 << (<$bits>::BITS - 1)),
                        "bits={bits:x}"
                    );
                    assert_eq!(x.is_nan(), matches!(reference, Classified::Nan));
                    assert_eq!(x.is_infinite(), matches!(reference, Classified::Infinity));
                    assert!(!x.is_finite());
                }
            }
            reference
        }

        fn $roundtrip(value: Finite) {
            let fields = unpacked(value);
            let encoded = $encode(
                InRange::checked(fields.neg, fields.coeff, i128::from(fields.exp))
                    .expect("generated fields are in range"),
            );
            assert_eq!(encoded as u128, refbid::encode(value, Format::$format));
            assert_eq!($decode(encoded).map(finite), Ok(value));
            $check(encoded);
        }
    };
}

check_width!(
    check32,
    roundtrip32,
    u32,
    Decimal32,
    D32,
    decode_32,
    encode_32
);
check_width!(
    check64,
    roundtrip64,
    u64,
    Decimal64,
    D64,
    decode_64,
    encode_64
);
check_width!(
    check128,
    roundtrip128,
    u128,
    Decimal128,
    D128,
    decode_128,
    encode_128
);

fn biased_exp(index: u64, random: u64, w: u32, bias: i32) -> u128 {
    let max = (3 << w) - 1;
    match index % 8 {
        0 => 0,
        1 => 1,
        2 => max - 1,
        3 => max,
        4 => bias as u128,
        5 => (1 << w) - 1,
        6 => 1 << w,
        _ => u128::from(random) % (max + 1),
    }
}

fn structured_bits(index: u64, rng: &mut Rng, format: Format) -> u128 {
    let (k, w, t, bias, max_coeff) = format.parameters();
    let random = rng.next_u128();
    let sign = (random >> 127) << (k - 1);
    let e = biased_exp(index / 12, rng.next(), w, bias);
    let small_bits = t + 3;
    let small_mask = (1 << small_bits) - 1;
    let large_mask = (1 << (t + 1)) - 1;
    let steering = 3 << (k - 3);
    let delta = u128::from(rng.next() % 9);
    let near_binary = (1 << small_bits) - 4 + delta;
    let near_decimal = max_coeff - 3 + delta;
    let width_mask = u128::MAX >> (128 - k);
    match index % 12 {
        0 => random & width_mask,
        1 => sign | (e << small_bits) | (random & small_mask),
        2 => sign | steering | (e << (t + 1)) | (random & large_mask),
        3 => sign | (e << small_bits) | (near_binary & small_mask),
        4 => sign | steering | (e << (t + 1)) | (near_binary & large_mask),
        5 => {
            if near_decimal <= small_mask {
                sign | (e << small_bits) | near_decimal
            } else {
                sign | steering | (e << (t + 1)) | (near_decimal & large_mask)
            }
        }
        6 => sign | steering | (e << (t + 1)) | large_mask,
        7 | 8 => {
            let prefix = 30 + (index % 12 - 7);
            sign | (u128::from(prefix) << (k - 6)) | (random & ((1 << (k - 6)) - 1))
        }
        9 => sign | (e << small_bits),
        10 => sign | (e << small_bits) | small_mask,
        11 => refbid::encode(
            Finite {
                neg: sign != 0,
                coeff: random % (max_coeff + 1),
                exp: e as i32 - bias,
            },
            format,
        ),
        _ => unreachable!(),
    }
}

#[test]
fn decimal64_structured_splitmix64_reference() {
    let mut rng = Rng(0x243f_6a88_85a3_08d3);
    for i in 0..10_000_000 {
        check64(structured_bits(i, &mut rng, Format::D64) as u64);
    }
}

#[test]
fn decimal128_structured_splitmix64_reference() {
    let mut rng = Rng(0x1319_8a2e_0370_7344);
    for i in 0..10_000_000 {
        check128(structured_bits(i, &mut rng, Format::D128));
    }
}

#[test]
fn all_exponents_signs_and_coefficient_boundaries() {
    for format in [Format::D32, Format::D64, Format::D128] {
        let (_, w, t, bias, max_coeff) = format.parameters();
        let coefficients = coefficient_boundaries(t, max_coeff);
        for e in 0..3 << w {
            for neg in [false, true] {
                for &coeff in &coefficients {
                    if coeff <= max_coeff {
                        let value = Finite {
                            neg,
                            coeff,
                            exp: e - bias,
                        };
                        match format {
                            Format::D32 => roundtrip32(value),
                            Format::D64 => roundtrip64(value),
                            Format::D128 => roundtrip128(value),
                        }
                    }
                    let (k, _, _, _, _) = format.parameters();
                    for large in [false, true] {
                        let payload_bits = if large { t + 1 } else { t + 3 };
                        let steering = if large { 3 << (k - 3) } else { 0 };
                        let bits = ((neg as u128) << (k - 1))
                            | steering
                            | ((e as u128) << payload_bits)
                            | (coeff & ((1 << payload_bits) - 1));
                        match format {
                            Format::D32 => {
                                check32(bits as u32);
                            }
                            Format::D64 => {
                                check64(bits as u64);
                            }
                            Format::D128 => {
                                check128(bits);
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn structured_in_range_encode_decode() {
    let mut rng = Rng(0xa409_3822_299f_31d0);
    for format in [Format::D32, Format::D64, Format::D128] {
        let (_, w, _, bias, max_coeff) = format.parameters();
        for i in 0..100_000 {
            let value = Finite {
                neg: rng.next() & 1 != 0,
                coeff: rng.next_u128() % (max_coeff + 1),
                exp: biased_exp(i, rng.next(), w, bias) as i32 - bias,
            };
            match format {
                Format::D32 => roundtrip32(value),
                Format::D64 => roundtrip64(value),
                Format::D128 => roundtrip128(value),
            }
        }
    }
}

fn coefficient_boundaries(t: u32, max_coeff: u128) -> std::vec::Vec<u128> {
    let mut values = std::vec![0, 1, max_coeff - 1, max_coeff, max_coeff + 1];
    for boundary in [1 << t, 1 << (t + 1), 1 << (t + 3)] {
        values.extend([boundary - 1, boundary, boundary + 1]);
    }
    for j in 1..=max_coeff.ilog10() {
        let power = 10_u128.pow(j);
        values.extend([power - 1, power, power + 1, 5 * power]);
    }
    values
}

#[test]
fn constants_match_reference() {
    macro_rules! constants {
        ($decimal:ident, $format:ident, $digits:literal, $min_exp:literal, $max_exp:literal, $zero:expr, $max:expr, $min:expr) => {
            assert_eq!($decimal::DIGITS, $digits);
            assert_eq!($decimal::MIN_EXP, $min_exp);
            assert_eq!($decimal::MAX_EXP, $max_exp);
            assert_eq!($decimal::ZERO.to_bits(), $zero);
            assert_eq!($decimal::MAX.to_bits(), $max);
            assert_eq!($decimal::MIN.to_bits(), $min);
            for (constant, value) in [
                (
                    $decimal::ZERO,
                    Finite {
                        neg: false,
                        coeff: 0,
                        exp: 0,
                    },
                ),
                (
                    $decimal::MAX,
                    Finite {
                        neg: false,
                        coeff: 10_u128.pow($digits) - 1,
                        exp: $max_exp,
                    },
                ),
                (
                    $decimal::MIN,
                    Finite {
                        neg: true,
                        coeff: 10_u128.pow($digits) - 1,
                        exp: $max_exp,
                    },
                ),
            ] {
                assert_eq!(
                    refbid::classify(constant.to_bits() as u128, Format::$format),
                    Classified::Finite(value)
                );
                assert_eq!(
                    constant.to_bits() as u128,
                    refbid::encode(value, Format::$format)
                );
            }
        };
    }
    constants!(
        Decimal32,
        D32,
        7,
        -101,
        90,
        0x3280_0000,
        0x77f8_967f,
        0xf7f8_967f
    );
    constants!(
        Decimal64,
        D64,
        16,
        -398,
        369,
        0x31c0_0000_0000_0000,
        0x77fb_86f2_6fc0_ffff,
        0xf7fb_86f2_6fc0_ffff
    );
    constants!(
        Decimal128,
        D128,
        34,
        -6176,
        6111,
        0x3040_0000_0000_0000_0000_0000_0000_0000,
        0x5fff_ed09_bead_87c0_378d_8e63_ffff_ffff,
        0xdfff_ed09_bead_87c0_378d_8e63_ffff_ffff
    );
}

fn assert_small_form_128(value: Decimal128, path: &str) {
    let bits = value.to_bits();
    assert_ne!((bits >> 125) & 0b11, 0b11, "{path}: bits={bits:x}");
    assert!(
        bits & ((1_u128 << 113) - 1) < 10_u128.pow(34),
        "{path}: bits={bits:x}"
    );
    assert_eq!(
        Decimal128::from_bits(bits).map(Decimal128::to_bits),
        Ok(bits),
        "{path}: bits={bits:x}"
    );
}

fn small_form_fields_128(value: Decimal128) -> (bool, u128, i32) {
    let bits = value.to_bits();
    let exp = i32::try_from((bits >> 113) & 0x3fff).unwrap() - 6176;
    (bits >> 127 == 1, bits & ((1_u128 << 113) - 1), exp)
}

#[derive(Default)]
struct SmallFormCheck {
    ok: std::collections::BTreeMap<std::string::String, usize>,
}

impl SmallFormCheck {
    fn check(&mut self, result: Result<Decimal128, Error>, path: &str) -> Option<Decimal128> {
        let count = self.ok.entry(path.into()).or_insert(0);
        let value = result.ok().filter(|v| v.is_finite())?;
        assert_small_form_128(value, path);
        *count += 1;
        Some(value)
    }

    fn assert_every_path_ok(&self, expected_paths: usize) {
        assert_eq!(self.ok.len(), expected_paths, "{:?}", self.ok);
        for (path, count) in &self.ok {
            assert!(*count >= 1, "{path}: no Ok result checked");
        }
    }
}

fn check_add_sub_128<A: crate::Operand, B: crate::Operand>(
    check: &mut SmallFormCheck,
    lhs: &[A],
    rhs: &[B],
    widths: &str,
) {
    use crate::Rounding;

    for a in lhs {
        for b in rhs {
            check.check(Decimal128::add(*a, *b), &std::format!("add {widths}"));
            check.check(Decimal128::sub(*a, *b), &std::format!("sub {widths}"));
            for dir in [Rounding::TowardNegative, Rounding::TowardPositive] {
                check.check(
                    Decimal128::add_rounded(*a, *b, dir),
                    &std::format!("add_rounded {widths}"),
                );
                check.check(
                    Decimal128::sub_rounded(*a, *b, dir),
                    &std::format!("sub_rounded {widths}"),
                );
            }
        }
    }
}

fn check_mul_128<A: crate::Operand, B: crate::Operand>(
    check: &mut SmallFormCheck,
    lhs: &[A],
    rhs: &[B],
    widths: &str,
) {
    use crate::Rounding;

    for a in lhs {
        for b in rhs {
            check.check(Decimal128::mul(*a, *b), &std::format!("mul {widths}"));
            for dir in [Rounding::TowardNegative, Rounding::TowardPositive] {
                check.check(
                    Decimal128::mul_rounded(*a, *b, dir),
                    &std::format!("mul_rounded {widths}"),
                );
            }
        }
    }
}

#[test]
fn decimal128_construction_paths_store_small_form() {
    use crate::Rounding;

    const DIRS: [Rounding; 2] = [Rounding::TowardNegative, Rounding::TowardPositive];
    let mut check = SmallFormCheck::default();

    let carried = check.check(
        Decimal128::parse_rounded(
            "99999999999999999999999999999999995",
            Rounding::TowardPositive,
        ),
        "parse_rounded carry",
    );
    assert_eq!(
        carried.map(small_form_fields_128),
        Some((false, 10_u128.pow(33), 2))
    );
    let max_coeff: Decimal128 = "9999999999999999999999999999999999".parse().unwrap();
    let five_hundredths: Decimal128 = "0.05".parse().unwrap();
    let carried = check.check(
        Decimal128::add_rounded(max_coeff, five_hundredths, Rounding::TowardPositive),
        "add_rounded carry",
    );
    assert_eq!(
        carried.map(small_form_fields_128),
        Some((false, 10_u128.pow(33), 1))
    );

    let strings = [
        "0",
        "-0",
        "0E-6176",
        "-0E+6111",
        "1",
        "-1.5",
        "0.05",
        "18446744073709551615",
        "18446744073709551616",
        "5192296858534827628530496329220095",
        "5192296858534827628530496329220096",
        "9999999999999999999999999999999999",
        "-9999999999999999999999999999999999E+6111",
        "1E-6176",
        "1E-6177",
        "99999999999999999999999999999999995",
        "10384593717069655257060992658440191",
        "10384593717069655257060992658440192",
        "1E+6145",
    ];
    let mut values = std::vec![Decimal128::ZERO, Decimal128::MAX, Decimal128::MIN];
    for (value, path) in values.iter().zip(["ZERO", "MAX", "MIN"]) {
        assert_small_form_128(*value, path);
    }
    for s in strings {
        check.check(Decimal128::parse(s), "parse");
        check.check(s.parse::<Decimal128>(), "from_str");
        for dir in DIRS {
            values.extend(check.check(Decimal128::parse_rounded(s, dir), "parse_rounded"));
        }
    }

    let d32 = [
        Decimal32::ZERO,
        Decimal32::MAX,
        Decimal32::MIN,
        "8388607".parse().unwrap(),
        "8388608".parse().unwrap(),
        "-1.53".parse().unwrap(),
    ];
    let d64 = [
        Decimal64::ZERO,
        Decimal64::MAX,
        Decimal64::MIN,
        "9007199254740991".parse().unwrap(),
        "9007199254740992".parse().unwrap(),
        "-1.53".parse().unwrap(),
    ];
    let steps32: [Decimal32; 3] = [
        "0.05".parse().unwrap(),
        "1".parse().unwrap(),
        "0.03".parse().unwrap(),
    ];
    let steps64: [Decimal64; 3] = [
        "0.05".parse().unwrap(),
        "1".parse().unwrap(),
        "0.03".parse().unwrap(),
    ];
    for v in d32 {
        values.extend(check.check(Ok(Decimal128::from(v)), "widening from Decimal32"));
        for step in steps32 {
            for dir in DIRS {
                values.extend(check.check(
                    v.round_to_multiple(step, dir).map(Decimal128::from),
                    "widening multiple of a Decimal32 step",
                ));
            }
        }
    }
    for v in d64 {
        values.extend(check.check(Ok(Decimal128::from(v)), "widening from Decimal64"));
        for step in steps64 {
            for dir in DIRS {
                values.extend(check.check(
                    v.round_to_multiple(step, dir).map(Decimal128::from),
                    "widening multiple of a Decimal64 step",
                ));
            }
        }
    }

    for value in &values {
        check.check(Decimal128::from_bits(value.to_bits()), "from_bits");
        for exp in [Decimal128::MIN_EXP, -40, -1, 0, 1, 40, Decimal128::MAX_EXP] {
            for dir in DIRS {
                check.check(value.quantize(exp, dir), "quantize");
            }
        }
    }

    check_add_sub_128(&mut check, &d32, &d32, "32x32");
    check_add_sub_128(&mut check, &d32, &d64, "32x64");
    check_add_sub_128(&mut check, &d32, &values, "32x128");
    check_add_sub_128(&mut check, &d64, &d32, "64x32");
    check_add_sub_128(&mut check, &d64, &d64, "64x64");
    check_add_sub_128(&mut check, &d64, &values, "64x128");
    check_add_sub_128(&mut check, &values, &d32, "128x32");
    check_add_sub_128(&mut check, &values, &d64, "128x64");
    check_add_sub_128(&mut check, &values, &values, "128x128");

    check_mul_128(&mut check, &d32, &d32, "32x32");
    check_mul_128(&mut check, &d32, &d64, "32x64");
    check_mul_128(&mut check, &d32, &values, "32x128");
    check_mul_128(&mut check, &d64, &d32, "64x32");
    check_mul_128(&mut check, &d64, &d64, "64x64");
    check_mul_128(&mut check, &d64, &values, "64x128");
    check_mul_128(&mut check, &values, &d32, "128x32");
    check_mul_128(&mut check, &values, &d64, "128x64");
    check_mul_128(&mut check, &values, &values, "128x128");

    for value in values.clone() {
        for step in [
            Decimal128::parse("0.05").unwrap(),
            Decimal128::parse("0.03").unwrap(),
        ] {
            for dir in DIRS {
                check.check(value.round_to_multiple(step, dir), "round_to_multiple");
            }
        }
    }

    for value in values.clone() {
        check.check(Ok(-value), "neg");
        check.check(Ok(value.abs()), "abs");
        check.check(Ok(value.copysign(Decimal128::MIN)), "copysign");
    }

    for &a in &values {
        for &b in &values {
            check.check(Decimal128::div(a, b), "div");
            for dir in DIRS {
                check.check(Decimal128::div_rounded(a, b, dir), "div_rounded");
            }
        }
    }

    check.assert_every_path_ok(2 + 3 + 4 + 2 + 4 * 9 + 2 * 9 + 1 + 3 + 2);
}

#[test]
fn decimal128_from_bits_rejects_large_form() {
    let mut patterns = std::vec::Vec::new();
    for g in [24_u128, 25, 26, 27, 28, 29] {
        for neg in [false, true] {
            let sign = u128::from(neg) << 127;
            patterns.push(sign | (g << 122));
            patterns.push(sign | (g << 122) | ((1_u128 << 122) - 1));
        }
    }
    let mut rng = Rng(0x2020_0128);
    for _ in 0..100_000 {
        let random = rng.next_u128();
        let g = 24 + u128::from(rng.next() % 6);
        patterns.push((random & (1_u128 << 127)) | (g << 122) | (random & ((1_u128 << 122) - 1)));
    }
    for bits in patterns {
        assert!(
            (24..=29).contains(&((bits >> 122) & 0b11111)),
            "bits={bits:x}"
        );
        assert_eq!(
            Decimal128::from_bits(bits).map(Decimal128::to_bits),
            Err(Error::InvalidInput),
            "bits={bits:x}"
        );
    }
}

#[test]
#[ignore = "exhaustively checks all 2^32 Decimal32 encodings"]
fn decimal32_exhaustive_reference() {
    let mut counts = [0_u64; 4];
    for bits in 0..=u32::MAX {
        let class = match check32(bits) {
            Classified::Finite(_) => 0,
            Classified::NonCanonical => 1,
            Classified::Infinity => 2,
            Classified::Nan => 3,
        };
        counts[class] += 1;
    }
    assert_eq!(
        counts,
        [3_840_000_000, 186_531_840, 134_217_728, 134_217_728]
    );
}
