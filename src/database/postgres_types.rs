//! rust-postgres and tokio-postgres bindings share this binary NUMERIC codec.
use super::pg_numeric::{self, PgNumeric};
use crate::{Decimal32, Decimal64, Decimal128};
use bytes::BytesMut;
use postgres_types_0_2::{FromSql, IsNull, ToSql, Type, WrongType};
use std::boxed::Box;
type BoxError = Box<dyn std::error::Error + Sync + Send>;

macro_rules! impl_postgres {
    ($decimal:ty) => {
        impl ToSql for $decimal {
            fn to_sql(&self, ty: &Type, out: &mut BytesMut) -> Result<IsNull, BoxError> {
                if !<Self as ToSql>::accepts(ty) {
                    return Err(Box::new(WrongType::new::<Self>(ty.clone())));
                }
                let plan = PgNumeric::new(*self)?;
                out.reserve(plan.len());
                let result: Result<(), core::convert::Infallible> = plan.write(|part| {
                    out.extend_from_slice(part);
                    Ok(())
                });
                match result {
                    Ok(()) => {}
                    Err(never) => match never {},
                }
                Ok(IsNull::No)
            }
            fn accepts(ty: &Type) -> bool {
                *ty == Type::NUMERIC
            }
            postgres_types_0_2::to_sql_checked!();
        }
        impl<'a> FromSql<'a> for $decimal {
            fn from_sql(ty: &Type, bytes: &'a [u8]) -> Result<Self, BoxError> {
                if !<Self as FromSql>::accepts(ty) {
                    return Err(Box::new(WrongType::new::<Self>(ty.clone())));
                }
                Ok(pg_numeric::decode(bytes)?)
            }
            fn accepts(ty: &Type) -> bool {
                *ty == Type::NUMERIC
            }
        }
    };
}
impl_postgres!(Decimal32);
impl_postgres!(Decimal64);
impl_postgres!(Decimal128);

#[cfg(test)]
mod tests {
    use super::*;
    use postgres_types_0_2::Format;
    const ONE_FIFTY: &[u8] = &[0, 2, 0, 0, 0, 0, 0, 2, 0, 1, 0x13, 0x88];

    #[test]
    fn postgres_types_exact_scalar_roundtrip_and_null_all_widths() {
        macro_rules! check {
            ($d:ty) => {{
                let decimal = <$d>::from_sql(&Type::NUMERIC, ONE_FIFTY).unwrap();
                assert_eq!(decimal.to_coefficient_and_exponent().unwrap(), (150, -2));
                let mut out = BytesMut::from(&[0xaa, 0xbb][..]);
                assert!(matches!(
                    decimal.to_sql_checked(&Type::NUMERIC, &mut out).unwrap(),
                    IsNull::No
                ));
                assert_eq!(&out[2..], ONE_FIFTY);
                assert!(matches!(
                    decimal.encode_format(&Type::NUMERIC),
                    Format::Binary
                ));
                let before = out.clone();
                assert!(matches!(
                    None::<$d>.to_sql_checked(&Type::NUMERIC, &mut out).unwrap(),
                    IsNull::Yes
                ));
                assert_eq!(out, before);
                assert_eq!(<Option<$d>>::from_sql_null(&Type::NUMERIC).unwrap(), None);
                assert!(<$d>::from_sql_null(&Type::NUMERIC).is_err());
                assert!(<$d as FromSql>::accepts(&Type::NUMERIC));
                assert!(<$d as ToSql>::accepts(&Type::NUMERIC));
                for ty in [Type::TEXT, Type::FLOAT8, Type::INT8, Type::NUMERIC_ARRAY] {
                    assert!(!<$d as FromSql>::accepts(&ty));
                    assert!(!<$d as ToSql>::accepts(&ty));
                    assert!(<$d>::from_sql(&ty, ONE_FIFTY).is_err());
                    assert!(decimal.to_sql_checked(&ty, &mut out).is_err());
                    assert!(decimal.to_sql(&ty, &mut out).is_err());
                    assert_eq!(out, before);
                }
            }};
        }
        check!(Decimal32);
        check!(Decimal64);
        check!(Decimal128);
    }

    #[test]
    fn postgres_types_special_mapping_and_write_error_rollback_all_widths() {
        macro_rules! check {
            ($d:ty) => {{
                for (decimal, sign) in [
                    (<$d>::NAN, 0xc0),
                    (<$d>::INFINITY, 0xd0),
                    (<$d>::NEG_INFINITY, 0xf0),
                ] {
                    let expected = [0, 0, 0, 0, sign, 0, 0, 0];
                    let mut out = BytesMut::new();
                    assert!(matches!(
                        decimal.to_sql(&Type::NUMERIC, &mut out).unwrap(),
                        IsNull::No
                    ));
                    assert_eq!(&out[..], &expected);
                    let decoded = <$d>::from_sql(&Type::NUMERIC, &expected).unwrap();
                    assert_eq!(decoded.to_bits(), decimal.to_bits());
                }
                let payload_nan = <$d>::from_bits(<$d>::NAN.to_bits() | 1).unwrap();
                let mut out = BytesMut::from(&[0xaa, 0xbb][..]);
                let signaling_nan = <$d>::from_bits(
                    <$d>::NAN.to_bits() | (1 << (core::mem::size_of::<$d>() * 8 - 7)),
                )
                .unwrap();
                for decimal in [-<$d>::NAN, payload_nan, signaling_nan] {
                    assert!(decimal.to_sql(&Type::NUMERIC, &mut out).is_err());
                    assert_eq!(&out[..], &[0xaa, 0xbb]);
                }
            }};
        }
        check!(Decimal32);
        check!(Decimal64);
        check!(Decimal128);
    }

    #[test]
    fn postgres_types_malformed_input_returns_errors_without_panics() {
        let cases: &[&[u8]] = &[
            &[],
            &[0, 0, 0, 0, 0, 0, 0],                // incomplete header
            &[0, 1, 0, 0, 0, 0, 0, 0],             // missing digit
            &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0],       // extra digit
            &[0, 1, 0, 0, 0, 0, 0, 0, 0x27, 0x10], // digit 10000
            &[0, 0, 0, 0, 0, 0, 0x40, 0],          // dscale 16384
            &[0, 1, 0xff, 0xff, 0, 0, 0, 0, 0, 1], // hidden fraction
            &[0, 0, 0, 0, 0x80, 0, 0, 0],          // invalid sign
        ];
        for raw in cases {
            assert!(Decimal32::from_sql(&Type::NUMERIC, raw).is_err());
            assert!(Decimal64::from_sql(&Type::NUMERIC, raw).is_err());
            assert!(Decimal128::from_sql(&Type::NUMERIC, raw).is_err());
        }
    }
}
