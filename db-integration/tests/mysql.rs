//! Run only against a disposable server. Every table is connection-local TEMPORARY.
//! A missing URL is an error, never a silently skipped test.
use bidecimal::{Decimal32, Decimal64, Decimal128};
type Error = Box<dyn std::error::Error + Send + Sync>;
type Result<T> = std::result::Result<T, Error>;

#[cfg(feature = "mysql-diesel")]
#[derive(Debug)]
struct DieselQueryError {
    operation: &'static str,
    sql: String,
    error: diesel::result::Error,
}
#[cfg(feature = "mysql-diesel")]
impl std::fmt::Display for DieselQueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "diesel operation={} sql={:?}: {}",
            self.operation, self.sql, self.error
        )
    }
}
#[cfg(feature = "mysql-diesel")]
impl std::error::Error for DieselQueryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}
#[cfg(feature = "mysql-diesel")]
fn diesel_query_error(operation: &'static str, sql: &str, error: diesel::result::Error) -> Error {
    Box::new(DieselQueryError {
        operation,
        sql: sql.into(),
        error,
    })
}

#[cfg(feature = "mysql-diesel")]
use diesel::{Connection as _, RunQueryDsl as _};
#[cfg(feature = "mysql-sync")]
use mysql::prelude::Queryable as _;
#[cfg(feature = "mysql-async")]
use mysql_async::prelude::Queryable as _;
#[cfg(feature = "mysql-sqlx")]
use sqlx::{Column as _, Connection as _, Row as _, TypeInfo as _};

#[cfg(feature = "mysql-diesel")]
#[derive(diesel::deserialize::QueryableByName)]
struct TextRow {
    #[diesel(sql_type = diesel::sql_types::Text)]
    v: String,
}
macro_rules! diesel_row {
    ($name:ident, $ty:ty) => {
        #[cfg(feature = "mysql-diesel")]
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
    #[cfg(feature = "mysql-sqlx")]
    Sqlx(sqlx::MySqlConnection),
    #[cfg(feature = "mysql-diesel")]
    Diesel(diesel::MysqlConnection),
    #[cfg(feature = "mysql-sync")]
    Sync(mysql::Conn),
    #[cfg(feature = "mysql-async")]
    Async(mysql_async::Conn),
}

// Each arm binds the actual public Decimal type, not its Display string.
// mysql_common intentionally requires an explicit fallible Value conversion.
macro_rules! decimal_methods {
    ($insert:ident, $read:ident, $ty:ty, $row:ident) => {
        async fn $insert(&mut self, query: &str, value: Option<$ty>) -> Result<()> {
            match self {
                #[cfg(feature = "mysql-sqlx")]
                Self::Sqlx(c) => {
                    sqlx::query(sqlx::AssertSqlSafe(query))
                        .bind(value)
                        .execute(c)
                        .await?;
                }
                #[cfg(feature = "mysql-diesel")]
                Self::Diesel(c) => {
                    diesel::sql_query(query)
                        .bind::<diesel::sql_types::Nullable<diesel::sql_types::Numeric>, _>(value)
                        .execute(c)
                        .map_err(|e| diesel_query_error(stringify!($insert), query, e))?;
                }
                #[cfg(feature = "mysql-sync")]
                Self::Sync(c) => {
                    let value = value
                        .map(mysql::Value::try_from)
                        .transpose()?
                        .unwrap_or(mysql::Value::NULL);
                    c.exec_drop(query, (value,))?;
                }
                #[cfg(feature = "mysql-async")]
                Self::Async(c) => {
                    let value = value
                        .map(mysql_async::Value::try_from)
                        .transpose()?
                        .unwrap_or(mysql_async::Value::NULL);
                    c.exec_drop(query, (value,)).await?;
                }
            }
            Ok(())
        }
        async fn $read(&mut self, query: &str, _prepared: bool) -> Result<Option<$ty>> {
            match self {
                #[cfg(feature = "mysql-sqlx")]
                Self::Sqlx(c) => {
                    let row = if _prepared {
                        sqlx::query(sqlx::AssertSqlSafe(query)).fetch_one(c).await?
                    } else {
                        sqlx::raw_sql(sqlx::AssertSqlSafe(query))
                            .fetch_one(c)
                            .await?
                    };
                    assert_eq!(row.columns()[0].type_info().name(), "DECIMAL");
                    Ok(row.try_get(0)?)
                }
                #[cfg(feature = "mysql-diesel")]
                Self::Diesel(c) => Ok(diesel::sql_query(query)
                    .get_result::<$row>(c)
                    .map_err(|e| diesel_query_error(stringify!($read), query, e))?
                    .v),
                #[cfg(feature = "mysql-sync")]
                Self::Sync(c) => {
                    let row: mysql::Row = if _prepared {
                        c.exec_first(query, ())?
                    } else {
                        c.query_first(query)?
                    }
                    .ok_or("missing row")?;
                    assert_eq!(
                        row.columns_ref()[0].column_type(),
                        mysql::consts::ColumnType::MYSQL_TYPE_NEWDECIMAL
                    );
                    Ok(row.get_opt::<Option<$ty>, _>(0).ok_or("missing field")??)
                }
                #[cfg(feature = "mysql-async")]
                Self::Async(c) => {
                    let row: mysql_async::Row = if _prepared {
                        c.exec_first(query, ()).await?
                    } else {
                        c.query_first(query).await?
                    }
                    .ok_or("missing row")?;
                    assert_eq!(
                        row.columns_ref()[0].column_type(),
                        mysql_async::consts::ColumnType::MYSQL_TYPE_NEWDECIMAL
                    );
                    Ok(row.get_opt::<Option<$ty>, _>(0).ok_or("missing field")??)
                }
            }
        }
    };
}
impl Driver {
    fn name(&self) -> &'static str {
        match self {
            #[cfg(feature = "mysql-sqlx")]
            Self::Sqlx(_) => "sqlx",
            #[cfg(feature = "mysql-diesel")]
            Self::Diesel(_) => "diesel",
            #[cfg(feature = "mysql-sync")]
            Self::Sync(_) => "mysql",
            #[cfg(feature = "mysql-async")]
            Self::Async(_) => "mysql_async",
        }
    }

    decimal_methods!(insert32, read32, Decimal32, D32);
    decimal_methods!(insert64, read64, Decimal64, D64);
    decimal_methods!(insert128, read128, Decimal128, D128);
    async fn execute(&mut self, sql: &str) -> Result<()> {
        match self {
            #[cfg(feature = "mysql-sqlx")]
            Self::Sqlx(c) => {
                sqlx::raw_sql(sqlx::AssertSqlSafe(sql)).execute(c).await?;
            }
            #[cfg(feature = "mysql-diesel")]
            Self::Diesel(c) => {
                diesel::sql_query(sql)
                    .execute(c)
                    .map_err(|e| diesel_query_error("execute", sql, e))?;
            }
            #[cfg(feature = "mysql-sync")]
            Self::Sync(c) => c.query_drop(sql)?,
            #[cfg(feature = "mysql-async")]
            Self::Async(c) => c.query_drop(sql).await?,
        }
        Ok(())
    }
    async fn text(&mut self, sql: &str) -> Result<String> {
        match self {
            #[cfg(feature = "mysql-sqlx")]
            Self::Sqlx(c) => Ok(sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
                .fetch_one(c)
                .await?
                .try_get(0)?),
            #[cfg(feature = "mysql-diesel")]
            Self::Diesel(c) => Ok(diesel::sql_query(sql)
                .get_result::<TextRow>(c)
                .map_err(|e| diesel_query_error("text", sql, e))?
                .v),
            #[cfg(feature = "mysql-sync")]
            Self::Sync(c) => Ok(c.query_first(sql)?.ok_or("missing text row")?),
            #[cfg(feature = "mysql-async")]
            Self::Async(c) => Ok(c.query_first(sql).await?.ok_or("missing text row")?),
        }
    }
    async fn parameter_text(&mut self, sql: &str, value: Decimal128) -> Result<String> {
        match self {
            #[cfg(feature = "mysql-sqlx")]
            Self::Sqlx(c) => Ok(sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(value)
                .fetch_one(c)
                .await?
                .try_get(0)?),
            #[cfg(feature = "mysql-diesel")]
            Self::Diesel(c) => Ok(diesel::sql_query(sql)
                .bind::<diesel::sql_types::Numeric, _>(value)
                .get_result::<TextRow>(c)
                .map_err(|e| diesel_query_error("parameter_text", sql, e))?
                .v),
            #[cfg(feature = "mysql-sync")]
            Self::Sync(c) => Ok(c
                .exec_first(sql, (mysql::Value::try_from(value)?,))?
                .ok_or("missing parameter row")?),
            #[cfg(feature = "mysql-async")]
            Self::Async(c) => Ok(c
                .exec_first(sql, (mysql_async::Value::try_from(value)?,))
                .await?
                .ok_or("missing parameter row")?),
        }
    }
    async fn rollback(&mut self) -> Result<()> {
        match self {
            #[cfg(feature = "mysql-sync")]
            Self::Sync(c) => {
                let row: mysql::Row = c
                    .query_first("SELECT CAST('1.50' AS DECIMAL(4,2)), 'not-u8'")?
                    .ok_or("missing rollback row")?;
                let pointer = match row.as_ref(0) {
                    Some(mysql::Value::Bytes(v)) => v.as_ptr(),
                    _ => return Err("not decimal bytes".into()),
                };
                let error = mysql::from_row_opt::<(Decimal128, u8)>(row)
                    .expect_err("second field must fail");
                match error.0.as_ref(0) {
                    Some(mysql::Value::Bytes(v)) => {
                        assert_eq!(v.as_slice(), b"1.50");
                        assert_eq!(v.as_ptr(), pointer);
                    }
                    _ => panic!("original bytes lost"),
                }
            }
            #[cfg(feature = "mysql-async")]
            Self::Async(c) => {
                let row: mysql_async::Row = c
                    .query_first("SELECT CAST('1.50' AS DECIMAL(4,2)), 'not-u8'")
                    .await?
                    .ok_or("missing rollback row")?;
                let pointer = match row.as_ref(0) {
                    Some(mysql_async::Value::Bytes(v)) => v.as_ptr(),
                    _ => return Err("not decimal bytes".into()),
                };
                let error = mysql_async::from_row_opt::<(Decimal128, u8)>(row)
                    .expect_err("second field must fail");
                match error.0.as_ref(0) {
                    Some(mysql_async::Value::Bytes(v)) => {
                        assert_eq!(v.as_slice(), b"1.50");
                        assert_eq!(v.as_ptr(), pointer);
                    }
                    _ => panic!("original bytes lost"),
                }
            }
            #[cfg(any(feature = "mysql-sqlx", feature = "mysql-diesel"))]
            _ => {}
        }
        Ok(())
    }
    async fn warning_codes(&mut self) -> Result<Vec<u32>> {
        match self {
            #[cfg(feature = "mysql-sqlx")]
            Self::Sqlx(c) => sqlx::raw_sql("SHOW WARNINGS")
                .fetch_all(c)
                .await?
                .iter()
                .map(|r| r.try_get(1).map_err(Into::into))
                .collect(),
            #[cfg(feature = "mysql-diesel")]
            Self::Diesel(c) => {
                use diesel::connection::SimpleConnection;
                // SHOW WARNINGS sets DA_KEEP_DIAGNOSTICS in MySQL 9.7.4,
                // which its prepare path rejects. Capture diagnostics through
                // text protocol before any SELECT can clear the original area.
                const CAPTURE: &str = "GET DIAGNOSTICS @bidecimal_condition_count = NUMBER; \
GET DIAGNOSTICS CONDITION @bidecimal_condition_count @bidecimal_main_errno = MYSQL_ERRNO";
                const READ: &str = "SELECT CAST(@bidecimal_condition_count AS UNSIGNED) AS condition_count, \
CAST(@bidecimal_main_errno AS UNSIGNED) AS main_errno";
                #[derive(diesel::deserialize::QueryableByName)]
                struct CapturedDiagnostic {
                    #[diesel(sql_type = diesel::sql_types::Unsigned<diesel::sql_types::BigInt>)]
                    condition_count: u64,
                    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Unsigned<diesel::sql_types::BigInt>>)]
                    main_errno: Option<u64>,
                }
                c.batch_execute(CAPTURE)
                    .map_err(|e| diesel_query_error("warning_codes/capture", CAPTURE, e))?;
                let captured = diesel::sql_query(READ)
                    .get_result::<CapturedDiagnostic>(c)
                    .map_err(|e| diesel_query_error("warning_codes/read", READ, e))?;
                eprintln!(
                    "DIAGNOSTICS driver=diesel scope=main-condition count={} main_errno={:?}",
                    captured.condition_count, captured.main_errno
                );
                // These fixtures expect exactly one condition. Do not silently
                // replace a multi-condition SHOW WARNINGS list by one code.
                single_diesel_warning(captured.condition_count, captured.main_errno)
            }
            #[cfg(feature = "mysql-sync")]
            Self::Sync(c) => Ok(c
                .query::<(String, u32, String), _>("SHOW WARNINGS")?
                .into_iter()
                .map(|r| r.1)
                .collect()),
            #[cfg(feature = "mysql-async")]
            Self::Async(c) => Ok(c
                .query::<(String, u32, String), _>("SHOW WARNINGS")
                .await?
                .into_iter()
                .map(|r| r.1)
                .collect()),
        }
    }
}

#[cfg(feature = "mysql-diesel")]
fn single_diesel_warning(count: u64, code: Option<u64>) -> Result<Vec<u32>> {
    if count != 1 {
        return Err(format!("diesel warning_codes: expected exactly one diagnostic condition, got {count}; refusing to discard conditions").into());
    }
    let code = code.ok_or("diesel warning_codes: captured main errno is NULL")?;
    Ok(vec![u32::try_from(code)?])
}

#[cfg(feature = "mysql-diesel")]
#[test]
fn diesel_diagnostic_capture_rejects_missing_or_multiple_conditions() {
    for code in [1264, 1265] {
        assert_eq!(
            single_diesel_warning(1, Some(code)).unwrap(),
            vec![code as u32]
        );
    }
    for count in [0, 2, 64] {
        assert!(single_diesel_warning(count, Some(1264)).is_err());
    }
    assert!(single_diesel_warning(1, None).is_err());
    assert!(single_diesel_warning(1, Some(u64::MAX)).is_err());
}

fn exact_text_value(text: &str) -> &str {
    // SQL arithmetic may choose a different result scale for an exact literal
    // versus a derived parameter type. Compare mathematical fixed-point values
    // without using production Decimal parse or lossy floating-point conversion.
    let trimmed = if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.')
    } else {
        text
    };
    if trimmed == "-0" { "0" } else { trimmed }
}

// Independent exact decimal/scientific normalization for observation reports.
// The tuple denotes (-1)^negative * coefficient * 10^exponent. No floating point
// or bidecimal parse/Display is used; irrelevant leading/trailing zeros vanish.
fn observed_decimal_value(text: &str) -> Result<(bool, String, i64)> {
    let (negative, unsigned) = if let Some(rest) = text.strip_prefix('-') {
        (true, rest)
    } else {
        (false, text.strip_prefix('+').unwrap_or(text))
    };
    let (mantissa, explicit) = if let Some((m, e)) = unsigned.split_once(['e', 'E']) {
        (m, e.parse::<i64>()?)
    } else {
        (unsigned, 0)
    };
    let mut digits = String::new();
    let mut after_point = false;
    let mut fractional = 0_i64;
    for byte in mantissa.bytes() {
        if byte.is_ascii_digit() {
            digits.push(char::from(byte));
            if after_point {
                fractional += 1;
            }
        } else if byte == b'.' && !after_point {
            after_point = true;
        } else {
            return Err("invalid observed numeric text".into());
        }
    }
    if digits.is_empty() {
        return Err("observed numeric text has no digits".into());
    }
    let significant = digits.trim_start_matches('0');
    if significant.is_empty() {
        return Ok((false, "0".to_owned(), 0));
    }
    let coefficient = significant.trim_end_matches('0');
    let removed = i64::try_from(significant.len() - coefficient.len())?;
    let exponent = explicit
        .checked_sub(fractional)
        .and_then(|v| v.checked_add(removed))
        .ok_or("observed exponent overflow")?;
    Ok((negative, coefficient.to_owned(), exponent))
}
#[test]
fn observation_decimal_scientific_normalization() -> Result<()> {
    for (a, b) in [
        ("123.4500", "1.2345e2"),
        ("0.1000", "1E-1"),
        ("-0.0", "0e100"),
        ("+00012.00", "12"),
        ("10000000000000000000", "1e19"),
        ("-.0100", "-1e-2"),
    ] {
        assert_eq!(observed_decimal_value(a)?, observed_decimal_value(b)?);
    }
    for (a, b) in [
        ("12345678901234567890.1", "1.2345678901234567e19"),
        ("0.1000000000000000001", "0.1"),
        ("1.01", "1.0100001"),
    ] {
        assert_ne!(observed_decimal_value(a)?, observed_decimal_value(b)?);
    }
    assert_eq!(
        observed_decimal_value("-00123.4500")?,
        (true, "12345".to_owned(), -2)
    );
    for invalid in [
        "",
        ".",
        "1.2.3",
        "1e",
        "1e2e3",
        "NaN",
        "Infinity",
        " 1",
        "10e9223372036854775807",
    ] {
        assert!(observed_decimal_value(invalid).is_err(), "{invalid}");
    }
    Ok(())
}

fn assert_decode_error(error: Error) {
    // Reject connection/query/type errors masquerading as expected precision
    // failures. Diesel's source() skips its boxed error itself, so inspect
    // DeserializationError explicitly before following the normal source chain.
    // mysql_common exposes FromValueError with the original Value instead.
    #[cfg(feature = "mysql-sync")]
    if error.downcast_ref::<mysql::FromValueError>().is_some() {
        return;
    }
    #[cfg(feature = "mysql-async")]
    if error
        .downcast_ref::<mysql_async::FromValueError>()
        .is_some()
    {
        return;
    }
    let mut cause: &(dyn std::error::Error + 'static) = error.as_ref();
    loop {
        if let Some(value) = cause.downcast_ref::<bidecimal::Error>() {
            assert!(
                matches!(
                    value,
                    bidecimal::Error::Inexact | bidecimal::Error::Overflow
                ),
                "unexpected codec error: {value}"
            );
            return;
        }
        #[cfg(feature = "mysql-diesel")]
        if let Some(diesel::result::Error::DeserializationError(inner)) =
            cause.downcast_ref::<diesel::result::Error>()
        {
            cause = inner.as_ref();
            continue;
        }
        match cause.source() {
            Some(next) => cause = next,
            None => break,
        }
    }
    panic!("expected codec precision/range decode error, got {error:?}");
}

#[cfg(feature = "mysql-diesel")]
#[test]
fn diesel_decode_error_wrapper_preserves_typed_classification() {
    use diesel::result::Error as DieselError;
    for codec in [bidecimal::Error::Inexact, bidecimal::Error::Overflow] {
        let wrapped = DieselError::DeserializationError(Box::new(codec));
        // Diesel 2.3.14 delegates to inner.source(), dropping the leaf from
        // ordinary source traversal, but retains it in the public enum.
        assert!(std::error::Error::source(&wrapped).is_none());
        assert_decode_error(Box::new(wrapped));
        let contextual = diesel_query_error(
            "read128",
            "SELECT v FROM bid_decimal",
            DieselError::DeserializationError(Box::new(codec)),
        );
        assert!(contextual.to_string().contains("read128"));
        assert!(contextual.to_string().contains("SELECT v FROM bid_decimal"));
        assert_decode_error(contextual);
    }
    for rejected in [
        DieselError::DeserializationError(Box::new(bidecimal::Error::InvalidInput)),
        DieselError::QueryBuilderError(Box::new(bidecimal::Error::Inexact)),
        DieselError::SerializationError(Box::new(bidecimal::Error::Inexact)),
        DieselError::NotFound,
    ] {
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                assert_decode_error(Box::new(rejected));
            }))
            .is_err()
        );
    }
}

async fn suite(mut driver: Driver) -> Result<()> {
    let version = driver.text("SELECT VERSION() AS v").await?;
    assert!(
        version.starts_with("9.7.4"),
        "expected MySQL 9.7.4, got {version}"
    );
    eprintln!(
        "server={version}; version_with_update={}",
        driver.text("SELECT VERSION_WITH_UPDATE() AS v").await?
    );
    driver
        .execute("SET SESSION sql_mode='STRICT_ALL_TABLES,NO_ENGINE_SUBSTITUTION'")
        .await?;
    eprintln!("sql_mode={}", driver.text("SELECT @@sql_mode AS v").await?);
    driver
        .execute("CREATE TEMPORARY TABLE bid_decimal (v DECIMAL(65,30) NULL)")
        .await?;
    macro_rules! widths {
        ($ty:ty, $insert:ident, $read:ident, $long:literal) => {{
            for source in [
                "1.50",
                "0",
                "-0",
                "-123.4500",
                $long,
                "0.000000000000000000000000000001",
            ] {
                let value: $ty = source.parse()?;
                driver.execute("DELETE FROM bid_decimal").await?;
                driver
                    .$insert("INSERT INTO bid_decimal(v) VALUES (?)", Some(value))
                    .await?;
                for prepared in [false, true] {
                    let got = driver
                        .$read("SELECT v FROM bid_decimal", prepared)
                        .await?
                        .unwrap();
                    assert_eq!(
                        got,
                        value,
                        "{} source={source}, prepared={prepared}",
                        stringify!($ty)
                    );
                    if value == <$ty>::ZERO {
                        assert!(!got.is_sign_negative(), "DB normalizes negative zero");
                    }
                }
            }
            // Exact scale-31 cohort must normalize the actual bytes to scale 30.
            driver.execute("DELETE FROM bid_decimal").await?;
            let normalized = <$ty>::from_mantissa_and_scale(10, 31)?;
            driver
                .$insert("INSERT INTO bid_decimal(v) VALUES (?)", Some(normalized))
                .await?;
            assert_eq!(
                driver
                    .$read("SELECT v FROM bid_decimal", true)
                    .await?
                    .unwrap(),
                normalized
            );
            driver.execute("DELETE FROM bid_decimal").await?;
            driver
                .$insert("INSERT INTO bid_decimal(v) VALUES (?)", None)
                .await
                .inspect_err(|error| {
                    eprintln!("width={} source=NULL stage=insert: {error:?}", stringify!($ty));
                })?;
            for prepared in [false, true] {
                let got = driver
                    .$read("SELECT v FROM bid_decimal", prepared)
                    .await
                    .inspect_err(|error| {
                        eprintln!(
                            "width={} source=NULL stage=read prepared={prepared}: {error:?}",
                            stringify!($ty)
                        );
                    })?;
                assert!(
                    got.is_none(),
                    "width={} source=NULL stage=read prepared={prepared}: expected None, got {got:?}",
                    stringify!($ty)
                );
            }
            assert_decode_error(
                driver
                    .$read(
                        "SELECT CAST('12345678901234567890123456789012345' AS DECIMAL(65,0)) AS v",
                        true,
                    )
                    .await
                    .expect_err("overprecision decimal must fail decode"),
            );
        }};
    }
    widths!(Decimal32, insert32, read32, "1234567");
    widths!(Decimal64, insert64, read64, "1234567890123456");
    widths!(
        Decimal128,
        insert128,
        read128,
        "1234567890123456789012345678901234"
    );
    driver.rollback().await?;
    // High integer magnitude, tiny scale, and exact scale/cohort normalization.
    driver
        .execute("CREATE TEMPORARY TABLE bid_wide(v DECIMAL(65,0))")
        .await?;
    let high: Decimal128 = "1e64".parse()?;
    driver
        .insert128("INSERT INTO bid_wide VALUES (?)", Some(high))
        .await?;
    assert_eq!(
        driver
            .read128("SELECT v FROM bid_wide", true)
            .await?
            .unwrap(),
        high
    );
    assert_decode_error(
        driver
            .read128("SELECT CAST(REPEAT('9',65) AS DECIMAL(65,0)) AS v", true)
            .await
            .expect_err("65 significant digits must fail decode"),
    );
    assert_eq!(
        driver
            .read32("SELECT CAST('0.1' AS DECIMAL(1,1)) AS v", false)
            .await?
            .unwrap()
            .to_mantissa_and_scale()?,
        (1, 1)
    );
    assert_eq!(
        driver
            .read32("SELECT CAST('1234567.00' AS DECIMAL(9,2)) AS v", true)
            .await?
            .unwrap()
            .to_mantissa_and_scale()?,
        (1234567, 0)
    );
    // A 66-digit exact SQL value is not constructible in Decimal128. Server/codec
    // rejection must not be described as a successful Decimal128 bind fixture.
    assert_decode_error(
        driver
            .read128(
                "SELECT CAST(CONCAT(REPEAT('9',36),'.',REPEAT('1',30)) AS DECIMAL(65,30)) AS v",
                true,
            )
            .await
            .expect_err("oversized exact server decimal must fail decode"),
    );
    // Compare actual adapter parameters against independent exact SQL literals.
    // Values in each pair are indistinguishable after a binary64 conversion.
    for (a, b) in [
        ("12345678901234567890.1", "12345678901234567890.2"),
        ("0.1000000000000000001", "0.1000000000000000002"),
    ] {
        driver.execute("DELETE FROM bid_decimal").await?;
        driver
            .execute(&format!("INSERT INTO bid_decimal VALUES ({a})"))
            .await?;
        for expression in [
            "v = {}",
            "v < {}",
            "v IN ({})",
            "v BETWEEN 0 AND {}",
            "v + {}",
            "v * {}",
            "{} - v",
        ] {
            let oracle = format!(
                "SELECT CAST({} AS CHAR) AS v FROM bid_decimal",
                expression.replace("{}", b)
            );
            let bound = format!(
                "SELECT CAST({} AS CHAR) AS v FROM bid_decimal",
                expression.replace("{}", "?")
            );
            let expected = driver.text(&oracle).await?;
            let actual = driver.parameter_text(&bound, b.parse()?).await?;
            assert_eq!(
                exact_text_value(&actual),
                exact_text_value(&expected),
                "{bound}: {b}"
            );
        }
        let expected = driver
            .text(&format!(
                "SELECT CAST(CAST({b} AS DECIMAL(65,30)) AS CHAR) AS v"
            ))
            .await?;
        assert_eq!(
            driver
                .parameter_text(
                    "SELECT CAST(CAST(? AS DECIMAL(65,30)) AS CHAR) AS v",
                    b.parse()?
                )
                .await?,
            expected
        );
    }
    // Observations, not an assumption of exact server arithmetic: there is no
    // DECIMAL column or explicit DECIMAL cast in these parameter expressions.
    // Log the literal oracle and actual value even when the server uses double.
    // A successful query must not be reported as evidence of exact arithmetic.
    for input in ["12345678901234567890.1", "0.1000000000000000001"] {
        for expression in ["{} * 2", "{} + 0", "{}"] {
            let oracle_sql = format!(
                "SELECT CAST({} AS CHAR) AS v",
                expression.replace("{}", input)
            );
            let parameter_sql = format!(
                "SELECT CAST({} AS CHAR) AS v",
                expression.replace("{}", "?")
            );
            let expected = driver.text(&oracle_sql).await?;
            let actual = driver
                .parameter_text(&parameter_sql, input.parse()?)
                .await?;
            let exact_match =
                observed_decimal_value(&actual)? == observed_decimal_value(&expected)?;
            eprintln!(
                "CONTEXT_FREE driver={} sql={parameter_sql:?} input={input:?} decimal_literal={expected:?} parameter_result={actual:?} exact_match={exact_match}",
                driver.name()
            );
        }
    }
    driver
        .execute("CREATE TEMPORARY TABLE bid_round(v DECIMAL(5,2))")
        .await?;
    for (input, expected) in [("1.005", "1.01"), ("-1.005", "-1.01")] {
        driver.execute("DELETE FROM bid_round").await?;
        driver
            .insert128("INSERT INTO bid_round VALUES (?)", Some(input.parse()?))
            .await?;
        let warnings = driver.warning_codes().await?;
        assert!(
            warnings.contains(&1265),
            "missing rounding note: {warnings:?}"
        );
        assert_eq!(
            driver
                .text("SELECT CAST(v AS CHAR) AS v FROM bid_round")
                .await?,
            expected
        );
    }
    driver
        .execute("CREATE TEMPORARY TABLE bid_overflow(v DECIMAL(3,2))")
        .await?;
    for (input, clipped) in [("9.995", "9.99"), ("-9.995", "-9.99")] {
        driver
            .execute("SET SESSION sql_mode='STRICT_ALL_TABLES,NO_ENGINE_SUBSTITUTION'")
            .await?;
        assert!(
            driver
                .insert128("INSERT INTO bid_overflow VALUES (?)", Some(input.parse()?))
                .await
                .is_err()
        );
        let strict_warnings = driver.warning_codes().await?;
        assert!(
            strict_warnings.contains(&1264),
            "expected strict overflow code 1264: {strict_warnings:?}"
        );
        driver
            .execute("SET SESSION sql_mode='NO_ENGINE_SUBSTITUTION'")
            .await?;
        driver.execute("DELETE FROM bid_overflow").await?;
        driver
            .insert128("INSERT INTO bid_overflow VALUES (?)", Some(input.parse()?))
            .await?;
        let warnings = driver.warning_codes().await?;
        assert!(
            warnings.contains(&1264),
            "missing range warning: {warnings:?}"
        );
        assert_eq!(
            driver
                .text("SELECT CAST(v AS CHAR) AS v FROM bid_overflow")
                .await?,
            clipped
        );
    }
    Ok(())
}
fn url() -> Result<String> {
    std::env::var("BIDECIMAL_TEST_MYSQL_URL")
        .map_err(|_| "BIDECIMAL_TEST_MYSQL_URL is required; live tests cannot be skipped".into())
}
#[cfg(feature = "mysql-sqlx")]
#[tokio::test(flavor = "current_thread")]
async fn sqlx_mysql() -> Result<()> {
    suite(Driver::Sqlx(sqlx::MySqlConnection::connect(&url()?).await?)).await
}
#[cfg(feature = "mysql-diesel")]
#[tokio::test(flavor = "current_thread")]
async fn diesel_mysql() -> Result<()> {
    suite(Driver::Diesel(diesel::MysqlConnection::establish(&url()?)?)).await
}
#[cfg(feature = "mysql-sync")]
#[tokio::test(flavor = "current_thread")]
async fn mysql_sync() -> Result<()> {
    suite(Driver::Sync(mysql::Conn::new(mysql::Opts::from_url(
        &url()?,
    )?)?))
    .await
}
#[cfg(feature = "mysql-async")]
#[tokio::test(flavor = "current_thread")]
async fn mysql_async() -> Result<()> {
    suite(Driver::Async(
        mysql_async::Conn::new(mysql_async::Opts::from_url(&url()?)?).await?,
    ))
    .await
}
