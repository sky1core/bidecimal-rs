use core::fmt;

use serde::de::{self, Unexpected, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{Decimal32, Decimal64, Decimal128, Error, OrderedDecimal};

struct FailedValue(&'static str, Error);

/// Fit an integer directly, preserving the exact parser's preferred exponent.
/// Reducing the sole out-of-domain coefficient range retains discarded digits
/// as sticky information; fit performs the only precision reduction.
fn integer<W: crate::layout::Width>(
    negative: bool,
    mut coefficient: u128,
) -> Result<crate::layout::InRange<W>, Error> {
    if let Some(value) = crate::layout::InRange::<W>::checked(negative, coefficient, 0) {
        return Ok(value);
    }
    let mut exponent = 0;
    let mut sticky = false;
    if coefficient > i128::MAX as u128 {
        sticky = !coefficient.is_multiple_of(10);
        coefficient /= 10;
        exponent = 1;
    }
    crate::kernel::fit(
        crate::kernel::Wide {
            neg: negative,
            coeff: coefficient,
            exp: exponent,
            sticky,
        },
        0,
        None,
    )
}

impl de::Expected for FailedValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} ({})", self.0, self.1)
    }
}

macro_rules! decimal_serde {
    ($decimal:ty, $width:ty, $encode:ident) => {
        impl Serialize for $decimal {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.collect_str(self)
            }
        }

        impl<'de> Deserialize<'de> for $decimal {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                struct DecimalVisitor;

                impl<'de> Visitor<'de> for DecimalVisitor {
                    type Value = $decimal;

                    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                        formatter.write_str(concat!(
                            "a ",
                            stringify!($decimal),
                            " string or an exact integer"
                        ))
                    }

                    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                        <$decimal>::parse(value).map_err(|error| {
                            E::invalid_value(
                                Unexpected::Str(value),
                                &FailedValue(stringify!($decimal), error),
                            )
                        })
                    }

                    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
                        integer::<$width>(value < 0, value.unsigned_abs() as u128)
                            .and_then(|value| <$decimal>::from_bits(crate::layout::$encode(value)))
                            .map_err(|error| {
                                E::invalid_value(
                                    Unexpected::Signed(value),
                                    &FailedValue(stringify!($decimal), error),
                                )
                            })
                    }

                    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
                        integer::<$width>(false, value as u128)
                            .and_then(|value| <$decimal>::from_bits(crate::layout::$encode(value)))
                            .map_err(|error| {
                                E::invalid_value(
                                    Unexpected::Unsigned(value),
                                    &FailedValue(stringify!($decimal), error),
                                )
                            })
                    }

                    fn visit_i128<E: de::Error>(self, value: i128) -> Result<Self::Value, E> {
                        integer::<$width>(value < 0, value.unsigned_abs())
                            .and_then(|value| <$decimal>::from_bits(crate::layout::$encode(value)))
                            .map_err(|error| {
                                E::invalid_value(
                                    Unexpected::Other("i128 integer"),
                                    &FailedValue(stringify!($decimal), error),
                                )
                            })
                    }

                    fn visit_u128<E: de::Error>(self, value: u128) -> Result<Self::Value, E> {
                        integer::<$width>(false, value)
                            .and_then(|value| <$decimal>::from_bits(crate::layout::$encode(value)))
                            .map_err(|error| {
                                E::invalid_value(
                                    Unexpected::Other("u128 integer"),
                                    &FailedValue(stringify!($decimal), error),
                                )
                            })
                    }

                    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
                        Err(E::invalid_type(
                            Unexpected::Float(value),
                            &"a decimal string (send floating-point values as strings)",
                        ))
                    }
                }

                deserializer.deserialize_any(DecimalVisitor)
            }
        }
    };
}

decimal_serde!(Decimal32, crate::layout::W32, encode_32);
decimal_serde!(Decimal64, crate::layout::W64, encode_64);
decimal_serde!(Decimal128, crate::layout::W128, encode_128);

impl<T: Serialize> Serialize for OrderedDecimal<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for OrderedDecimal<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(Self)
    }
}
