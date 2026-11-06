//! Live PostgreSQL 18.6 tests. Missing configuration is a failure, not a skip.
use bidecimal::{Decimal32, Decimal64, Decimal128};
type Error = Box<dyn std::error::Error + Send + Sync>;
type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
struct ContextError {
    width: &'static str,
    input: String,
    stage: &'static str,
    cause: Error,
}
impl std::fmt::Display for ContextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "width={} source={:?} stage={}: {}",
            self.width, self.input, self.stage, self.cause
        )
    }
}
impl std::error::Error for ContextError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.cause.as_ref())
    }
}
struct CaseContext {
    width: &'static str,
    input: String,
}
impl CaseContext {
    fn new(width: &'static str, input: impl Into<String>) -> Self {
        Self {
            width,
            input: input.into(),
        }
    }
    fn error(&self, stage: &'static str, cause: impl Into<Error>) -> Error {
        Box::new(ContextError {
            width: self.width,
            input: self.input.clone(),
            stage,
            cause: cause.into(),
        })
    }
}
#[test]
fn contextual_errors_preserve_source_chain() {
    let context = CaseContext::new("Decimal32", "Infinity");
    let inner = context.error("read", bidecimal::Error::InvalidInput);
    let error = context.error("unconstrained", inner);
    let text = error.to_string();
    assert!(text.contains("width=Decimal32"));
    assert!(text.contains("source=\"Infinity\""));
    assert!(text.contains("stage=read"));
    assert_codec_error(error, bidecimal::Error::InvalidInput);
    let sql = context.error("insert", std::io::Error::other("SQL execution failed"));
    assert!(
        sql.source()
            .unwrap()
            .downcast_ref::<std::io::Error>()
            .is_some()
    );
}
#[cfg(feature = "postgres-diesel")]
use diesel::{Connection as _, RunQueryDsl as _};
#[cfg(feature = "postgres-sqlx")]
use sqlx::{Column as _, Connection as _, Row as _, TypeInfo as _};

#[cfg(feature = "postgres-diesel")]
#[derive(diesel::deserialize::QueryableByName)]
struct TextRow {
    #[diesel(sql_type = diesel::sql_types::Text)]
    v: String,
}
#[cfg(feature = "postgres-diesel")]
#[derive(diesel::deserialize::QueryableByName)]
struct BytesRow {
    #[diesel(sql_type = diesel::sql_types::Binary)]
    v: Vec<u8>,
}
macro_rules! diesel_row {
    ($name:ident, $ty:ty) => {
        #[cfg(feature = "postgres-diesel")]
        #[derive(diesel::deserialize::QueryableByName)]
        struct $name {
            #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Numeric>)]
            v: Option<$ty>,
        }
    };
}
diesel_row!(D32, Decimal32);
diesel_row!(D64, Decimal64);
diesel_row!(D128, Decimal128);
enum Driver {
    #[cfg(feature = "postgres-sqlx")]
    Sqlx(sqlx::PgConnection),
    #[cfg(feature = "postgres-diesel")]
    Diesel(diesel::PgConnection),
    #[cfg(feature = "postgres-sync")]
    Sync(postgres::Client),
    #[cfg(feature = "postgres-async")]
    Async(tokio_postgres::Client),
}
macro_rules! decimal_methods {
    ($insert:ident, $read:ident, $wire:ident, $ty:ty, $row:ident) => {
        async fn $insert(&mut self, query: &str, value: Option<$ty>) -> Result<()> {
            let context = CaseContext::new(stringify!($ty), format!("{value:?}; query={query}"));
            let result: Result<_> = async {
                match self {
                    #[cfg(feature = "postgres-sqlx")]
                    Self::Sqlx(c) => {
                        // SQLx 0.9 Query::bind formats encoding errors into a
                        // string. try_bind preserves the typed codec error.
                        let mut statement = sqlx::query(sqlx::AssertSqlSafe(query));
                        statement
                            .try_bind(value)
                            .map_err(|error| context.error("encode/try_bind", error))?;
                        statement.execute(c).await?;
                    }
                    #[cfg(feature = "postgres-diesel")]
                    Self::Diesel(c) => {
                        diesel::sql_query(query)
                            .bind::<diesel::sql_types::Nullable<diesel::sql_types::Numeric>, _>(
                                value,
                            )
                            .execute(c)?;
                    }
                    #[cfg(feature = "postgres-sync")]
                    Self::Sync(c) => {
                        c.execute(query, &[&value])?;
                    }
                    #[cfg(feature = "postgres-async")]
                    Self::Async(c) => {
                        c.execute(query, &[&value]).await?;
                    }
                }
                Ok(())
            }
            .await;
            result.map_err(|error| context.error("insert", error))
        }
        async fn $read(&mut self, query: &str) -> Result<Option<$ty>> {
            let context = CaseContext::new(stringify!($ty), query);
            let result: Result<_> = async {
                match self {
                    #[cfg(feature = "postgres-sqlx")]
                    Self::Sqlx(c) => {
                        let row = sqlx::query(sqlx::AssertSqlSafe(query)).fetch_one(c).await?;
                        assert_eq!(row.columns()[0].type_info().name(), "NUMERIC");
                        Ok(row.try_get(0)?)
                    }
                    #[cfg(feature = "postgres-diesel")]
                    Self::Diesel(c) => Ok(diesel::sql_query(query).get_result::<$row>(c)?.v),
                    #[cfg(feature = "postgres-sync")]
                    Self::Sync(c) => {
                        let row = c.query_one(query, &[])?;
                        assert_eq!(*row.columns()[0].type_(), postgres::types::Type::NUMERIC);
                        Ok(row.try_get(0)?)
                    }
                    #[cfg(feature = "postgres-async")]
                    Self::Async(c) => {
                        let row = c.query_one(query, &[]).await?;
                        assert_eq!(
                            *row.columns()[0].type_(),
                            tokio_postgres::types::Type::NUMERIC
                        );
                        Ok(row.try_get(0)?)
                    }
                }
            }
            .await;
            result.map_err(|error| context.error("read", error))
        }
        async fn $wire(&mut self, value: $ty) -> Result<Vec<u8>> {
            let context = CaseContext::new(stringify!($ty), format!("{value:?}"));
            let result: Result<_> = async {
                let query = "SELECT numeric_send($1::numeric) AS v";
                match self {
                    #[cfg(feature = "postgres-sqlx")]
                    Self::Sqlx(c) => Ok(sqlx::query(query)
                        .bind(value)
                        .fetch_one(c)
                        .await?
                        .try_get(0)?),
                    #[cfg(feature = "postgres-diesel")]
                    Self::Diesel(c) => Ok(diesel::sql_query(query)
                        .bind::<diesel::sql_types::Numeric, _>(value)
                        .get_result::<BytesRow>(c)?
                        .v),
                    #[cfg(feature = "postgres-sync")]
                    Self::Sync(c) => Ok(c.query_one(query, &[&value])?.try_get(0)?),
                    #[cfg(feature = "postgres-async")]
                    Self::Async(c) => Ok(c.query_one(query, &[&value]).await?.try_get(0)?),
                }
            }
            .await;
            result.map_err(|error| context.error("wire", error))
        }
    };
}
impl Driver {
    decimal_methods!(insert32, read32, wire32, Decimal32, D32);
    decimal_methods!(insert64, read64, wire64, Decimal64, D64);
    decimal_methods!(insert128, read128, wire128, Decimal128, D128);
    async fn execute(&mut self, query: &str) -> Result<()> {
        let context = CaseContext::new("SQL", query);
        let result: Result<_> = async {
            match self {
                #[cfg(feature = "postgres-sqlx")]
                Self::Sqlx(c) => {
                    sqlx::raw_sql(sqlx::AssertSqlSafe(query)).execute(c).await?;
                }
                #[cfg(feature = "postgres-diesel")]
                Self::Diesel(c) => {
                    diesel::sql_query(query).execute(c)?;
                }
                #[cfg(feature = "postgres-sync")]
                Self::Sync(c) => c.batch_execute(query)?,
                #[cfg(feature = "postgres-async")]
                Self::Async(c) => c.batch_execute(query).await?,
            }
            Ok(())
        }
        .await;
        result.map_err(|error| context.error("setup/execute", error))
    }
    async fn text(&mut self, query: &str) -> Result<String> {
        let context = CaseContext::new("SQL", query);
        let result: Result<_> = async {
            match self {
                #[cfg(feature = "postgres-sqlx")]
                Self::Sqlx(c) => Ok(sqlx::query(sqlx::AssertSqlSafe(query))
                    .fetch_one(c)
                    .await?
                    .try_get(0)?),
                #[cfg(feature = "postgres-diesel")]
                Self::Diesel(c) => Ok(diesel::sql_query(query).get_result::<TextRow>(c)?.v),
                #[cfg(feature = "postgres-sync")]
                Self::Sync(c) => Ok(c.query_one(query, &[])?.try_get(0)?),
                #[cfg(feature = "postgres-async")]
                Self::Async(c) => Ok(c.query_one(query, &[]).await?.try_get(0)?),
            }
        }
        .await;
        result.map_err(|error| context.error("read/text", error))
    }
    async fn bytes(&mut self, query: &str) -> Result<Vec<u8>> {
        let context = CaseContext::new("SQL", query);
        let result: Result<_> = async {
            match self {
                #[cfg(feature = "postgres-sqlx")]
                Self::Sqlx(c) => Ok(sqlx::query(sqlx::AssertSqlSafe(query))
                    .fetch_one(c)
                    .await?
                    .try_get(0)?),
                #[cfg(feature = "postgres-diesel")]
                Self::Diesel(c) => Ok(diesel::sql_query(query).get_result::<BytesRow>(c)?.v),
                #[cfg(feature = "postgres-sync")]
                Self::Sync(c) => Ok(c.query_one(query, &[])?.try_get(0)?),
                #[cfg(feature = "postgres-async")]
                Self::Async(c) => Ok(c.query_one(query, &[]).await?.try_get(0)?),
            }
        }
        .await;
        result.map_err(|error| context.error("wire/oracle", error))
    }
}
fn codec_error_matches(
    error: &(dyn std::error::Error + 'static),
    expected: bidecimal::Error,
) -> bool {
    let mut cause = error;
    loop {
        if let Some(actual) = cause.downcast_ref::<bidecimal::Error>() {
            return *actual == expected;
        }
        // Diesel 2.3.14 result::Error::source calls inner.source(), skipping
        // the boxed codec leaf. Inspect only its codec-bearing variants.
        #[cfg(feature = "postgres-diesel")]
        if let Some(
            diesel::result::Error::DeserializationError(inner)
            | diesel::result::Error::SerializationError(inner),
        ) = cause.downcast_ref::<diesel::result::Error>()
        {
            cause = inner.as_ref();
            continue;
        }
        match cause.source() {
            Some(next) => cause = next,
            None => return false,
        }
    }
}
fn assert_codec_error(error: Error, expected: bidecimal::Error) {
    assert!(
        codec_error_matches(error.as_ref(), expected),
        "expected codec {expected}, got unrelated {error:?}"
    );
}

#[cfg(feature = "postgres-diesel")]
#[test]
fn diesel_wrapped_codec_errors_are_typed() {
    use diesel::result::Error as DriverError;
    let context = CaseContext::new("Decimal32", "driver wrapper regression");
    for expected in [
        bidecimal::Error::InvalidInput,
        bidecimal::Error::Inexact,
        bidecimal::Error::Overflow,
    ] {
        for wrapped in [
            DriverError::DeserializationError(Box::new(expected)),
            DriverError::SerializationError(Box::new(expected)),
        ] {
            // Reproduce the actual Diesel wrapper behavior, not a mock source chain.
            assert!(std::error::Error::source(&wrapped).is_none());
            let error = context.error("read/insert", wrapped);
            assert!(codec_error_matches(error.as_ref(), expected));
            let different = if expected == bidecimal::Error::InvalidInput {
                bidecimal::Error::Inexact
            } else {
                bidecimal::Error::InvalidInput
            };
            assert!(!codec_error_matches(error.as_ref(), different));
        }
    }
    for wrapped in [
        DriverError::NotFound,
        DriverError::DeserializationError("result is not exactly representable".into()),
        DriverError::SerializationError("invalid input".into()),
    ] {
        for expected in [bidecimal::Error::InvalidInput, bidecimal::Error::Inexact] {
            assert!(!codec_error_matches(&wrapped, expected));
        }
    }
}

#[cfg(feature = "postgres-sqlx")]
#[test]
fn sqlx_try_bind_preserves_codec_errors() {
    use sqlx::Execute as _;
    macro_rules! check {
        ($d:ty) => {{
            let value = -<$d>::NAN;
            let mut old =
                sqlx::query::<sqlx::Postgres>(sqlx::AssertSqlSafe("SELECT $1")).bind(Some(value));
            let erased = old.take_arguments().unwrap_err();
            assert_eq!(
                erased.to_string(),
                "Encoding argument $1 failed: invalid input"
            );
            let wrapped = sqlx::Error::Encode(erased);
            // The old API irreversibly erased the type; never accept its text as proof.
            assert!(!codec_error_matches(
                &wrapped,
                bidecimal::Error::InvalidInput
            ));
            let mut query = sqlx::query::<sqlx::Postgres>(sqlx::AssertSqlSafe("SELECT $1"));
            let typed = query.try_bind(Some(value)).unwrap_err();
            let context = CaseContext::new(stringify!($d), "signed NaN");
            let wrapped = context.error("insert", typed);
            assert_codec_error(wrapped, bidecimal::Error::InvalidInput);
            let mut valid = sqlx::query::<sqlx::Postgres>(sqlx::AssertSqlSafe("SELECT $1"));
            valid.try_bind(Some(<$d>::INFINITY)).unwrap();
            let new_arguments = valid.take_arguments().unwrap().unwrap();
            let mut bound = sqlx::query::<sqlx::Postgres>(sqlx::AssertSqlSafe("SELECT $1"))
                .bind(Some(<$d>::INFINITY));
            let old_arguments = bound.take_arguments().unwrap().unwrap();
            // Both APIs call the same Arguments::add on success (SQLx query.rs).
            assert_eq!(format!("{new_arguments:?}"), format!("{old_arguments:?}"));
        }};
    }
    check!(Decimal32);
    check!(Decimal64);
    check!(Decimal128);
    let misleading = sqlx::Error::Decode("invalid input".into());
    assert!(!codec_error_matches(
        &misleading,
        bidecimal::Error::InvalidInput
    ));
}
fn assert_numeric_overflow(error: Error) {
    let mut cause: &(dyn std::error::Error + 'static) = error.as_ref();
    loop {
        #[cfg(feature = "postgres-sqlx")]
        if let Some(e) = cause.downcast_ref::<sqlx::Error>() {
            assert_eq!(
                e.as_database_error().and_then(|e| e.code()).as_deref(),
                Some("22003")
            );
            return;
        }
        #[cfg(any(feature = "postgres-sync", feature = "postgres-async"))]
        if let Some(e) = cause.downcast_ref::<tokio_postgres_error::Error>() {
            assert_eq!(e.code().map(|c| c.code()), Some("22003"));
            return;
        }
        #[cfg(feature = "postgres-diesel")]
        if let Some(diesel::result::Error::DatabaseError(_, info)) =
            cause.downcast_ref::<diesel::result::Error>()
        {
            assert!(
                info.message().contains("numeric field overflow"),
                "{}",
                info.message()
            );
            return;
        }
        match cause.source() {
            Some(next) => cause = next,
            None => break,
        }
    }
    panic!("expected numeric server overflow, got {error:?}");
}
// The synchronous crate reexports exactly the same protocol error type.
#[cfg(feature = "postgres-sync")]
use postgres as tokio_postgres_error;
#[cfg(all(feature = "postgres-async", not(feature = "postgres-sync")))]
use tokio_postgres as tokio_postgres_error;

async fn suite(mut driver: Driver) -> Result<()> {
    let mut context = CaseContext::new("SQL", "server/setup");
    assert_eq!(
        driver
            .text("SELECT current_setting('server_version_num') AS v")
            .await
            .map_err(|error| context.error("read/text", error))?,
        "180006"
    );
    eprintln!(
        "server={}",
        driver
            .text("SELECT version() AS v")
            .await
            .map_err(|error| context.error("read/text", error))?
    );
    driver
        .execute("CREATE TEMPORARY TABLE bid_decimal(v NUMERIC)")
        .await
        .map_err(|error| context.error("setup/execute", error))?;
    macro_rules! widths {
        ($ty:ty, $bits:ty, $insert:ident, $read:ident, $wire:ident, $long:literal) => {{
            let sources = vec![
                "1.50".to_owned(),
                "0".to_owned(),
                "-0.0000".to_owned(),
                "-123.4500".to_owned(),
                $long.to_owned(),
                format!("1e{}", <$ty>::MIN_EXP),
                format!("{}e{}", "9".repeat(<$ty>::DIGITS as usize), <$ty>::MAX_EXP),
            ];
            for source in sources {
                context = CaseContext::new(stringify!($ty), source.clone());
                let value: $ty = source
                    .parse()
                    .map_err(|error| context.error("parse", error))?;
                driver
                    .execute("DELETE FROM bid_decimal")
                    .await
                    .map_err(|error| context.error("setup/execute", error))?;
                driver
                    .$insert("INSERT INTO bid_decimal VALUES ($1)", Some(value))
                    .await
                    .map_err(|error| context.error("insert", error))?;
                let got = driver
                    .$read("SELECT v FROM bid_decimal")
                    .await
                    .map_err(|error| context.error("read", error))?
                    .unwrap();
                assert_eq!(got, value, "{}: {source}", stringify!($ty));
                if value.is_zero() {
                    assert!(!got.is_sign_negative());
                }
                #[cfg(feature = "postgres-sqlx")]
                if let Driver::Sqlx(connection) = &mut driver {
                    let row = sqlx::raw_sql("SELECT v FROM bid_decimal")
                        .fetch_one(connection)
                        .await
                        .map_err(|error| context.error("read/text", error))?;
                    let text_got: Option<$ty> = row
                        .try_get(0)
                        .map_err(|error| context.error("read/text", error))?;
                    assert_eq!(text_got, Some(value), "SQLx simple-query text numeric");
                }
                // Independent server decimal parser creates expected wire bytes.
                // These finite inputs retain the same display scale on both paths.
                let expected = driver
                    .bytes(&format!("SELECT numeric_send('{source}'::numeric) AS v"))
                    .await
                    .map_err(|error| context.error("wire", error))?;
                assert_eq!(
                    driver
                        .$wire(value)
                        .await
                        .map_err(|error| context.error("wire", error))?,
                    expected,
                    "wire {} {source}",
                    stringify!($ty)
                );
                #[cfg(feature = "postgres-sqlx")]
                {
                    let mut encoded = sqlx::postgres::PgArgumentBuffer::default();
                    let _ = <$ty as sqlx::Encode<'_, sqlx::Postgres>>::encode_by_ref(
                        &value,
                        &mut encoded,
                    )
                    .map_err(|error| context.error("wire/client", error))?;
                    assert_eq!(
                        &encoded[..],
                        expected.as_slice(),
                        "client bytes {} {source}",
                        stringify!($ty)
                    );
                }
                #[cfg(any(feature = "postgres-sync", feature = "postgres-async"))]
                {
                    let mut encoded = bytes::BytesMut::new();
                    let _ = <$ty as tokio_postgres_error::types::ToSql>::to_sql(
                        &value,
                        &tokio_postgres_error::types::Type::NUMERIC,
                        &mut encoded,
                    )
                    .map_err(|error| context.error("wire/client", error))?;
                    assert_eq!(
                        &encoded[..],
                        expected.as_slice(),
                        "rust-postgres client bytes"
                    );
                }
            }
            for (value, literal) in [
                (<$ty>::NAN, "NaN"),
                (<$ty>::INFINITY, "Infinity"),
                (<$ty>::NEG_INFINITY, "-Infinity"),
            ] {
                context = CaseContext::new(stringify!($ty), literal);
                driver
                    .execute("DELETE FROM bid_decimal")
                    .await
                    .map_err(|error| context.error("setup/execute", error))?;
                driver
                    .$insert("INSERT INTO bid_decimal VALUES ($1)", Some(value))
                    .await
                    .map_err(|error| context.error("insert", error))?;
                let got = driver
                    .$read("SELECT v FROM bid_decimal")
                    .await
                    .map_err(|error| context.error("read", error))?
                    .unwrap();
                if value.is_nan() {
                    assert!(got.is_nan());
                    assert_eq!(got.to_bits(), <$ty>::NAN.to_bits());
                } else {
                    assert_eq!(got, value);
                }
                let expected = driver
                    .bytes(&format!("SELECT numeric_send('{literal}'::numeric) AS v"))
                    .await
                    .map_err(|error| context.error("wire", error))?;
                assert_eq!(
                    driver
                        .$wire(value)
                        .await
                        .map_err(|error| context.error("wire", error))?,
                    expected
                );
            }
            context = CaseContext::new(stringify!($ty), "NULL");
            driver
                .execute("DELETE FROM bid_decimal")
                .await
                .map_err(|error| context.error("setup/execute", error))?;
            driver
                .$insert("INSERT INTO bid_decimal VALUES ($1)", None)
                .await
                .map_err(|error| context.error("insert", error))?;
            assert!(
                driver
                    .$read("SELECT v FROM bid_decimal")
                    .await
                    .map_err(|error| context.error("read", error))?
                    .is_none()
            );
            context = CaseContext::new(stringify!($ty), "1.50 cohort");
            let cohort = driver
                .$read("SELECT 1.50::numeric AS v")
                .await
                .map_err(|error| context.error("read", error))?
                .unwrap();
            assert_eq!(
                cohort
                    .to_mantissa_and_scale()
                    .map_err(|error| context.error("read/cohort", error))?,
                (150, 2)
            );
            context = CaseContext::new(stringify!($ty), concat!($long, ".00 cohort"));
            let short = driver
                .$read(&format!("SELECT '{}.00'::numeric AS v", $long))
                .await
                .map_err(|error| context.error("read", error))?
                .unwrap();
            assert_eq!(
                short,
                $long
                    .parse::<$ty>()
                    .map_err(|error| context.error("parse", error))?
            );
            assert_codec_error(
                driver
                    .$read("SELECT 12345678901234567890123456789012345::numeric AS v")
                    .await
                    .expect_err("overprecision"),
                bidecimal::Error::Inexact,
            );
            // Canonical public BID construction supplies all three forbidden NaN forms.
            context = CaseContext::new(stringify!($ty), "payload NaN");
            let payload = <$ty>::from_bits(<$ty>::NAN.to_bits() | 1)
                .map_err(|error| context.error("construct/payload NaN", error))?;
            context = CaseContext::new(stringify!($ty), "signaling NaN");
            let signaling =
                <$ty>::from_bits(<$ty>::NAN.to_bits() | (1 as $bits) << (<$bits>::BITS - 7))
                    .map_err(|error| context.error("construct/signaling NaN", error))?;
            for value in [-<$ty>::NAN, payload, signaling] {
                assert_codec_error(
                    driver
                        .$insert("INSERT INTO bid_decimal VALUES ($1)", Some(value))
                        .await
                        .expect_err("NaN representation loss"),
                    bidecimal::Error::InvalidInput,
                );
            }
        }};
    }
    widths!(Decimal32, u32, insert32, read32, wire32, "1234567");
    widths!(Decimal64, u64, insert64, read64, wire64, "1234567890123456");
    widths!(
        Decimal128,
        u128,
        insert128,
        read128,
        wire128,
        "1234567890123456789012345678901234"
    );
    context = CaseContext::new("Decimal32", "1234567.00 cohort");
    assert_eq!(
        driver
            .read32("SELECT 1234567.00::numeric AS v")
            .await
            .map_err(|error| context.error("read", error))?
            .unwrap()
            .to_mantissa_and_scale()
            .map_err(|error| context.error("read/cohort", error))?,
        (1234567, 0)
    );
    context = CaseContext::new("Decimal128", "NUMERIC(5,2) setup");
    driver
        .execute("CREATE TEMPORARY TABLE bid_round(v NUMERIC(5,2))")
        .await
        .map_err(|error| context.error("setup/execute", error))?;
    for (input, expected) in [("1.005", "1.01"), ("-1.005", "-1.01")] {
        context = CaseContext::new("Decimal128", format!("{input}; NUMERIC(5,2)"));
        driver
            .execute("DELETE FROM bid_round")
            .await
            .map_err(|error| context.error("setup/execute", error))?;
        driver
            .insert128(
                "INSERT INTO bid_round VALUES ($1)",
                Some(
                    input
                        .parse()
                        .map_err(|error| context.error("parse", error))?,
                ),
            )
            .await
            .map_err(|error| context.error("insert", error))?;
        assert_eq!(
            driver
                .text("SELECT v::text AS v FROM bid_round")
                .await
                .map_err(|error| context.error("read/text", error))?,
            expected
        );
    }
    context = CaseContext::new("Decimal128", "12345; NUMERIC(2,-3)");
    driver
        .execute("CREATE TEMPORARY TABLE bid_negscale(v NUMERIC(2,-3))")
        .await
        .map_err(|error| context.error("setup/execute", error))?;
    driver
        .insert128(
            "INSERT INTO bid_negscale VALUES ($1)",
            Some(
                "12345"
                    .parse()
                    .map_err(|error| context.error("parse", error))?,
            ),
        )
        .await
        .map_err(|error| context.error("insert", error))?;
    assert_eq!(
        driver
            .read128("SELECT v FROM bid_negscale")
            .await
            .map_err(|error| context.error("read", error))?
            .unwrap()
            .to_mantissa_and_scale()
            .map_err(|error| context.error("read/cohort", error))?,
        (12000, 0)
    );
    context = CaseContext::new("Decimal128", "0.001234; NUMERIC(3,5)");
    driver
        .execute("CREATE TEMPORARY TABLE bid_small(v NUMERIC(3,5))")
        .await
        .map_err(|error| context.error("setup/execute", error))?;
    driver
        .insert128(
            "INSERT INTO bid_small VALUES ($1)",
            Some(
                "0.001234"
                    .parse()
                    .map_err(|error| context.error("parse", error))?,
            ),
        )
        .await
        .map_err(|error| context.error("insert", error))?;
    assert_eq!(
        driver
            .text("SELECT v::text AS v FROM bid_small")
            .await
            .map_err(|error| context.error("read/text", error))?,
        "0.00123"
    );
    context = CaseContext::new("Decimal128", "NUMERIC(3,2) setup");
    driver
        .execute("CREATE TEMPORARY TABLE bid_overflow(v NUMERIC(3,2))")
        .await
        .map_err(|error| context.error("setup/execute", error))?;
    for input in ["9.995", "-9.995"] {
        context = CaseContext::new("Decimal128", format!("{input}; NUMERIC(3,2)"));
        assert_numeric_overflow(
            driver
                .insert128(
                    "INSERT INTO bid_overflow VALUES ($1)",
                    Some(
                        input
                            .parse()
                            .map_err(|error| context.error("parse", error))?,
                    ),
                )
                .await
                .expect_err("rounded overflow"),
        );
    }
    context = CaseContext::new("Decimal128", "NaN; NUMERIC(3,2)");
    driver
        .insert128(
            "INSERT INTO bid_overflow VALUES ($1)",
            Some(Decimal128::NAN),
        )
        .await
        .map_err(|error| context.error("insert", error))?;
    assert!(
        driver
            .read128("SELECT v FROM bid_overflow")
            .await
            .map_err(|error| context.error("read", error))?
            .unwrap()
            .is_nan()
    );
    assert_numeric_overflow(
        driver
            .insert128(
                "INSERT INTO bid_overflow VALUES ($1)",
                Some(Decimal128::INFINITY),
            )
            .await
            .expect_err("constrained Infinity"),
    );
    // 131072 integer digits use 32768 base-10000 groups: unsigned ndigits.
    context = CaseContext::new("Decimal128", "131072-digit numeric / unsigned 32768 groups");
    let giant = "repeat('1',131072)::numeric";
    let wire = driver
        .bytes(&format!("SELECT numeric_send({giant}) AS v"))
        .await
        .map_err(|error| context.error("wire", error))?;
    assert_eq!(u16::from_be_bytes([wire[0], wire[1]]), 32768);
    assert_eq!(wire.len(), 8 + 2 * 32768);
    assert_codec_error(
        driver
            .read128(&format!("SELECT {giant} AS v"))
            .await
            .expect_err("huge integer overflow"),
        bidecimal::Error::Overflow,
    );
    assert_codec_error(
        driver
            .read128("SELECT ('0.' || repeat('0',16382) || '1')::numeric AS v")
            .await
            .expect_err("tiny inexact"),
        bidecimal::Error::Inexact,
    );
    Ok(())
}

fn url() -> Result<String> {
    std::env::var("BIDECIMAL_TEST_POSTGRES_URL")
        .map_err(|_| "BIDECIMAL_TEST_POSTGRES_URL is required; live tests cannot be skipped".into())
}
#[cfg(feature = "postgres-sqlx")]
#[tokio::test(flavor = "current_thread")]
async fn sqlx_postgres() -> Result<()> {
    suite(Driver::Sqlx(sqlx::PgConnection::connect(&url()?).await?)).await
}
#[cfg(feature = "postgres-diesel")]
#[tokio::test(flavor = "current_thread")]
async fn diesel_postgres() -> Result<()> {
    suite(Driver::Diesel(diesel::PgConnection::establish(&url()?)?)).await
}
#[cfg(feature = "postgres-async")]
#[tokio::test(flavor = "current_thread")]
async fn tokio_postgres() -> Result<()> {
    let (client, connection) = tokio_postgres::connect(&url()?, tokio_postgres::NoTls).await?;
    let connection = tokio::spawn(connection);
    let result = suite(Driver::Async(client)).await;
    connection.await??;
    result
}
#[cfg(feature = "postgres-sync")]
#[test]
fn postgres_sync() -> Result<()> {
    // postgres owns its runtime and must not be called inside a Tokio runtime.
    // Every Sync arm completes immediately, so one safe poll runs the shared suite.
    let client = postgres::Client::connect(&url()?, postgres::NoTls)?;
    let future = suite(Driver::Sync(client));
    let mut future = std::pin::pin!(future);
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    match std::future::Future::poll(future.as_mut(), &mut context) {
        std::task::Poll::Ready(result) => result,
        std::task::Poll::Pending => panic!("synchronous postgres suite unexpectedly yielded"),
    }
}
