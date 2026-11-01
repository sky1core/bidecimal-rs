use serde::de::value::{Error as ValueError, I128Deserializer, U128Deserializer};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_test::{
    Configure, Readable, Token, assert_de_tokens, assert_de_tokens_error, assert_ser_tokens,
    assert_tokens,
};
use std::format;
use std::string::ToString;

use crate::{Decimal32, Decimal64, Decimal128, OrderedDecimal};

trait Bits: Copy {
    fn bits(self) -> u128;
}

impl<T: Bits> Bits for OrderedDecimal<T> {
    fn bits(self) -> u128 {
        self.0.bits()
    }
}

#[derive(Debug)]
struct Bitwise<T>(T);

impl<T: Bits> PartialEq for Bitwise<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0.bits() == other.0.bits()
    }
}

impl<T: Serialize> Serialize for Bitwise<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Bitwise<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(Self)
    }
}

macro_rules! serde_width_tests {
    ($module:ident, $decimal:ty, $bits:ty) => {
        mod $module {
            use super::*;

            impl Bits for $decimal {
                fn bits(self) -> u128 {
                    self.to_bits() as u128
                }
            }

            pub(super) fn samples() -> std::vec::Vec<$decimal> {
                let mut values = std::vec![];
                for text in [
                    "1.50", "1.5", "-1.50", "-1.5", "0", "-0", "0.00", "-0.00", "1E+5",
                    "-1E+5", "inf", "-inf", "NaN", "-NaN",
                ] {
                    values.push(<$decimal>::parse(text).unwrap());
                }
                values.extend([
                    <$decimal>::MAX,
                    <$decimal>::MIN,
                    <$decimal>::parse(&format!("1e{}", <$decimal>::MIN_EXP)).unwrap(),
                    <$decimal>::parse(&format!("-0e{}", <$decimal>::MAX_EXP)).unwrap(),
                ]);
                for negative in [false, true] {
                    for signaling in [false, true] {
                        for payload in [0, 1, 10_u128.pow(<$decimal>::DIGITS - 1) - 1] {
                            let bits = <$decimal>::NAN.to_bits()
                                | ((negative as $bits) << (<$bits>::BITS - 1))
                                | ((signaling as $bits) << (<$bits>::BITS - 7))
                                | payload as $bits;
                            values.push(<$decimal>::from_bits(bits).unwrap());
                        }
                    }
                }
                values
            }

            fn check_readable(value: $decimal, text: &'static str) {
                assert_eq!(value.to_string(), text);
                let parsed = <$decimal>::parse(text).unwrap();
                for token in [Token::Str(text), Token::BorrowedStr(text), Token::String(text)] {
                    assert_de_tokens(&Bitwise(parsed).readable(), &[token]);
                    assert_de_tokens(&Bitwise(OrderedDecimal(parsed)).readable(), &[token]);
                }
                assert_ser_tokens(&value.readable(), &[Token::Str(text)]);
                assert_ser_tokens(&OrderedDecimal(value).readable(), &[Token::Str(text)]);
                assert_tokens(&Bitwise(parsed).readable(), &[Token::Str(text)]);
            }

            #[test]
            fn readable_display_and_parse_bits() {
                for text in ["1.50", "1.5", "-1.50", "-1.5", "0", "-0", "0.00", "-0.00", "inf", "-inf"] {
                    check_readable(<$decimal>::parse(text).unwrap(), text);
                }
                check_readable(<$decimal>::parse("1E+5").unwrap(), "100000");
                check_readable(<$decimal>::parse("-1E+5").unwrap(), "-100000");
                for value in samples().into_iter().filter(|value| value.is_nan()) {
                    check_readable(value, "NaN");
                }
                assert_ne!(
                    <$decimal>::parse("1E+5").unwrap().to_bits(),
                    <$decimal>::parse("100000").unwrap().to_bits()
                );
            }

            #[test]
            fn readable_integer_boundaries_match_parse() {
                let boundary = 10_u128.pow(<$decimal>::DIGITS);
                for value in [
                    0, 1, boundary - 1, boundary, boundary + 1, boundary * 10,
                    boundary * 10 + 1, u64::MAX as u128, u128::MAX,
                ].into_iter().chain((0..=38).flat_map(|exponent| {
                    let power = 10_u128.pow(exponent);
                    [power - 1, power, power + 1]
                })) {
                    let expected = <$decimal>::parse(&value.to_string()).map(|v| v.to_bits());
                    let actual = <$decimal>::deserialize(
                        U128Deserializer::<ValueError>::new(value).readable(),
                    );
                    let wrapped = OrderedDecimal::<$decimal>::deserialize(
                        U128Deserializer::<ValueError>::new(value).readable(),
                    );
                    assert_eq!(
                        actual.as_ref().map(|v| v.to_bits()).map_err(ToString::to_string),
                        wrapped.map(|v| v.0.to_bits()).map_err(|error| error.to_string())
                    );
                    match expected {
                        Ok(bits) => assert_eq!(actual.unwrap().to_bits(), bits),
                        Err(error) => assert!(actual.unwrap_err().to_string().contains(&error.to_string())),
                    }
                    if let Ok(value) = u64::try_from(value) {
                        match expected {
                            Ok(bits) => {
                                let parsed = <$decimal>::from_bits(bits).unwrap();
                                assert_de_tokens(&Bitwise(parsed).readable(), &[Token::U64(value)]);
                                assert_de_tokens(&Bitwise(OrderedDecimal(parsed)).readable(), &[Token::U64(value)]);
                            }
                            Err(error) => assert_de_tokens_error::<Readable<$decimal>>(
                                &[Token::U64(value)],
                                &format!("invalid value: integer `{value}`, expected {} ({error})", stringify!($decimal)),
                            ),
                        }
                    }
                }
                for value in [
                    0, 1, -1, boundary as i128 - 1, boundary as i128,
                    boundary as i128 + 1, -(boundary as i128) + 1,
                    -(boundary as i128), -(boundary as i128) - 1,
                    i64::MIN as i128, i64::MAX as i128, i128::MIN, i128::MAX,
                ].into_iter().chain((0..=38).flat_map(|exponent| {
                    let power = 10_i128.pow(exponent);
                    [power - 1, power, power + 1, -power + 1, -power, -power - 1]
                })) {
                    let expected = <$decimal>::parse(&value.to_string()).map(|v| v.to_bits());
                    let actual = <$decimal>::deserialize(
                        I128Deserializer::<ValueError>::new(value).readable(),
                    );
                    let wrapped = OrderedDecimal::<$decimal>::deserialize(
                        I128Deserializer::<ValueError>::new(value).readable(),
                    );
                    assert_eq!(
                        actual.as_ref().map(|v| v.to_bits()).map_err(ToString::to_string),
                        wrapped.map(|v| v.0.to_bits()).map_err(|error| error.to_string())
                    );
                    match expected {
                        Ok(bits) => assert_eq!(actual.unwrap().to_bits(), bits),
                        Err(error) => assert!(actual.unwrap_err().to_string().contains(&error.to_string())),
                    }
                    if let Ok(value) = i64::try_from(value) {
                        match expected {
                            Ok(bits) => {
                                let parsed = <$decimal>::from_bits(bits).unwrap();
                                assert_de_tokens(&Bitwise(parsed).readable(), &[Token::I64(value)]);
                                assert_de_tokens(&Bitwise(OrderedDecimal(parsed)).readable(), &[Token::I64(value)]);
                            }
                            Err(error) => assert_de_tokens_error::<Readable<$decimal>>(
                                &[Token::I64(value)],
                                &format!("invalid value: integer `{value}`, expected {} ({error})", stringify!($decimal)),
                            ),
                        }
                    }
                }
            }

            #[test]
            fn integer_direct_path_matches_exact_parser_random_bits() {
                let mut bits = 0xb6e1_a0c3_5544_9876_1234_5678_9abc_def0u128;
                for _ in 0..10000 {
                    bits ^= bits << 23;
                    bits ^= bits >> 17;
                    bits ^= bits << 26;
                    let expected = <$decimal>::parse(&bits.to_string()).map(|v| v.to_bits());
                    let actual = <$decimal>::deserialize(U128Deserializer::<ValueError>::new(bits).readable());
                    match expected {
                        Ok(expected) => assert_eq!(actual.unwrap().to_bits(), expected),
                        Err(error) => assert!(actual.unwrap_err().to_string().contains(&error.to_string())),
                    }
                    let signed = bits as i128;
                    let expected = <$decimal>::parse(&signed.to_string()).map(|v| v.to_bits());
                    let actual = <$decimal>::deserialize(I128Deserializer::<ValueError>::new(signed).readable());
                    match expected {
                        Ok(expected) => assert_eq!(actual.unwrap().to_bits(), expected),
                        Err(error) => assert!(actual.unwrap_err().to_string().contains(&error.to_string())),
                    }
                }
            }

            #[test]
            fn readable_rejects_invalid_inexact_overflow_and_floats() {
                for text in ["", "1.2.3", " 1", "sNaN", "NaN(1)", "1.2345678901234567890123456789012345", "1e99999"] {
                    let error = <$decimal>::parse(text).unwrap_err();
                    let message = format!("invalid value: string {text:?}, expected {} ({error})", stringify!($decimal));
                    assert_de_tokens_error::<Readable<$decimal>>(&[Token::Str(text)], &message);
                    assert_de_tokens_error::<Readable<OrderedDecimal<$decimal>>>(&[Token::Str(text)], &message);
                }
                for token in [Token::F32(1.5), Token::F64(1.5)] {
                    let message = "invalid type: floating point `1.5`, expected a decimal string (send floating-point values as strings)";
                    assert_de_tokens_error::<Readable<$decimal>>(&[token], message);
                    assert_de_tokens_error::<Readable<OrderedDecimal<$decimal>>>(&[token], message);
                }
            }
        }
    };
}

serde_width_tests!(decimal32, Decimal32, u32);
serde_width_tests!(decimal64, Decimal64, u64);
serde_width_tests!(decimal128, Decimal128, u128);
