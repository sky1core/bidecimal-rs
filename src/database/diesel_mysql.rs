//! Backend-only Diesel adapters, without a native MySQL client dependency.
use super::{DbDecimal, MysqlText, decode_text};
use crate::{Decimal32, Decimal64, Decimal128, Error};
use diesel_2::deserialize::FromSql;
use diesel_2::mysql::{Mysql, MysqlType, MysqlValue};
use diesel_2::serialize::{IsNull, Output, ToSql};
use diesel_2::sql_types::Numeric;
use diesel_2::{deserialize, serialize};
use std::io::Write;

// Keep the raw-byte decode boundary testable without Diesel's unstable
// third-party-backend constructor or fabricating MysqlValue internals.
fn decode_numeric<D: DbDecimal>(bytes: &[u8], tpe: MysqlType) -> deserialize::Result<D> {
    if tpe != MysqlType::Numeric {
        return Err(Error::InvalidInput.into());
    }
    Ok(decode_text(bytes, false)?)
}

macro_rules! impl_mysql {
    ($decimal:ty) => {
        impl ToSql<Numeric, Mysql> for $decimal {
            fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Mysql>) -> serialize::Result {
                // Validate before writing. The collector owns its parameter Vec;
                // no intermediate String or Vec is made by this adapter.
                let text = MysqlText::<false>::new_diesel(*self)?;
                text.write_contiguous(|bytes| out.write_all(bytes))?;
                Ok(IsNull::No)
            }
        }
        impl FromSql<Numeric, Mysql> for $decimal {
            fn from_sql(value: MysqlValue<'_>) -> deserialize::Result<Self> {
                decode_numeric(value.as_bytes(), value.value_type())
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
    use diesel_2::deserialize::FromSqlRow;
    use diesel_2::expression::AsExpression;

    #[test]
    fn diesel_mysql_empty_numeric_is_invalid_input_for_all_widths() {
        macro_rules! check {
            ($d:ty) => {{
                let error = decode_numeric::<$d>(b"", MysqlType::Numeric).unwrap_err();
                assert!(matches!(
                    error.downcast_ref::<Error>(),
                    Some(Error::InvalidInput)
                ));
            }};
        }
        check!(Decimal32);
        check!(Decimal64);
        check!(Decimal128);
    }

    #[test]
    fn diesel_mysql_collects_numeric_and_rejects_without_partial_bind() {
        use diesel_2::query_builder::bind_collector::{BindCollector, RawBytesBindCollector};
        macro_rules! check {
            ($d:ty) => {{
                let mut collector = RawBytesBindCollector::<Mysql>::new();
                let decimal = <$d>::parse("1.50").unwrap();
                collector
                    .push_bound_value::<Numeric, _>(&decimal, &mut ())
                    .unwrap();
                // The collector's public Debug view is the only backend-only
                // public observation of its private payload buffer.
                let before = std::format!("{collector:?}");
                assert!(before.contains("Numeric"));
                assert!(before.contains("[49, 46, 53, 48]"));
                assert!(
                    collector
                        .push_bound_value::<Numeric, _>(&<$d>::NAN, &mut ())
                        .is_err()
                );
                assert_eq!(std::format!("{collector:?}"), before);
                assert!(
                    collector
                        .push_bound_value::<Numeric, _>(&<$d>::parse("1e-31").unwrap(), &mut ())
                        .is_err()
                );
                assert_eq!(std::format!("{collector:?}"), before);
            }};
        }
        check!(Decimal32);
        check!(Decimal64);
        check!(Decimal128);
    }

    #[test]
    fn diesel_mysql_supports_owned_borrowed_expressions_and_rows() {
        fn expression<T: AsExpression<Numeric>>() {}
        fn row<T: FromSqlRow<Numeric, Mysql>>() {}
        expression::<Decimal32>();
        expression::<&Decimal32>();
        row::<Decimal32>();
        expression::<Decimal64>();
        expression::<&Decimal64>();
        row::<Decimal64>();
        expression::<Decimal128>();
        expression::<&Decimal128>();
        row::<Decimal128>();
    }
    // The reference keeps the pre-candidate chunked sink, while all conversion
    // and validation stay in the shared codec. Compare complete public collector
    // state, including the sentinel bind, payload, and metadata.
    #[derive(Debug)]
    struct Chunked<D>(D);
    impl<D: DbDecimal + core::fmt::Debug> ToSql<Numeric, Mysql> for Chunked<D> {
        fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Mysql>) -> serialize::Result {
            MysqlText::new(self.0)?.write(|part| out.write_all(part))?;
            Ok(IsNull::No)
        }
    }

    fn assert_same_collector<D>(value: D)
    where
        D: DbDecimal + core::fmt::Debug + ToSql<Numeric, Mysql>,
    {
        use diesel_2::query_builder::bind_collector::{BindCollector, RawBytesBindCollector};
        let sentinel = Decimal32::parse("1.50").unwrap();
        let mut actual = RawBytesBindCollector::<Mysql>::new();
        let mut expected = RawBytesBindCollector::<Mysql>::new();
        actual
            .push_bound_value::<Numeric, _>(&sentinel, &mut ())
            .unwrap();
        expected
            .push_bound_value::<Numeric, _>(&sentinel, &mut ())
            .unwrap();
        let before = std::format!("{actual:?}");
        let reference = Chunked(value);
        let result = actual.push_bound_value::<Numeric, _>(&value, &mut ());
        let old_result = expected.push_bound_value::<Numeric, _>(&reference, &mut ());
        assert_eq!(result.is_ok(), old_result.is_ok());
        if let (Err(actual_error), Err(expected_error)) = (result, old_result) {
            match (actual_error, expected_error) {
                (
                    diesel_2::result::Error::SerializationError(actual_error),
                    diesel_2::result::Error::SerializationError(expected_error),
                ) => assert_eq!(
                    actual_error.downcast_ref::<Error>().unwrap(),
                    expected_error.downcast_ref::<Error>().unwrap(),
                ),
                errors => panic!("unexpected error pair: {errors:?}"),
            }
            assert_eq!(std::format!("{actual:?}"), before);
        }
        assert_eq!(std::format!("{actual:?}"), std::format!("{expected:?}"));
    }

    #[test]
    fn diesel_mysql_single_write_matches_chunked_payload_and_errors_all_widths() {
        macro_rules! check {
            ($d:ty, $i:ty, $precision:expr, $long:literal) => {{
                for input in ["1.50", $long, "100e-32", "-0.000", "-1e64", "-1e-30"] {
                    assert_same_collector(<$d>::parse(input).unwrap());
                }
                for coefficient in [0_u128, 1, 150, 10_u128.pow($precision) - 1] {
                    for exponent in [-31, -30, -2, -1, 0, 1, 65 - $precision as i32, 64, 65] {
                        for negative in [false, true] {
                            let signed = if negative {
                                -(coefficient as $i)
                            } else {
                                coefficient as $i
                            };
                            let value =
                                <$d>::from_coefficient_and_exponent(signed, exponent).unwrap();
                            let value = if coefficient == 0 && negative {
                                -value
                            } else {
                                value
                            };
                            assert_same_collector(value);
                        }
                    }
                }
                for value in [<$d>::NAN, -<$d>::NAN, <$d>::INFINITY, <$d>::NEG_INFINITY] {
                    assert_same_collector(value);
                }
                assert_same_collector(<$d>::from_bits(<$d>::NAN.to_bits() | 1).unwrap());
                assert_same_collector(
                    <$d>::from_bits(<$d>::NAN.to_bits() | (1 << (<$i>::BITS - 7))).unwrap(),
                );
            }};
        }
        check!(Decimal32, i32, 7, "-1234.567");
        check!(Decimal64, i64, 16, "-12345678.90123456");
        check!(Decimal128, i128, 34, "-12345678901234567.89012345678901234");
    }
}

#[cfg(test)]
#[test]
fn diesel_fast_fit_differential() {
    macro_rules! check {
        ($ty:ty) => {{
            let coefficients = [
                "0",
                "1",
                "9",
                "10",
                "9999999",
                "10000000",
                "9999999999999999",
                "10000000000000000",
                "9999999999999999999999999999999999",
            ];
            let exponents = [
                -100, -65, -32, -31, -30, -29, -1, 0, 1, 30, 31, 32, 65, 66, 100,
            ];
            let mut count = 0;
            for coeff in coefficients {
                for exp in exponents {
                    for sign in ["", "-"] {
                        let text = std::format!("{sign}{coeff}e{exp}");
                        let Ok(value) = <$ty>::parse(&text) else {
                            continue;
                        };
                        let a = MysqlText::<true>::new(value);
                        let b = MysqlText::<false>::new_diesel(value);
                        assert_eq!(
                            a.as_ref().err().map(|e| std::format!("{e:?}")),
                            b.as_ref().err().map(|e| std::format!("{e:?}")),
                            "{text}"
                        );
                        if let (Ok(a), Ok(b)) = (a, b) {
                            let mut x = std::vec::Vec::new();
                            let mut y = std::vec::Vec::new();
                            a.write_contiguous(|bytes| {
                                x.extend_from_slice(bytes);
                                Ok::<_, ()>(())
                            })
                            .unwrap();
                            b.write_contiguous(|bytes| {
                                y.extend_from_slice(bytes);
                                Ok::<_, ()>(())
                            })
                            .unwrap();
                            assert_eq!(x, y, "{text}");
                        }
                        count += 1;
                    }
                }
            }
            assert!(count > 50);
        }};
    }
    check!(Decimal32);
    check!(Decimal64);
    check!(Decimal128);
}
