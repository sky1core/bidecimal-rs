//! SQLx's public MySQL API uses string-tagged parameters for exact decimals.
use super::{MysqlText, decode_decimal_str};
use crate::{Decimal32, Decimal64, Decimal128, Error};
use sqlx_0_9::encode::IsNull;
use sqlx_0_9::error::BoxDynError;
use sqlx_0_9::mysql::{MySqlTypeInfo, MySqlValueRef};
use sqlx_0_9::{Decode, Encode, MySql, Type, TypeInfo, ValueRef};
use std::vec::Vec;

macro_rules! impl_mysql {
    ($decimal:ty) => {
        impl Type<MySql> for $decimal {
            fn type_info() -> MySqlTypeInfo {
                <str as Type<MySql>>::type_info()
            }
            fn compatible(ty: &MySqlTypeInfo) -> bool {
                ty.name() == "DECIMAL"
            }
        }
        impl Encode<'_, MySql> for $decimal {
            fn encode_by_ref(&self, out: &mut Vec<u8>) -> Result<IsNull, BoxDynError> {
                let text = MysqlText::new(*self)?;
                // MySQL's validated DECIMAL limit is 65 digits plus punctuation,
                // strictly below the 251-byte boundary of a one-byte lenenc.
                // Complete every fallible check before touching the buffer.
                let len = u8::try_from(text.len()).map_err(|_| Error::InvalidInput)?;
                if len >= 251 {
                    return Err(Error::InvalidInput.into());
                }
                let original_len = out.len();
                out.reserve(1 + usize::from(len));
                out.push(len);
                let result = text.write(|bytes| {
                    out.extend_from_slice(bytes);
                    Ok::<_, Error>(())
                });
                if let Err(error) = result {
                    out.truncate(original_len);
                    return Err(Error::from(error).into());
                }
                Ok(IsNull::No)
            }
            fn size_hint(&self) -> usize {
                MysqlText::new(*self).map_or(0, |text| 1 + text.len())
            }
        }
        impl<'r> Decode<'r, MySql> for $decimal {
            #[inline(always)]
            fn decode(value: MySqlValueRef<'r>) -> Result<Self, BoxDynError> {
                if !<Self as Type<MySql>>::compatible(&value.type_info()) {
                    return Err(Error::InvalidInput.into());
                }
                let text = match <&str as Decode<MySql>>::decode(value) {
                    Ok(text) => text,
                    // Preserve the codec error used by decode_text for invalid UTF-8.
                    // NULL and any other driver errors retain their original identity.
                    Err(error) if error.is::<core::str::Utf8Error>() => {
                        return Err(Error::InvalidInput.into());
                    }
                    Err(error) => return Err(error),
                };
                Ok(decode_decimal_str(text)?)
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
    use sqlx_0_9::Arguments;
    use sqlx_0_9::mysql::MySqlArguments;
    use std::vec;

    #[test]
    fn sqlx_mysql_direct_encoding_and_failure_rollback_all_widths() {
        macro_rules! check {
            ($d:ty) => {{
                let mut bytes = vec![0xaa, 0xbb];
                let value = <$d>::parse("-1.500").unwrap();
                assert_eq!(<$d as Encode<MySql>>::size_hint(&value), 7);
                assert!(matches!(
                    <$d as Encode<MySql>>::encode_by_ref(&value, &mut bytes).unwrap(),
                    IsNull::No
                ));
                assert_eq!(&bytes, &[0xaa, 0xbb, 6, b'-', b'1', b'.', b'5', b'0', b'0']);
                let before = bytes.clone();
                for value in [
                    <$d>::NAN,
                    <$d>::INFINITY,
                    <$d>::NEG_INFINITY,
                    <$d>::from_coefficient_and_exponent(1, -31).unwrap(),
                    <$d>::from_coefficient_and_exponent(1, 65).unwrap(),
                ] {
                    assert!(<$d as Encode<MySql>>::encode_by_ref(&value, &mut bytes).is_err());
                    assert_eq!(bytes, before);
                }
                let mut args = MySqlArguments::default();
                args.add(value).unwrap();
                assert_eq!(args.len(), 1);
                assert!(args.add(<$d>::NAN).is_err());
                assert_eq!(args.len(), 1);
                args.add(<$d>::ZERO).unwrap();
                assert_eq!(args.len(), 2);
                assert!(!<$d as Type<MySql>>::compatible(
                    &<str as Type<MySql>>::type_info()
                ));
                assert!(!<$d as Type<MySql>>::compatible(
                    &<f64 as Type<MySql>>::type_info()
                ));
            }};
        }
        check!(Decimal32);
        check!(Decimal64);
        check!(Decimal128);
    }

    #[test]
    fn sqlx_mysql_normalizes_actual_payload_when_scale_exceeds_limit() {
        let value = Decimal128::from_coefficient_and_exponent(10, -31).unwrap();
        let mut bytes = Vec::new();
        assert!(matches!(
            <Decimal128 as Encode<MySql>>::encode_by_ref(&value, &mut bytes).unwrap(),
            IsNull::No
        ));
        assert_eq!(bytes[0], 32);
        assert_eq!(&bytes[1..], b"0.000000000000000000000000000001");
    }
}
