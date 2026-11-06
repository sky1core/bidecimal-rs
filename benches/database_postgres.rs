//! Client-only PostgreSQL conversion and production Diesel bind-collector costs.
//! A text parameter needs an explicit server-side cast to NUMERIC: its wire
//! payload is not semantically interchangeable with binary NUMERIC. No server,
//! network, SQL cast, native connection, or allocation-hook count is measured.
//! Practical decode compares postgres-types NUMERIC receive with the same
//! driver's TEXT-to-String receive plus Decimal::parse, including String drop.
//! Preexisting borrowed text parsing is a separately labeled diagnostic.
//! SQLx Arguments::add is distinct from direct Encode. SQLx and Diesel decode
//! are not claimed here: no public raw-value constructors are enabled or added.
use bidecimal::{Decimal32, Decimal64, Decimal128};
use bytes::BytesMut;
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use diesel_2::{
    pg::{Pg, PgMetadataLookup, PgTypeMetadata},
    query_builder::{BindCollector, bind_collector::RawBytesBindCollector},
    sql_types::{Numeric, Text},
};
use postgres_types_0_2::{FromSql, ToSql, Type};
use sqlx_0_9::{
    Arguments, Encode, Postgres,
    postgres::{PgArgumentBuffer, PgArguments},
};
use std::{hint::black_box, time::Duration};

struct BuiltinOnly;
impl PgMetadataLookup for BuiltinOnly {
    fn lookup_type(&mut self, _: &str, _: Option<&str>) -> PgTypeMetadata {
        panic!("built-in Numeric/Text must use static metadata, not a connection lookup")
    }
}

// Independent setup-only reference from the literal fixed-point fixture.
// Production parse/Display/binary encode do not supply expected wire bytes.
fn expected_numeric(text: &str) -> Vec<u8> {
    let negative = text.starts_with('-');
    let text = text.strip_prefix('-').unwrap_or(text);
    let (integer, fraction) = text.split_once('.').unwrap_or((text, ""));
    let left = (4 - integer.len() % 4) % 4;
    let right = (4 - fraction.len() % 4) % 4;
    let digits = format!(
        "{}{}{}{}",
        "0".repeat(left),
        integer,
        fraction,
        "0".repeat(right)
    );
    let mut groups: Vec<u16> = digits
        .as_bytes()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|s| s.iter().fold(0_u16, |n, b| n * 10 + u16::from(b - b'0')))
        .collect();
    let mut weight = ((integer.len() + left) / 4) as i16 - 1;
    while groups.first() == Some(&0) {
        groups.remove(0);
        weight -= 1;
    }
    while groups.last() == Some(&0) {
        groups.pop();
    }
    if groups.is_empty() {
        weight = 0;
    }
    let sign: u16 = if negative && !groups.is_empty() {
        0x4000
    } else {
        0
    };
    let mut out = Vec::new();
    out.extend_from_slice(&(groups.len() as u16).to_be_bytes());
    out.extend_from_slice(&weight.to_be_bytes());
    out.extend_from_slice(&sign.to_be_bytes());
    out.extend_from_slice(&(fraction.len() as u16).to_be_bytes());
    for group in groups {
        out.extend_from_slice(&group.to_be_bytes());
    }
    out
}

fn postgres(c: &mut Criterion) {
    // Validate every fixture and baseline before registering any timing.
    postgres_matrix(c, true);
    postgres_matrix(c, false);
}

fn postgres_matrix(c: &mut Criterion, verify_only: bool) {
    macro_rules! case {
        ($d:ty, $name:literal, $text:literal) => {{
            let value = <$d>::parse($text).unwrap();
            let expected = expected_numeric($text);
            let mut bytes = BytesMut::new();
            let _ = value.to_sql_checked(&Type::NUMERIC, &mut bytes).unwrap();
            assert_eq!(&bytes[..], expected);
            let back = <$d>::from_sql(&Type::NUMERIC, &expected).unwrap();
            assert_eq!(back.to_bits(), value.to_bits());
            let mut sqlx = PgArgumentBuffer::default();
            let _ = <$d as Encode<Postgres>>::encode_by_ref(&value, &mut sqlx).unwrap();
            assert_eq!(&sqlx[..], expected);
            assert_eq!(value.to_string(), $text);
            let received_string =
                <String as FromSql>::from_sql(&Type::TEXT, $text.as_bytes()).unwrap();
            assert_eq!(received_string, $text);
            assert_eq!(
                <$d>::parse(&received_string).unwrap().to_bits(),
                value.to_bits()
            );
            let received_str = <&str as FromSql>::from_sql(&Type::TEXT, $text.as_bytes()).unwrap();
            assert_eq!(
                <$d>::parse(received_str).unwrap().to_bits(),
                value.to_bits()
            );
            let mut string_bytes = BytesMut::new();
            let _ = received_string
                .to_sql_checked(&Type::TEXT, &mut string_bytes)
                .unwrap();
            assert_eq!(&string_bytes[..], $text.as_bytes());
            let mut string_sqlx = PgArgumentBuffer::default();
            let _ = <String as Encode<Postgres>>::encode_by_ref(&received_string, &mut string_sqlx)
                .unwrap();
            assert_eq!(&string_sqlx[..], $text.as_bytes());
            let mut args = PgArguments::default();
            args.add(value).unwrap();
            let mut string_args = PgArguments::default();
            string_args.add(value.to_string()).unwrap();
            assert_eq!(args.len(), 1);
            assert_eq!(string_args.len(), 1);
            let mut framed = (expected.len() as i32).to_be_bytes().to_vec();
            framed.extend_from_slice(&expected);
            let mut text_framed = ($text.len() as i32).to_be_bytes().to_vec();
            text_framed.extend_from_slice($text.as_bytes());
            assert!(format!("{args:?}").contains(&format!("buffer: {framed:?},")));
            assert!(format!("{string_args:?}").contains(&format!("buffer: {text_framed:?},")));
            let mut diesel_pg = RawBytesBindCollector::<Pg>::new();
            diesel_pg
                .push_bound_value::<Numeric, _>(&value, &mut BuiltinOnly)
                .unwrap();
            assert!(
                format!("{diesel_pg:?}")
                    .contains(&format!("binds: {:?}", [Some(expected.clone())]))
            );
            let mut diesel_pg_string = RawBytesBindCollector::<Pg>::new();
            diesel_pg_string
                .push_bound_value::<Text, _>(&received_string, &mut BuiltinOnly)
                .unwrap();
            assert!(
                format!("{diesel_pg_string:?}")
                    .contains(&format!("binds: {:?}", [Some($text.as_bytes().to_vec())]))
            );
            if !verify_only {
                let mut group = c.benchmark_group($name);
                group.bench_function("diagnostic/display_new_string", |b| {
                    b.iter(|| black_box(black_box(value).to_string()))
                });
                group.bench_function("diagnostic/borrowed_text_parse", |b| {
                    b.iter(|| black_box(<$d>::parse(black_box($text)).unwrap()))
                });
                group.bench_function("pg_types_decode_borrowed_binary", |b| {
                    b.iter(|| {
                        black_box(<$d>::from_sql(&Type::NUMERIC, black_box(&expected)).unwrap())
                    })
                });
                group.bench_function("pg_types_encode_warm", |b| {
                    let mut out = BytesMut::with_capacity(128);
                    b.iter(|| {
                        out.clear();
                        let _ = black_box(&value)
                            .to_sql_checked(&Type::NUMERIC, &mut out)
                            .unwrap();
                        black_box(&out);
                    })
                });
                group.bench_function("pg_types_encode_cold", |b| {
                    b.iter(|| {
                        let mut out = BytesMut::new();
                        let _ = black_box(&value)
                            .to_sql_checked(&Type::NUMERIC, &mut out)
                            .unwrap();
                        black_box(out)
                    })
                });
                group.bench_function("diagnostic/pg_types_encode_forced_growth", |b| {
                    b.iter_batched(
                        || {
                            let mut out = BytesMut::with_capacity(4);
                            let cap = out.capacity();
                            out.resize(cap, 0xaa);
                            out
                        },
                        |mut out| {
                            let _ = black_box(&value)
                                .to_sql_checked(&Type::NUMERIC, &mut out)
                                .unwrap();
                            black_box(out)
                        },
                        BatchSize::SmallInput,
                    )
                });
                group.bench_function("sqlx_binary_encode_warm", |b| {
                    let mut out = PgArgumentBuffer::default();
                    out.reserve(128);
                    b.iter(|| {
                        out.clear();
                        let _ =
                            <$d as Encode<Postgres>>::encode_by_ref(black_box(&value), &mut out)
                                .unwrap();
                        black_box(&out);
                    })
                });
                group.bench_function("sqlx_binary_encode_cold", |b| {
                    b.iter(|| {
                        let mut out = PgArgumentBuffer::default();
                        let _ =
                            <$d as Encode<Postgres>>::encode_by_ref(black_box(&value), &mut out)
                                .unwrap();
                        black_box(out)
                    })
                });
                group.bench_function("diagnostic/sqlx_binary_encode_forced_growth", |b| {
                    b.iter_batched(
                        || {
                            let mut out = PgArgumentBuffer::default();
                            out.reserve(4);
                            let cap = out.capacity();
                            out.resize(cap, 0xaa);
                            out
                        },
                        |mut out| {
                            let _ = <$d as Encode<Postgres>>::encode_by_ref(
                                black_box(&value),
                                &mut out,
                            )
                            .unwrap();
                            black_box(out)
                        },
                        BatchSize::SmallInput,
                    )
                });
                group.bench_function("sqlx_display_string_text_parameter_warm", |b| {
                    let mut out = PgArgumentBuffer::default();
                    out.reserve(128);
                    b.iter(|| {
                        let text = black_box(value).to_string();
                        out.clear();
                        let _ =
                            <String as Encode<Postgres>>::encode_by_ref(&text, &mut out).unwrap();
                        black_box(&out);
                    })
                });
                group.bench_function("sqlx_display_string_text_parameter_cold", |b| {
                    b.iter(|| {
                        let text = black_box(value).to_string();
                        let mut out = PgArgumentBuffer::default();
                        let _ =
                            <String as Encode<Postgres>>::encode_by_ref(&text, &mut out).unwrap();
                        black_box(out)
                    })
                });
                group.bench_function("diesel_pg_numeric_bind_collector", |b| {
                    b.iter(|| {
                        let mut collector = RawBytesBindCollector::<Pg>::new();
                        collector
                            .push_bound_value::<Numeric, _>(black_box(&value), &mut BuiltinOnly)
                            .unwrap();
                        black_box(collector)
                    })
                });
                group.bench_function("diesel_pg_display_text_bind_collector", |b| {
                    b.iter(|| {
                        let text = black_box(value).to_string();
                        let mut collector = RawBytesBindCollector::<Pg>::new();
                        collector
                            .push_bound_value::<Text, _>(&text, &mut BuiltinOnly)
                            .unwrap();
                        black_box(collector)
                    })
                });
                group.bench_function("pg_types_string_receive_parse_text", |b| {
                    b.iter(|| {
                        let text =
                            <String as FromSql>::from_sql(&Type::TEXT, black_box($text.as_bytes()))
                                .unwrap();
                        black_box(<$d>::parse(&text).unwrap())
                    })
                });
                group.bench_function("diagnostic/pg_types_borrowed_str_receive_parse", |b| {
                    b.iter(|| {
                        let text =
                            <&str as FromSql>::from_sql(&Type::TEXT, black_box($text.as_bytes()))
                                .unwrap();
                        black_box(<$d>::parse(text).unwrap())
                    })
                });
                group.bench_function("pg_types_display_string_text_parameter_warm", |b| {
                    let mut out = BytesMut::with_capacity(128);
                    b.iter(|| {
                        let text = black_box(value).to_string();
                        out.clear();
                        let _ = text.to_sql_checked(&Type::TEXT, &mut out).unwrap();
                        black_box(&out);
                    })
                });
                group.bench_function("pg_types_display_string_text_parameter_cold", |b| {
                    b.iter(|| {
                        let text = black_box(value).to_string();
                        let mut out = BytesMut::new();
                        let _ = text.to_sql_checked(&Type::TEXT, &mut out).unwrap();
                        black_box(out)
                    })
                });
                group.bench_function("sqlx_arguments_add_cold", |b| {
                    b.iter(|| {
                        let mut args = PgArguments::default();
                        args.add(black_box(value)).unwrap();
                        black_box(args)
                    })
                });
                group.bench_function("sqlx_display_string_arguments_add_cold", |b| {
                    b.iter(|| {
                        let text = black_box(value).to_string();
                        let mut args = PgArguments::default();
                        args.add(text).unwrap();
                        black_box(args)
                    })
                });
                group.finish();
            }
        }};
    }
    case!(Decimal32, "postgres/d32/short", "1.50");
    case!(Decimal32, "postgres/d32/long", "-1234.567");
    case!(Decimal64, "postgres/d64/short", "1.50");
    case!(Decimal64, "postgres/d64/long", "-12345678.90123456");
    case!(Decimal128, "postgres/d128/short", "1.50");
    case!(
        Decimal128,
        "postgres/d128/long",
        "-12345678901234567.89012345678901234"
    );
}
criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(30).warm_up_time(Duration::from_millis(100)).measurement_time(Duration::from_millis(250));
    targets = postgres
}
criterion_main!(benches);
