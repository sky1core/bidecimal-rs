use core::fmt;

mod lane;

use serde::de::{self, Unexpected, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::operand::sealed::{Class, Sealed};
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
    ($decimal:ty, $width:ty) => {
        impl Serialize for $decimal {
            /// Writes the `Display` output as one string.
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                match Sealed::class(*self) {
                    Class::Finite(v) => {
                        // Reject wide coefficients before creating lane buffers.
                        if let Ok(coeff) = u64::try_from(v.coeff)
                            && let Some(value) = lane::SmallValue::new(v.neg, coeff, v.exp)
                        {
                            let mut small: lane::SmallText;
                            let mut unsigned: fmt::NumBuffer<u64>;
                            let mut signed: fmt::NumBuffer<i64>;
                            let text = match value {
                                lane::SmallValue::Unsigned(integer) => {
                                    unsigned = fmt::NumBuffer::new();
                                    integer.format_into(&mut unsigned)
                                }
                                lane::SmallValue::Signed(integer) => {
                                    signed = fmt::NumBuffer::new();
                                    integer.format_into(&mut signed)
                                }
                                lane::SmallValue::NegativeZero => "-0",
                                lane::SmallValue::Fraction { neg, coeff, exp } => {
                                    small = lane::SmallText::new();
                                    small.text(neg, coeff, exp).map_err(|_| {
                                        <S::Error as serde::ser::Error>::custom(
                                            "decimal text does not fit its buffer",
                                        )
                                    })?
                                }
                            };
                            return serializer.serialize_str(text);
                        }
                        let mut digits = crate::display::Digits::new();
                        let parts = digits.parts(v, false);
                        if let Some(text) = parts.digits_only() {
                            return serializer.serialize_str(text);
                        }
                        if parts.text_len() > crate::display::TEXT_LEN {
                            return serializer.collect_str(&parts);
                        }
                        let mut bytes = [0; crate::display::TEXT_LEN];
                        let text = parts.compose(&mut bytes).map_err(|_| {
                            <S::Error as serde::ser::Error>::custom(
                                "decimal text does not fit its buffer",
                            )
                        })?;
                        serializer.serialize_str(text)
                    }
                    Class::Nan { .. } => serializer.serialize_str("NaN"),
                    Class::Infinite(neg) => {
                        serializer.serialize_str(if neg { "-inf" } else { "inf" })
                    }
                }
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
                            .map(<$decimal>::from_in_range)
                            .map_err(|error| {
                                E::invalid_value(
                                    Unexpected::Signed(value),
                                    &FailedValue(stringify!($decimal), error),
                                )
                            })
                    }

                    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
                        integer::<$width>(false, value as u128)
                            .map(<$decimal>::from_in_range)
                            .map_err(|error| {
                                E::invalid_value(
                                    Unexpected::Unsigned(value),
                                    &FailedValue(stringify!($decimal), error),
                                )
                            })
                    }

                    fn visit_i128<E: de::Error>(self, value: i128) -> Result<Self::Value, E> {
                        integer::<$width>(value < 0, value.unsigned_abs())
                            .map(<$decimal>::from_in_range)
                            .map_err(|error| {
                                E::invalid_value(
                                    Unexpected::Other("i128 integer"),
                                    &FailedValue(stringify!($decimal), error),
                                )
                            })
                    }

                    fn visit_u128<E: de::Error>(self, value: u128) -> Result<Self::Value, E> {
                        integer::<$width>(false, value)
                            .map(<$decimal>::from_in_range)
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

decimal_serde!(Decimal32, crate::layout::W32);
decimal_serde!(Decimal64, crate::layout::W64);
decimal_serde!(Decimal128, crate::layout::W128);

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
