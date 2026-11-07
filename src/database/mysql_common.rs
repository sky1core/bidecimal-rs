//! Fallible mysql_common conversions with ownership-preserving rollback.
use super::{DbDecimal, MysqlText, decode_text};
use crate::{Decimal32, Decimal64, Decimal128, Error};
use mysql_common_0_37::value::{
    Value,
    convert::{FromValue, FromValueError},
};
use std::vec::Vec;

/// A parsed decimal and its original MySQL value, retained for row rollback.
///
/// Successful conversion consumes the original value. Conversion back to
/// `Value` returns the exact original bytes and allocation without formatting.
#[derive(Debug)]
pub struct MysqlIntermediate<D> {
    decimal: D,
    original: Value,
}

fn decode_integer<D: DbDecimal>(negative: bool, mut magnitude: u64) -> Result<D, Error> {
    // A u64 needs at most 20 digits, plus one sign. No temporary allocation.
    let mut bytes = [0_u8; 21];
    let mut start = bytes.len();
    loop {
        start -= 1;
        bytes[start] = b'0' + (magnitude % 10) as u8;
        magnitude /= 10;
        if magnitude == 0 {
            break;
        }
    }
    if negative {
        start -= 1;
        bytes[start] = b'-';
    }
    decode_text(&bytes[start..], false)
}

impl<D: DbDecimal> TryFrom<Value> for MysqlIntermediate<D> {
    type Error = FromValueError;
    #[inline]
    fn try_from(original: Value) -> Result<Self, Self::Error> {
        let parsed = match &original {
            Value::Bytes(bytes) => decode_text(bytes, false),
            Value::Int(value) => decode_integer(*value < 0, value.unsigned_abs()),
            Value::UInt(value) => decode_integer(false, *value),
            _ => Err(Error::InvalidInput),
        };
        match parsed {
            Ok(decimal) => Ok(Self { decimal, original }),
            Err(_) => Err(FromValueError(original)),
        }
    }
}
impl<D> From<MysqlIntermediate<D>> for Value {
    #[inline]
    fn from(value: MysqlIntermediate<D>) -> Self {
        value.original
    }
}

macro_rules! impl_mysql {
    ($decimal:ty) => {
        impl FromValue for $decimal {
            type Intermediate = MysqlIntermediate<Self>;
        }
        impl From<MysqlIntermediate<$decimal>> for $decimal {
            #[inline]
            fn from(value: MysqlIntermediate<$decimal>) -> Self {
                value.decimal
            }
        }
        impl TryFrom<$decimal> for Value {
            type Error = Error;
            #[inline]
            fn try_from(value: $decimal) -> Result<Self, Self::Error> {
                let text = MysqlText::new(value)?;
                // Value::Bytes necessarily owns this buffer. Capacity is exact;
                // the codec itself does not create another allocation.
                let mut bytes = Vec::with_capacity(text.len());
                let result = text.write(|part| {
                    bytes.extend_from_slice(part);
                    Ok::<_, Error>(())
                });
                result.map_err(Error::from)?;
                Ok(Value::Bytes(bytes))
            }
        }
    };
}
impl_mysql!(Decimal32);
impl_mysql!(Decimal64);
impl_mysql!(Decimal128);

#[cfg(test)]
mod tests {
    use super::*;
    use mysql_common_0_37::{
        constants::ColumnType,
        packets::Column,
        row::{convert::from_row_opt, new_row},
    };
    use std::{sync::Arc, vec};

    #[test]
    fn mysql_common_commit_rollback_and_errors_all_widths() {
        macro_rules! check {
            ($d:ty) => {{
                let bytes = b"+001.500".to_vec();
                let pointer = bytes.as_ptr();
                let capacity = bytes.capacity();
                let intermediate = <$d>::get_intermediate(Value::Bytes(bytes)).unwrap();
                let Value::Bytes(rolled_back) = Value::from(intermediate) else {
                    panic!("bytes")
                };
                assert_eq!(rolled_back, b"+001.500");
                assert_eq!(rolled_back.as_ptr(), pointer);
                assert_eq!(rolled_back.capacity(), capacity);
                let decimal = <$d>::from_value_opt(Value::Bytes(rolled_back)).unwrap();
                assert_eq!(<Option<$d>>::from_value_opt(Value::NULL).unwrap(), None);
                assert_eq!(
                    <Option<$d>>::from_value_opt(Value::Bytes(b"0.00".to_vec()))
                        .unwrap()
                        .unwrap()
                        .to_coefficient_and_exponent()
                        .unwrap(),
                    (0, -2)
                );
                assert_eq!(decimal.to_coefficient_and_exponent().unwrap(), (1500, -3));
                assert_eq!(
                    Value::try_from(decimal).unwrap(),
                    Value::Bytes(b"1.500".to_vec())
                );
                for original in [
                    Value::NULL,
                    Value::Float(1.5),
                    Value::Double(1.5),
                    Value::Bytes(b"NaN".to_vec()),
                    Value::Bytes(b"Infinity".to_vec()),
                    Value::Bytes(b"1.500x".to_vec()),
                ] {
                    let expected = original.clone();
                    assert_eq!(<$d>::from_value_opt(original).unwrap_err().0, expected);
                }
                for value in [
                    <$d>::NAN,
                    <$d>::INFINITY,
                    <$d>::NEG_INFINITY,
                    <$d>::from_coefficient_and_exponent(1, -31).unwrap(),
                    <$d>::from_coefficient_and_exponent(1, 65).unwrap(),
                ] {
                    assert!(Value::try_from(value).is_err());
                }
                assert_eq!(
                    <$d>::from_value_opt(Value::Int(-123)).unwrap(),
                    <$d>::parse("-123").unwrap()
                );
                assert_eq!(
                    <$d>::from_value_opt(Value::UInt(123)).unwrap(),
                    <$d>::parse("123").unwrap()
                );
            }};
        }
        check!(Decimal32);
        check!(Decimal64);
        check!(Decimal128);
    }

    #[test]
    fn mysql_common_outgoing_normalized_cohort_is_the_validated_cohort() {
        let value = Decimal128::from_coefficient_and_exponent(10, -31).unwrap();
        assert_eq!(
            Value::try_from(value).unwrap(),
            Value::Bytes(b"0.000000000000000000000000000001".to_vec())
        );
    }

    #[test]
    fn mysql_common_integer_boundaries_are_exact_not_truncated() {
        assert!(Decimal32::from_value_opt(Value::Int(i64::MIN)).is_err());
        assert!(Decimal64::from_value_opt(Value::UInt(u64::MAX)).is_err());
        assert_eq!(
            Decimal128::from_value_opt(Value::Int(i64::MIN))
                .unwrap()
                .to_coefficient_and_exponent()
                .unwrap(),
            (i128::from(i64::MIN), 0)
        );
        assert_eq!(
            Decimal128::from_value_opt(Value::UInt(u64::MAX))
                .unwrap()
                .to_coefficient_and_exponent()
                .unwrap(),
            (i128::from(u64::MAX), 0)
        );
        assert_eq!(
            Decimal32::from_value_opt(Value::UInt(10_000_000))
                .unwrap()
                .to_coefficient_and_exponent()
                .unwrap(),
            (1_000_000, 1)
        );
    }

    #[test]
    fn mysql_common_later_tuple_failure_restores_original_owned_bytes() {
        let first = b"+0001.5000".to_vec();
        let pointer = first.as_ptr();
        let row = new_row(
            vec![
                Value::Bytes(first),
                Value::Bytes(b"not-an-integer".to_vec()),
            ],
            Arc::from(vec![
                Column::new(ColumnType::MYSQL_TYPE_NEWDECIMAL),
                Column::new(ColumnType::MYSQL_TYPE_VAR_STRING),
            ]),
        );
        let error = from_row_opt::<(Decimal32, u32)>(row).unwrap_err();
        let Some(Value::Bytes(bytes)) = error.0.as_ref(0) else {
            panic!("original bytes")
        };
        assert_eq!(bytes, b"+0001.5000");
        assert_eq!(bytes.as_ptr(), pointer);
        assert_eq!(
            error.0.as_ref(1),
            Some(&Value::Bytes(b"not-an-integer".to_vec()))
        );
    }
}
