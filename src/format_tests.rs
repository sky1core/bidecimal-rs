use core::fmt::{self, Write};
use std::format;
use std::string::{String, ToString};
use std::time::Instant;

use crate::parse_tests;
use crate::refbid::{self, Classified, Finite, Format};
use crate::test_support::*;
use crate::{Decimal32, Error};

pub(crate) fn reference(v: Finite, strip: bool) -> String {
    let mut digits = v.coeff.to_string();
    if v.exp >= 0 {
        if v.coeff != 0 {
            digits.push_str(&"0".repeat(v.exp as usize));
        }
    } else {
        let fraction = (-v.exp) as usize;
        if digits.len() <= fraction {
            digits = format!("0.{}{digits}", "0".repeat(fraction - digits.len()));
        } else {
            digits.insert(digits.len() - fraction, '.');
        }
        if strip {
            digits = digits
                .trim_end_matches('0')
                .trim_end_matches('.')
                .to_string();
        }
    }
    if v.neg {
        digits.insert(0, '-');
    }
    digits
}

pub(crate) fn actual(v: Finite, format: Format, strip: bool) -> String {
    macro_rules! text {
        ($v:expr) => {
            if strip {
                format!("{}", $v.stripped())
            } else {
                format!("{}", $v)
            }
        };
    }
    match format {
        Format::D32 => text!(d32(v)),
        Format::D64 => text!(d64(v)),
        Format::D128 => text!(d128(v)),
    }
}

fn parsed_cohort(mut v: Finite, format: Format, strip: bool) -> Finite {
    if strip && v.exp < 0 {
        if v.coeff == 0 {
            v.exp = 0;
        } else {
            while v.exp < 0 && v.coeff.is_multiple_of(10) {
                v.coeff /= 10;
                v.exp += 1;
            }
        }
    }
    if v.exp > 0 {
        if v.coeff == 0 {
            v.exp = 0;
        } else {
            let p = bounds(format).0;
            while v.exp > 0 && v.coeff < 10_u128.pow(p - 1) {
                v.coeff *= 10;
                v.exp -= 1;
            }
        }
    }
    v
}

fn check(v: Finite, format: Format, powers: &Powers) {
    let v = decoded(refbid::encode(v, format), format);
    for strip in [false, true] {
        let text = actual(v, format, strip);
        assert_eq!(text, reference(v, strip), "v={v:?}, strip={strip}");
        let expected = parsed_cohort(v, format, strip);
        assert_eq!(
            parse_tests::actual(&text, format, None),
            Ok(Value::Finite(expected))
        );
        for mode in MODES {
            assert_eq!(
                parse_tests::actual(&text, format, mode),
                directed(
                    parse_tests::reference(&text, format, mode, powers),
                    text.starts_with('-'),
                    mode,
                    format
                )
            );
        }
    }
}

#[test]
fn format_all_coefficient_exponent_sign_classes_and_extreme_lengths() {
    let powers = Powers::new(13_000);
    for format in FORMATS {
        for v in classes(format, format) {
            check(v, format, &powers);
        }
        for exp in [bounds(format).1, -4, -2, -1, 0, 1, bounds(format).2] {
            for coeff in [0, 1, 10, 15, 100, 15000] {
                for neg in [false, true] {
                    check(Finite { neg, coeff, exp }, format, &powers);
                }
            }
        }
    }
    let v = Finite {
        neg: true,
        coeff: 1,
        exp: -6176,
    };
    let text = actual(v, Format::D128, false);
    assert_eq!(text.len(), 6179);
    assert_eq!(text, format!("-0.{}1", "0".repeat(6175)));
    for text in ["1.5000", "2.000", "-0.00", "1500", "-0"] {
        let v = crate::Decimal128::parse(text).unwrap();
        assert_eq!(format!("{v}"), text);
    }
}

struct ReferenceFlags(String);
impl fmt::Display for ReferenceFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let neg = self.0.starts_with('-');
        let magnitude = self.0.strip_prefix('-').unwrap_or(&self.0);
        let sign = if neg {
            "-"
        } else if f.sign_plus() {
            "+"
        } else {
            ""
        };
        let padding = f
            .width()
            .unwrap_or(0)
            .saturating_sub(sign.len() + magnitude.len());
        if f.sign_aware_zero_pad() {
            return f.write_str(&format!("{sign}{}{magnitude}", "0".repeat(padding)));
        }
        let (left, right) = match f.align().unwrap_or(fmt::Alignment::Right) {
            fmt::Alignment::Left => (0, padding),
            fmt::Alignment::Center => (padding / 2, padding - padding / 2),
            fmt::Alignment::Right => (padding, 0),
        };
        f.write_str(&format!(
            "{}{sign}{magnitude}{}",
            f.fill().to_string().repeat(left),
            f.fill().to_string().repeat(right)
        ))
    }
}

fn flags(v: impl fmt::Display + fmt::Debug, expected: String) {
    let r = ReferenceFlags(expected.clone());
    macro_rules! compare {
        ($($spec:literal),* $(,)?) => { $(assert_eq!(format!($spec, v), format!($spec, r), "spec={}", $spec);)* };
    }
    compare!(
        "{}",
        "{:1}",
        "{:20}",
        "{:<1}",
        "{:^1}",
        "{:>1}",
        "{:<20}",
        "{:^20}",
        "{:>20}",
        "{:*<20}",
        "{:*^20}",
        "{:*>20}",
        "{:🦀<20}",
        "{:🦀^20}",
        "{:🦀>20}",
        "{:+}",
        "{:-}",
        "{:+1}",
        "{:+20}",
        "{:+020}",
        "{:020}",
        "{:<020}",
        "{:^020}",
        "{:>020}",
        "{:*<+020}",
        "{:*^+020}",
        "{:*>+020}",
        "{:*<+20}",
        "{:*^+20}",
        "{:*>+20}",
        "{:.0}",
        "{:.2}",
        "{:20.0}",
        "{:+020.2}",
        "{:🦀^+20.2}",
        "{:#}"
    );
    macro_rules! matrix {
        ($alignment:literal, $sign:literal, $zero:literal) => {
            for width in [1, 20, 6200] {
                for precision in [0, 2] {
                    assert_eq!(
                        format!(
                            concat!("{:", $alignment, $sign, $zero, "width$.precision$}"),
                            v,
                            width = width,
                            precision = precision
                        ),
                        format!(
                            concat!("{:", $alignment, $sign, $zero, "width$.precision$}"),
                            r,
                            width = width,
                            precision = precision
                        )
                    );
                }
            }
        };
    }
    macro_rules! signs {
        ($alignment:literal) => {
            matrix!($alignment, "", "");
            matrix!($alignment, "+", "");
            matrix!($alignment, "-", "");
            matrix!($alignment, "", "0");
            matrix!($alignment, "+", "0");
            matrix!($alignment, "-", "0");
        };
    }
    signs!("");
    signs!("<");
    signs!("^");
    signs!(">");
    signs!("*<");
    signs!("*^");
    signs!("*>");
    signs!("🦀<");
    signs!("🦀^");
    signs!("🦀>");
    assert_eq!(format!("{v:?}"), expected);
    assert_eq!(format!("{v:+020.2?}"), format!("{v:+020.2}"));
    assert_eq!(format!("{v:*^20?}"), format!("{v:*^20}"));
    assert_eq!(format!("{v:#?}"), expected);
}

#[test]
fn format_flags_precision_ignored_and_debug_equals_display() {
    let powers = Powers::new(13_000);
    for format in FORMATS {
        for text in [
            "1.5000", "2.000", "-1.5000", "0", "-0", "-0.00", "1500", "0.00001",
        ] {
            let v = parse_tests::reference(text, format, None, &powers).unwrap();
            for strip in [false, true] {
                let expected = reference(v, strip);
                macro_rules! test {
                    ($value:expr) => {
                        if strip {
                            flags($value.stripped(), expected);
                        } else {
                            flags($value, expected);
                        }
                    };
                }
                match format {
                    Format::D32 => test!(d32(v)),
                    Format::D64 => test!(d64(v)),
                    Format::D128 => test!(d128(v)),
                }
            }
        }
        let v = Finite {
            neg: true,
            coeff: 1,
            exp: bounds(format).1,
        };
        match format {
            Format::D32 => flags(d32(v), reference(v, false)),
            Format::D64 => flags(d64(v), reference(v, false)),
            Format::D128 => flags(d128(v), reference(v, false)),
        }
    }
}

struct Stack {
    bytes: [u8; 6179],
    used: usize,
}
impl Stack {
    fn new() -> Self {
        Self {
            bytes: [0; 6179],
            used: 0,
        }
    }
    fn text(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.used]).unwrap()
    }
}
impl Write for Stack {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let end = self.used.checked_add(text.len()).ok_or(fmt::Error)?;
        self.bytes
            .get_mut(self.used..end)
            .ok_or(fmt::Error)?
            .copy_from_slice(text.as_bytes());
        self.used = end;
        Ok(())
    }
}

#[test]
fn format_writer_errors_propagate_on_direct_and_padded_paths() {
    struct Reject;
    impl Write for Reject {
        fn write_str(&mut self, _: &str) -> fmt::Result {
            Err(fmt::Error)
        }
    }
    for format in FORMATS {
        let v = Finite {
            neg: true,
            coeff: 15000,
            exp: -4,
        };
        macro_rules! reject {
            ($value:expr) => {{
                assert!(write!(Reject, "{}", $value).is_err());
                assert!(write!(Reject, "{:020}", $value).is_err());
                assert!(write!(Reject, "{}", $value.stripped()).is_err());
                assert!(write!(Reject, "{:020}", $value.stripped()).is_err());
            }};
        }
        match format {
            Format::D32 => reject!(d32(v)),
            Format::D64 => reject!(d64(v)),
            Format::D128 => reject!(d128(v)),
        }
    }
    let v = crate::Decimal128::parse("-1e-6176").unwrap();
    let mut stack = Stack::new();
    write!(stack, "{v:+06180}").unwrap_err();
    assert!(stack.used <= 6179);
}

#[test]
fn format_parse_random_two_thousand_per_width() {
    let powers = Powers::new(13_000);
    let mut rng = Rng(0xab12_8093_1717_ddea);
    for format in FORMATS {
        for _ in 0..2_000 {
            check(rng.value(format), format, &powers);
        }
    }
}

#[test]
#[ignore = "one million values per width, plain and stripped, all three modes"]
fn format_parse_million_per_width_independent_references() {
    let started = Instant::now();
    let powers = Powers::new(13_000);
    for format in FORMATS {
        let mut rng = Rng(0x9912_6c88_03dd_ace7);
        for _ in 0..1_000_000 {
            check(rng.value(format), format, &powers);
        }
    }
    std::eprintln!("format/parse million per width: {:?}", started.elapsed());
}

#[test]
#[ignore = "all 2^32 Decimal32 bit patterns, plain and stripped"]
fn decimal32_exhaustive_format_parse_round_trip() {
    let started = Instant::now();
    let mut text = Stack::new();
    for bits in 0..1_u64 << 32 {
        match (
            Decimal32::from_bits(bits as u32),
            refbid::classify(u128::from(bits), Format::D32),
        ) {
            (Ok(value), Classified::Finite(v)) => {
                for strip in [false, true] {
                    text.used = 0;
                    if strip {
                        write!(text, "{}", value.stripped()).unwrap();
                    } else {
                        write!(text, "{value}").unwrap();
                    }
                    let parsed = Decimal32::parse(text.text()).unwrap();
                    let expected = parsed_cohort(v, Format::D32, strip);
                    assert_eq!(
                        u128::from(parsed.to_bits()),
                        refbid::encode(expected, Format::D32),
                        "bits={bits:x}, strip={strip}"
                    );
                }
            }
            (Ok(value), Classified::Infinity | Classified::Nan) => {
                assert!(
                    canonical_special(u128::from(bits), Format::D32),
                    "bits={bits:x}"
                );
                text.used = 0;
                write!(text, "{value}").unwrap();
                let parsed = Decimal32::parse(text.text()).unwrap();
                assert_eq!(parsed.is_nan(), value.is_nan(), "bits={bits:x}");
                assert_eq!(parsed.is_infinite(), value.is_infinite(), "bits={bits:x}");
                assert_eq!(
                    parsed.is_sign_negative(),
                    value.is_sign_negative() && value.is_infinite(),
                    "bits={bits:x}"
                );
            }
            (
                Err(Error::InvalidInput),
                Classified::NonCanonical | Classified::Infinity | Classified::Nan,
            ) => {
                assert!(
                    !canonical_special(u128::from(bits), Format::D32),
                    "bits={bits:x}"
                );
            }
            (actual, expected) => {
                panic!("bits={bits:x}, actual={actual:?}, expected={expected:?}")
            }
        }
    }
    std::eprintln!(
        "Decimal32 exhaustive plain+stripped format/parse: {:?}",
        started.elapsed()
    );
}
