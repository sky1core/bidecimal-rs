//! SQLx NUMERIC binary bindings using only its public argument buffer API.
use super::{
    decode_text,
    pg_numeric::{self, PgNumeric},
};
use crate::{Decimal32, Decimal64, Decimal128, Error};
use sqlx_0_9::encode::IsNull;
use sqlx_0_9::error::BoxDynError;
use sqlx_0_9::postgres::{PgArgumentBuffer, PgTypeInfo, PgValueFormat, PgValueRef};
use sqlx_0_9::{Decode, Encode, Postgres, Type, ValueRef};

macro_rules! impl_postgres {
    ($decimal:ty, $maximum_payload:expr) => {
        impl Type<Postgres> for $decimal {
            fn type_info() -> PgTypeInfo {
                PgTypeInfo::with_name("numeric")
            }
            fn compatible(ty: &PgTypeInfo) -> bool {
                // PgTypeInfo's loose equality can equate an unknown OID to
                // any declared name. Inspect known OIDs, and use strict name
                // equality only for unresolved named metadata.
                match ty.oid() {
                    Some(oid) => oid.0 == 1700,
                    None => ty.type_eq(&<Self as Type<Postgres>>::type_info()),
                }
            }
        }
        impl Encode<'_, Postgres> for $decimal {
            fn encode_by_ref(&self, out: &mut PgArgumentBuffer) -> Result<IsNull, BoxDynError> {
                let plan = PgNumeric::new(*self)?;
                out.reserve(plan.len());
                // SQLx adds the outer 4-byte parameter length. Write only the
                // NUMERIC header and base-10000 groups here.
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
            fn size_hint(&self) -> usize {
                // Advisory upper bound, not an exact wire length. These validated
                // decimal widths have scale <=6176 and high weight <=1536,
                // so every finite value passes NUMERIC range checks.
                // Preserve canonical-special and invalid-special hint behavior.
                match super::DbDecimal::pg_special(*self) {
                    Ok(Some(_)) => 8,
                    Err(_) => 0,
                    Ok(None) => $maximum_payload,
                }
            }
        }
        impl<'r> Decode<'r, Postgres> for $decimal {
            fn decode(value: PgValueRef<'r>) -> Result<Self, BoxDynError> {
                if !<Self as Type<Postgres>>::compatible(&value.type_info()) {
                    return Err(Error::InvalidInput.into());
                }
                let bytes = value.as_bytes()?;
                Ok(match value.format() {
                    PgValueFormat::Binary => pg_numeric::decode(bytes)?,
                    PgValueFormat::Text => decode_text(bytes, true)?,
                })
            }
        }
    };
}
impl_postgres!(Decimal32, 14);
impl_postgres!(Decimal64, 18);
impl_postgres!(Decimal128, 28);

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx_0_9::{Arguments, postgres::PgArguments};

    #[test]
    fn sqlx_postgres_exact_header_and_no_outer_length_all_widths() {
        macro_rules! check {
            ($d:ty) => {{
                let decimal = <$d>::parse("1.50").unwrap();
                let mut out = PgArgumentBuffer::default();
                out.extend_from_slice(&[0xaa, 0xbb]);
                // Candidate changes advisory hint only; the exact 12-byte payload
                // assertion below remains unchanged.
                let maximum = match core::mem::size_of::<$d>() {
                    4 => 14,
                    8 => 18,
                    16 => 28,
                    _ => unreachable!(),
                };
                assert_eq!(<$d as Encode<Postgres>>::size_hint(&decimal), maximum);
                assert!(matches!(
                    <$d as Encode<Postgres>>::encode_by_ref(&decimal, &mut out).unwrap(),
                    IsNull::No
                ));
                assert_eq!(
                    &out[..],
                    &[0xaa, 0xbb, 0, 2, 0, 0, 0, 0, 0, 2, 0, 1, 0x13, 0x88]
                );
                assert!(<$d as Type<Postgres>>::compatible(&PgTypeInfo::with_oid(
                    sqlx_0_9::postgres::types::Oid(1700)
                )));
                assert!(!<$d as Type<Postgres>>::compatible(&<f64 as Type<
                    Postgres,
                >>::type_info(
                )));
                assert!(!<$d as Type<Postgres>>::compatible(&<str as Type<
                    Postgres,
                >>::type_info(
                )));
            }};
        }
        check!(Decimal32);
        check!(Decimal64);
        check!(Decimal128);
    }

    #[test]
    fn sqlx_postgres_specials_and_error_rollback_all_widths() {
        macro_rules! check {
            ($d:ty) => {{
                for (decimal, sign) in [
                    (<$d>::NAN, 0xc0),
                    (<$d>::INFINITY, 0xd0),
                    (<$d>::NEG_INFINITY, 0xf0),
                ] {
                    let mut out = PgArgumentBuffer::default();
                    assert!(matches!(
                        <$d as Encode<Postgres>>::encode_by_ref(&decimal, &mut out).unwrap(),
                        IsNull::No
                    ));
                    assert_eq!(&out[..], &[0, 0, 0, 0, sign, 0, 0, 0]);
                }
                let mut out = PgArgumentBuffer::default();
                out.extend_from_slice(&[0xaa, 0xbb]);
                let payload_nan = <$d>::from_bits(<$d>::NAN.to_bits() | 1).unwrap();
                let signaling_nan = <$d>::from_bits(
                    <$d>::NAN.to_bits() | (1 << (core::mem::size_of::<$d>() * 8 - 7)),
                )
                .unwrap();
                for decimal in [-<$d>::NAN, payload_nan, signaling_nan] {
                    assert!(<$d as Encode<Postgres>>::encode_by_ref(&decimal, &mut out).is_err());
                    assert_eq!(&out[..], &[0xaa, 0xbb]);
                }
                let mut args = PgArguments::default();
                args.add(<$d>::ZERO).unwrap();
                assert_eq!(args.len(), 1);
                assert!(args.add(-<$d>::NAN).is_err());
                assert_eq!(args.len(), 1);
                args.add(None::<$d>).unwrap();
                assert_eq!(args.len(), 2);
            }};
        }
        check!(Decimal32);
        check!(Decimal64);
        check!(Decimal128);
    }
}

#[cfg(test)]
mod hint_candidate_tests {
    use super::*;
    macro_rules! width {
        ($test:ident,$d:ty,$digits:expr,$emin:expr,$emax:expr,$bound:expr) => {
            #[test]
            fn $test() {
                fn check(value: $d) {
                    let reference = PgNumeric::new(value);
                    let hint = <$d as Encode<Postgres>>::size_hint(&value);
                    match reference {
                        Ok(plan) => {
                            assert!(hint >= plan.len());
                            assert!(hint <= $bound);
                            if !value.is_finite() {
                                assert_eq!(hint, 8);
                            } else {
                                assert_eq!(hint, $bound);
                            }
                        }
                        Err(_) => assert_eq!(hint, 0),
                    }
                }
                for v in [
                    <$d>::ZERO,
                    -<$d>::ZERO,
                    <$d>::NAN,
                    -<$d>::NAN,
                    <$d>::INFINITY,
                    <$d>::NEG_INFINITY,
                ] {
                    check(v);
                }
                check(<$d>::from_bits(<$d>::NAN.to_bits() | 1).unwrap());
                check(
                    <$d>::from_bits(
                        <$d>::NAN.to_bits() | (1 << (core::mem::size_of::<$d>() * 8 - 7)),
                    )
                    .unwrap(),
                );
                let maximum = 10_u128.pow($digits) - 1;
                for exp in [
                    $emin,
                    $emin + 1,
                    -9,
                    -8,
                    -7,
                    -6,
                    -5,
                    -4,
                    -3,
                    -2,
                    -1,
                    0,
                    1,
                    2,
                    3,
                    4,
                    5,
                    6,
                    7,
                    8,
                    9,
                    $emax - 1,
                    $emax,
                ] {
                    for power in 0..=$digits {
                        let ten = 10_u128.pow(power);
                        for coefficient in [0, 1, ten - 1, ten, ten + 1, maximum] {
                            if coefficient > maximum {
                                continue;
                            }
                            for negative in [false, true] {
                                let text = std::format!(
                                    "{}{}e{}",
                                    if negative { "-" } else { "" },
                                    coefficient,
                                    exp
                                );
                                check(<$d>::parse(&text).unwrap());
                            }
                        }
                    }
                }
                let mut state = 0x9e3779b97f4a7c15_u64;
                for _ in 0..10000 {
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    let high = state;
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    let coefficient =
                        ((u128::from(high) << 64) | u128::from(state)) % (maximum + 1);
                    let exp = $emin + (state % u64::try_from($emax - $emin + 1).unwrap()) as i32;
                    check(
                        <$d>::parse(&std::format!(
                            "{}{}e{}",
                            if state & 1 == 0 { "-" } else { "" },
                            coefficient,
                            exp
                        ))
                        .unwrap(),
                    );
                }
            }
        };
    }
    width!(d32_hint_boundaries, Decimal32, 7, -101, 90, 14);
    width!(d64_hint_boundaries, Decimal64, 16, -398, 369, 18);
    width!(d128_hint_boundaries, Decimal128, 34, -6176, 6111, 28);
}
