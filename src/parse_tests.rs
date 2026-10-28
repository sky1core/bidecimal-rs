use num_bigint::{BigInt, BigUint, Sign};
use std::format;
use std::string::{String, ToString};
use std::time::Instant;

use crate::layout::{InRange, Unpacked, W32, W64, W128, Width};
use crate::refbid::{Finite, Format};
use crate::test_support::*;
use crate::{Decimal32, Decimal64, Decimal128, Error, Rounding};

pub(crate) fn actual(s: &str, format: Format, mode: Option<Rounding>) -> Result<Value, Error> {
    macro_rules! parse {
        ($ty:ty) => {
            match mode {
                None => <$ty>::parse(s),
                Some(dir) => <$ty>::parse_rounded(s, dir),
            }
            .map(|v| v.to_bits() as u128)
        };
    }
    match format {
        Format::D32 => parse!(Decimal32),
        Format::D64 => parse!(Decimal64),
        Format::D128 => parse!(Decimal128),
    }
    .map(|bits| classified(bits, format))
}

pub(crate) fn reference(
    s: &str,
    format: Format,
    mode: Option<Rounding>,
    powers: &Powers,
) -> Result<Finite, Error> {
    let neg = s.starts_with('-');
    let unsigned = s.strip_prefix(['+', '-']).unwrap_or(s);
    let mut pieces = unsigned.split(['e', 'E']);
    let significand = pieces.next().unwrap();
    let exponent = pieces.next();
    if pieces.next().is_some() {
        return Err(Error::InvalidInput);
    }
    let mut halves = significand.split('.');
    let integer = halves.next().unwrap();
    let fraction = halves.next();
    if halves.next().is_some() {
        return Err(Error::InvalidInput);
    }
    let fraction = fraction.unwrap_or("");
    let all_digits = format!("{integer}{fraction}");
    if all_digits.is_empty() || !all_digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(Error::InvalidInput);
    }
    // Independent threshold: longer than the input plus every format's range.
    let threshold = s.len() as i128 + 20_000;
    let written = match exponent {
        None => 0,
        Some(text) => {
            let exp_neg = text.starts_with('-');
            let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
            if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return Err(Error::InvalidInput);
            }
            let magnitude = digits.bytes().fold(0_i128, |n, b| {
                (n * 10 + i128::from(b - b'0')).min(threshold)
            });
            if exp_neg { -magnitude } else { magnitude }
        }
    };
    let preferred = written - fraction.len() as i128;
    let significant = all_digits.trim_start_matches('0');
    let (p, lo, hi) = bounds(format);
    if significant.is_empty() {
        return Ok(Finite {
            neg,
            coeff: 0,
            exp: preferred.clamp(i128::from(lo), i128::from(hi)) as i32,
        });
    }
    let order = preferred + significant.len() as i128 - 1;
    if order > i128::from(hi) + i128::from(p) {
        return Err(Error::Overflow);
    }
    if order < i128::from(lo) - 1 {
        return match mode {
            None => Err(Error::Inexact),
            Some(dir) => Ok(Finite {
                neg,
                coeff: u128::from((dir == Rounding::TowardPositive) != neg),
                exp: lo,
            }),
        };
    }
    let normalized = significant.trim_end_matches('0');
    let source_exp = preferred + (significant.len() - normalized.len()) as i128;
    let c = BigUint::parse_bytes(normalized.as_bytes(), 10).unwrap();
    let value = BigInt::from_biguint(if neg { Sign::Minus } else { Sign::Plus }, c.clone());
    let mut result = round_integer(value, source_exp as i32, neg, format, mode, powers)?;
    let base = source_exp.min(i128::from(result.exp));
    let exact = &c * &*powers.pow((source_exp - base) as i64)
        == BigUint::from(result.coeff) * &*powers.pow((i128::from(result.exp) - base) as i64);
    if exact {
        let desired = preferred.clamp(i128::from(lo), i128::from(hi));
        while i128::from(result.exp) > desired && result.coeff < 10_u128.pow(p - 1) {
            result.coeff *= 10;
            result.exp -= 1;
        }
    }
    Ok(result)
}

fn check(s: &str, format: Format, powers: &Powers) {
    for mode in MODES {
        assert_eq!(
            actual(s, format, mode),
            directed(
                reference(s, format, mode, powers),
                s.starts_with('-'),
                mode,
                format
            ),
            "P={}, input length={}, input={:.160}, mode={mode:?}",
            bounds(format).0,
            s.len(),
            s
        );
    }
}

#[test]
fn parse_grammar_shapes_and_rejected_classes() {
    let powers = Powers::new(13_000);
    for format in FORMATS {
        for shape in ["5", "5.", ".5", "5.5"] {
            for sign in ["", "+", "-"] {
                for marker in ["", "e", "E"] {
                    if marker.is_empty() {
                        check(&format!("{sign}{shape}"), format, &powers);
                    } else {
                        for exp_sign in ["", "+", "-"] {
                            for exp in ["0", "2", "00002"] {
                                check(
                                    &format!("{sign}{shape}{marker}{exp_sign}{exp}"),
                                    format,
                                    &powers,
                                );
                            }
                        }
                    }
                }
            }
        }
        for s in [
            "", "+", "-", ".", "+.", "-.", " 1", "1 ", "1\t", "1\n", "1 2", "1_0", "１", "١",
            "snan", "nan(1)", "infin", "-in", "0x10", "1..0", "1.2.3", "1+2", "--1", "+-1", "1e",
            "1e+", "1e-", "e1", ".e1", "1e1e1", "1e1.0", "1e 1", "1e++1", "1\0",
        ] {
            assert_eq!(
                reference(s, format, None, &powers),
                Err(Error::InvalidInput)
            );
            check(s, format, &powers);
        }
    }
    assert_eq!(
        "-1.50".parse::<Decimal32>().unwrap().to_bits(),
        Decimal32::parse("-1.50").unwrap().to_bits()
    );
    assert_eq!(
        "-1.50".parse::<Decimal64>().unwrap().to_bits(),
        Decimal64::parse("-1.50").unwrap().to_bits()
    );
    assert_eq!(
        "-1.50".parse::<Decimal128>().unwrap().to_bits(),
        Decimal128::parse("-1.50").unwrap().to_bits()
    );
}

#[test]
fn parse_coefficient_exponent_sign_and_range_classes() {
    let powers = Powers::new(13_000);
    for format in FORMATS {
        let (p, lo, hi) = bounds(format);
        for coeff in coefficients(p) {
            for exp in [
                lo - 40,
                lo - 1,
                lo,
                lo + 1,
                -35,
                -1,
                0,
                1,
                hi - 1,
                hi,
                hi + 1,
            ] {
                for sign in ["", "-"] {
                    check(&format!("{sign}{coeff}e{exp}"), format, &powers);
                }
            }
        }
        for v in classes(format, format) {
            check(
                &format!("{}{}e{}", if v.neg { "-" } else { "" }, v.coeff, v.exp),
                format,
                &powers,
            );
        }
        for n in [34, 35, 36, 40] {
            for tail in ['0', '1', '9'] {
                let digits = format!("1{}{tail}", "0".repeat(n - 2));
                for exp in [
                    lo - n as i32,
                    lo - 1,
                    -(n as i32),
                    0,
                    hi - n as i32 + p as i32,
                ] {
                    for sign in ["", "-"] {
                        check(&format!("{sign}{digits}e{exp}"), format, &powers);
                    }
                }
            }
        }
        for delta in [-1, 0, 1] {
            let max = 10_u128.pow(p) - 1;
            for sign in ["", "-"] {
                check(
                    &format!("{sign}{}e{hi}", max as i128 + delta),
                    format,
                    &powers,
                );
                check(
                    &format!("{sign}{}e{}", max * 10 + 1, hi - 1),
                    format,
                    &powers,
                );
                check(&format!("{sign}{}e{}", delta + 2, lo - 1), format, &powers);
            }
        }
        for s in [
            "-0",
            "-0.00",
            "0e999999",
            "-0e-999999",
            "1e99999999999999999999999",
            "-1e-99999999999999999999999",
        ] {
            check(s, format, &powers);
        }
    }
}

#[test]
fn parse_unbounded_digits_saturation_and_scale_clamps() {
    let powers = Powers::new(13_000);
    for format in FORMATS {
        for n in [10_000, 1_000_000] {
            for s in [
                format!("{}1.50", "0".repeat(n)),
                format!("-1.5{}", "0".repeat(n)),
                format!("1{}e-{n}", "0".repeat(n)),
                format!("0.{}1e{n}", "0".repeat(n)),
                format!("{}e-99999999999999999999999", "9".repeat(n)),
            ] {
                check(&s, format, &powers);
            }
        }
        for threshold in [1_i128 << 40, 1_i128 << 64] {
            for sign in [-1, 1] {
                for delta in [-2, -1, 0, 1, 2] {
                    let exponent = sign * threshold + delta;
                    for digits in [
                        "1".to_string(),
                        format!("1{}", "0".repeat(40)),
                        format!("0.{}1", "0".repeat(40)),
                    ] {
                        check(&format!("{digits}e{exponent}"), format, &powers);
                        check(&format!("-{digits}e{exponent}"), format, &powers);
                    }
                }
            }
        }
    }
}

#[test]
fn parse_long_nonzero_significands_in_range() {
    let powers = Powers::new(13_000);
    let inputs = [
        format!("1.{}", "1".repeat(7_000)),
        format!("-0.{}", "9".repeat(10_000)),
        format!("{}e-4990", "9".repeat(5_000)),
        format!("-{}e-10780", "123456789".repeat(1_200)),
        format!("1{}1e-3001", "0".repeat(3_000)),
        format!("0.{}{}", "0".repeat(6_000), "1".repeat(6_000)),
        format!("{}e-99990", "7".repeat(100_000)),
    ];
    for format in FORMATS {
        for s in &inputs {
            check(s, format, &powers);
        }
    }
}

#[test]
fn parse_accumulator_chunk_and_exact_dispatch_boundaries() {
    let powers = Powers::new(13_000);
    for format in FORMATS {
        let (p, lo, hi) = bounds(format);
        for n in [1, 7, 8, 9, 15, 16, 17, 19, 20, 28, 29, 33, 34, 35, 36, 37] {
            for digit in ['0', '1', '9'] {
                let digits = digit.to_string().repeat(n);
                for point in 0..=n + 1 {
                    let mut significand = digits.clone();
                    if point <= n {
                        significand.insert(point, '.');
                    }
                    for sign in ["", "+", "-"] {
                        let text = format!("{sign}{significand}");
                        check(&text, format, &powers);
                        for exponent in ["e0", "E+1", "e-1"] {
                            check(&format!("{text}{exponent}"), format, &powers);
                        }
                    }
                }
            }
        }
        for coeff in [0, 1, 10_u128.pow(p) - 1, 10_u128.pow(p)] {
            for exp in [lo - 1, lo, lo + 1, hi - 1, hi, hi + 1] {
                for sign in ["", "-"] {
                    check(&format!("{sign}{coeff}e{exp}"), format, &powers);
                }
            }
        }
        for n in [7, 8, 9, 16, 17, 19, 20, 34, 35, 36, 37, 100] {
            for sign in ["", "-"] {
                check(&format!("{sign}{}.00", "0".repeat(n)), format, &powers);
                check(&format!("{sign}{}1", "0".repeat(n)), format, &powers);
            }
        }
    }
}

#[test]
fn parse_invalid_bytes_at_each_chunk_position() {
    let powers = Powers::new(13_000);
    for format in FORMATS {
        for start in [0, 8, 16, 24, 32] {
            for pos in start..start + 8 {
                for byte in 0_u8..=127 {
                    if byte.is_ascii_digit() {
                        continue;
                    }
                    let mut text = "1".repeat(if pos < 32 { 32 } else { 40 });
                    text.replace_range(pos..pos + 1, &char::from(byte).to_string());
                    check(&text, format, &powers);
                    let short = text.chars().take(pos + 1).collect::<String>();
                    check(&short, format, &powers);
                }
                for invalid in ["\u{80}", "é", "١", "１", "🦀"] {
                    let mut text = "1".repeat(if pos < 32 { 32 } else { 40 });
                    text.replace_range(pos..pos + 1, invalid);
                    check(&text, format, &powers);
                    check(&text[..pos + invalid.len()], format, &powers);
                }
            }
        }
    }
}

pub(crate) fn fuzz_valid(rng: &mut Rng) -> String {
    let count = (rng.next() % 80 + 1) as usize;
    let point = (rng.next() % (count as u64 + 2)) as usize;
    let mut s = String::new();
    s.push_str(["", "+", "-"][(rng.next() % 3) as usize]);
    for i in 0..=count {
        if i == point {
            s.push('.');
        }
        if i < count {
            s.push((b'0' + (rng.next() % 10) as u8) as char);
        }
    }
    if rng.next() & 1 != 0 {
        s.push(if rng.next() & 1 == 0 { 'e' } else { 'E' });
        s.push_str(["", "+", "-"][(rng.next() % 3) as usize]);
        s.push_str("000");
        s.push_str(&(rng.next() % 13_000).to_string());
    }
    s
}

pub(crate) fn fuzz_invalid(rng: &mut Rng) -> String {
    let mut s = fuzz_valid(rng);
    let invalid = [" ", "_", "x", "\0", "١", "\n", "NaN"][(rng.next() % 7) as usize];
    let position = (rng.next() % (s.len() as u64 + 1)) as usize;
    s.insert_str(position, invalid);
    s
}

fn fuzz(format: Format, count: u64, seed: u64, powers: &Powers) {
    let mut rng = Rng(seed);
    for _ in 0..count {
        check(&fuzz_valid(&mut rng), format, powers);
        let invalid = fuzz_invalid(&mut rng);
        assert_eq!(
            reference(&invalid, format, None, powers),
            Err(Error::InvalidInput)
        );
        check(&invalid, format, powers);
    }
}

#[test]
fn parse_valid_and_invalid_grammar_fuzzer_ten_thousand_per_width_mode() {
    let powers = Powers::new(13_000);
    for format in FORMATS {
        fuzz(format, 10_000, 0xd892_7104_47ee_72af, &powers);
    }
}

#[test]
#[ignore = "one million valid and one million invalid strings per width, all three modes"]
fn parse_valid_and_invalid_grammar_fuzzer_million_per_width_mode() {
    let started = Instant::now();
    let powers = Powers::new(13_000);
    for format in FORMATS {
        fuzz(format, 1_000_000, 0x93a8_441d_889e_cc10, &powers);
    }
    std::eprintln!(
        "parse million valid and invalid per width/mode: {:?}",
        started.elapsed()
    );
}

fn short_fields(
    result: Result<crate::layout::Unpacked, Error>,
) -> Result<(bool, u128, i32), Error> {
    result.map(|v| (v.neg, v.coeff, v.exp))
}

type ParseAt = fn(&str, Option<Rounding>) -> Result<Unpacked, Error>;

struct Parsers {
    digits: u32,
    fast: ParseAt,
    general: ParseAt,
}

fn parse_32_at(s: &str, dir: Option<Rounding>) -> Result<Unpacked, Error> {
    crate::parse::parse_32(s, dir).map(InRange::unpacked)
}

fn parse_64_at(s: &str, dir: Option<Rounding>) -> Result<Unpacked, Error> {
    crate::parse::parse_64(s, dir).map(InRange::unpacked)
}

fn parse_at<W: Width>(s: &str, dir: Option<Rounding>) -> Result<Unpacked, Error> {
    crate::parse::parse::<W>(s, dir).map(InRange::unpacked)
}

fn general_at<W: Width>(s: &str, dir: Option<Rounding>) -> Result<Unpacked, Error> {
    crate::parse::parse_general::<W>(s, dir).map(InRange::unpacked)
}

const D32_PARSERS: Parsers = Parsers {
    digits: W32::LAYOUT.digits,
    fast: parse_32_at,
    general: general_at::<W32>,
};

const D64_PARSERS: Parsers = Parsers {
    digits: W64::LAYOUT.digits,
    fast: parse_64_at,
    general: general_at::<W64>,
};

const D128_PARSERS: Parsers = Parsers {
    digits: W128::LAYOUT.digits,
    fast: parse_at::<W128>,
    general: general_at::<W128>,
};

fn short_strings(
    text: &mut String,
    parsers: &Parsers,
    tokens: &[&str],
    max: usize,
    count: &mut u64,
) {
    for mode in MODES {
        assert_eq!(
            short_fields((parsers.fast)(text, mode)),
            short_fields((parsers.general)(text, mode)),
            "P={}, input={text:?}, mode={mode:?}",
            parsers.digits
        );
    }
    *count += 1;
    let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text).len();
    for token in tokens {
        // The unsigned root never starts with `-`; that string belongs to the `-` sign root.
        if text.is_empty() && *token == "-" {
            continue;
        }
        if unsigned + token.len() <= max {
            text.push_str(token);
            short_strings(text, parsers, tokens, max, count);
            text.truncate(text.len() - token.len());
        }
    }
}

fn short_significands(parsers: &Parsers, tokens: &[&str], max: usize) {
    let started = Instant::now();
    let mut total = 0;
    for sign in ["", "+", "-"] {
        short_strings(&mut String::from(sign), parsers, tokens, max, &mut total);
    }
    std::eprintln!(
        "P={} significands up to {max} bytes against the general parser: {} inputs, {:?}",
        parsers.digits,
        total,
        started.elapsed()
    );
}

#[test]
fn parse_short_significands_match_general_parser() {
    let tokens = ["0", "7", "9", ".", "e", "-", "/", ":", "\u{ae}"];
    short_significands(&D32_PARSERS, &tokens, 8);
    let tokens = ["0", "9", ".", "e", ":"];
    short_significands(&D64_PARSERS, &tokens, 10);
    short_significands(&D128_PARSERS, &tokens, 10);
    for parsers in [&D32_PARSERS, &D64_PARSERS, &D128_PARSERS] {
        short_significands(parsers, &["9", "."], 22);
    }
}

fn short_reference(s: &str, format: Format, parsers: &Parsers, powers: &Powers) {
    check(s, format, powers);
    for mode in MODES {
        assert_eq!(
            short_fields((parsers.fast)(s, mode)),
            short_fields((parsers.general)(s, mode)),
            "P={}, input={s:?}, mode={mode:?}",
            parsers.digits
        );
    }
}

#[test]
fn parse_short_significands_match_independent_reference() {
    let powers = Powers::new(13_000);
    let boundaries = [
        "/", ":", ".", "e", "E", "\u{0}", "\u{7f}", "\u{80}", "\u{ae}",
    ];
    let mut count = 0_u64;
    for (format, parsers, max) in [
        (Format::D32, &D32_PARSERS, 8),
        (Format::D64, &D64_PARSERS, 17),
        (Format::D128, &D128_PARSERS, 8),
    ] {
        for sign in ["", "+", "-"] {
            for len in 1..=max {
                for point in core::iter::once(None).chain((0..len).map(Some)) {
                    for fill in ["0", "9", "1234567890"] {
                        let base: std::vec::Vec<char> = (0..len)
                            .map(|pos| match point {
                                Some(at) if at == pos => '.',
                                _ => char::from(fill.as_bytes()[pos % fill.len()]),
                            })
                            .collect();
                        let text: String = base.iter().collect();
                        short_reference(&format!("{sign}{text}"), format, parsers, &powers);
                        count += 1;
                        for pos in 0..len {
                            for byte in boundaries {
                                let mut text = String::new();
                                for (at, c) in base.iter().enumerate() {
                                    if at == pos {
                                        text.push_str(byte);
                                    } else {
                                        text.push(*c);
                                    }
                                }
                                short_reference(&format!("{sign}{text}"), format, parsers, &powers);
                                count += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    std::eprintln!("short significands against the independent reference: {count} inputs");
}
