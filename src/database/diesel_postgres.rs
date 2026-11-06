//! Backend-only Diesel NUMERIC binary conversion, including infinity.
use super::pg_numeric::{self, PgNumeric};
use crate::{Decimal32, Decimal64, Decimal128, Error};
use diesel_2::deserialize::FromSql;
use diesel_2::pg::{Pg, PgValue};
use diesel_2::serialize::{IsNull, Output, ToSql};
use diesel_2::sql_types::Numeric;
use diesel_2::{deserialize, serialize};
use std::io::Write;

macro_rules! impl_postgres {
    ($decimal:ty) => {
        impl ToSql<Numeric, Pg> for $decimal {
            fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
                let plan = PgNumeric::new(*self)?;
                plan.write(|part| out.write_all(part))?;
                Ok(IsNull::No)
            }
        }
        impl FromSql<Numeric, Pg> for $decimal {
            fn from_sql(value: PgValue<'_>) -> deserialize::Result<Self> {
                // PostgreSQL's built-in NUMERIC OID is stable, unlike a user
                // defined type's OID. Do not reinterpret other binary types.
                if value.get_oid().get() != 1700 {
                    return Err(Error::InvalidInput.into());
                }
                Ok(pg_numeric::decode(value.as_bytes())?)
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
    use diesel_2::pg::{PgMetadataLookup, PgTypeMetadata};
    use diesel_2::query_builder::bind_collector::{BindCollector, RawBytesBindCollector};
    use diesel_2::{deserialize::FromSqlRow, expression::AsExpression};

    struct NoLookup;
    impl PgMetadataLookup for NoLookup {
        fn lookup_type(&mut self, _: &str, _: Option<&str>) -> PgTypeMetadata {
            panic!("built-in NUMERIC must not require a server lookup")
        }
    }

    #[test]
    fn diesel_postgres_expression_row_and_bind_collector_all_widths() {
        fn expression<T: AsExpression<Numeric>>() {}
        fn row<T: FromSqlRow<Numeric, Pg>>() {}
        macro_rules! check {
            ($d:ty) => {{
                expression::<$d>();
                expression::<&$d>();
                row::<$d>();
                let mut collector = RawBytesBindCollector::<Pg>::new();
                collector
                    .push_bound_value::<Numeric, _>(&<$d>::parse("1.50").unwrap(), &mut NoLookup)
                    .unwrap();
                let before = std::format!("{collector:?}");
                assert!(before.contains("[0, 2, 0, 0, 0, 0, 0, 2, 0, 1, 19, 136]"));
                assert!(
                    collector
                        .push_bound_value::<Numeric, _>(&-<$d>::NAN, &mut NoLookup)
                        .is_err()
                );
                assert_eq!(std::format!("{collector:?}"), before);
                for decimal in [<$d>::NAN, <$d>::INFINITY, <$d>::NEG_INFINITY] {
                    collector
                        .push_bound_value::<Numeric, _>(&decimal, &mut NoLookup)
                        .unwrap();
                }
            }};
        }
        check!(Decimal32);
        check!(Decimal64);
        check!(Decimal128);
    }
}
