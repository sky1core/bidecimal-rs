//! MySQL client-only conversion costs. No server or network is involved.
//! Cold buffers include allocation; warm buffers reuse capacity. Common's
//! Value conversion necessarily owns a Vec. Its input setup is excluded via
//! iter_batched on both sides of owned Value receive comparisons. The practical
//! baseline converts the same driver's Value to String, then parses the Decimal;
//! String destruction is timed. Prepared-string/borrowed parsing and custom
//! policy references are diagnostics, not substitutes for String receive.
//! SQLx public Arguments::add is measured separately from direct Encode.
//! No SQLx decode benchmark is claimed: public raw-value construction is absent.
use bidecimal::{Decimal32, Decimal64, Decimal128};
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use diesel_2::{
    mysql::Mysql,
    query_builder::{BindCollector, bind_collector::RawBytesBindCollector},
    sql_types::{Numeric, Text},
};
use mysql_common_0_37::value::{
    Value,
    convert::{FromValue, FromValueError},
};
use sqlx_0_9::{Arguments, Encode, MySql, mysql::MySqlArguments};
use std::{hint::black_box, time::Duration};

fn database(c: &mut Criterion) {
    // Validate every fixture and baseline before registering any timing.
    database_matrix(c, true);
    database_matrix(c, false);
}

fn database_matrix(c: &mut Criterion, verify_only: bool) {
    macro_rules! case {
        ($d:ty, $name:literal, $text:literal) => {{
            let value = <$d>::parse($text).unwrap();
            let mut wire = Vec::new();
            let _ = <$d as Encode<MySql>>::encode_by_ref(&value, &mut wire).unwrap();
            assert_eq!(&wire[1..], $text.as_bytes());
            assert_eq!(usize::from(wire[0]), $text.len());
            let decoded = <$d>::from_value_opt(Value::Bytes($text.as_bytes().to_vec())).unwrap();
            assert_eq!(decoded.to_bits(), value.to_bits());
            assert_eq!(value.to_string(), $text);
            let received_string =
                String::from_value_opt(Value::Bytes($text.as_bytes().to_vec())).unwrap();
            assert_eq!(received_string, $text);
            assert_eq!(
                <$d>::parse(&received_string).unwrap().to_bits(),
                value.to_bits()
            );
            assert_eq!(
                Value::try_from(value).unwrap(),
                Value::from(value.to_string())
            );
            let mut string_wire = Vec::new();
            let _ = <String as Encode<MySql>>::encode_by_ref(&received_string, &mut string_wire)
                .unwrap();
            assert_eq!(string_wire, wire);
            let mut args = MySqlArguments::default();
            args.add(value).unwrap();
            let mut string_args = MySqlArguments::default();
            string_args.add(value.to_string()).unwrap();
            assert_eq!(args.len(), 1);
            assert_eq!(string_args.len(), 1);
            // Existing public Debug exposes payloads; no private fixture hook.
            let payload = format!("values: {wire:?},");
            assert!(format!("{args:?}").contains(&payload));
            assert!(format!("{string_args:?}").contains(&payload));
            // Byte-owned reference: same Value enum and input allocation,
            // fixed-point grammar, UTF8, finite-value and rollback policies.
            // Integer variants retain the same exact policy, although timed
            // fixtures are Bytes. Prepared-string and borrowed baselines are diagnostics only.
            let integer_policy = |negative: bool, mut magnitude: u64| {
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
                <$d>::parse(core::str::from_utf8(&bytes[start..]).unwrap())
            };
            let bytes_policy = |original: Value| {
                let parsed = match &original {
                    Value::Bytes(bytes) => {
                        if bytes.contains(&b'e') || bytes.contains(&b'E') {
                            Err(bidecimal::Error::InvalidInput)
                        } else {
                            core::str::from_utf8(bytes)
                                .map_err(|_| bidecimal::Error::InvalidInput)
                                .and_then(<$d>::parse)
                                .and_then(|value| {
                                    if value.is_finite() {
                                        Ok(value)
                                    } else {
                                        Err(bidecimal::Error::InvalidInput)
                                    }
                                })
                        }
                    }
                    Value::Int(value) => integer_policy(*value < 0, value.unsigned_abs()),
                    Value::UInt(value) => integer_policy(false, *value),
                    _ => Err(bidecimal::Error::InvalidInput),
                };
                match parsed {
                    Ok(decimal) => Ok((decimal, original)),
                    Err(_) => Err(FromValueError(original)),
                }
            };
            // Equal successful output and unchanged failure ownership.
            let (reference, original) =
                bytes_policy(Value::Bytes($text.as_bytes().to_vec())).unwrap();
            assert_eq!(reference.to_bits(), value.to_bits());
            assert_eq!(original, Value::Bytes($text.as_bytes().to_vec()));
            for bytes in [b"NaN".as_slice(), b"1e0", b"1E0", b"1.2.3", b"\xff"] {
                let original = Value::Bytes(bytes.to_vec());
                let expected = original.clone();
                let result = bytes_policy(original);
                assert_eq!(result.unwrap_err().0, expected);
            }
            for original in [
                Value::Int(i64::MIN),
                Value::Int(0),
                Value::Int(i64::MAX),
                Value::UInt(u64::MAX),
                Value::Float(1.5),
                Value::Double(1.5),
                Value::NULL,
            ] {
                let actual = <$d>::from_value_opt(original.clone());
                let reference = bytes_policy(original).map(|(decimal, _)| decimal);
                match (actual, reference) {
                    (Ok(actual), Ok(reference)) => {
                        assert_eq!(actual.to_bits(), reference.to_bits())
                    }
                    (Err(actual), Err(reference)) => assert_eq!(actual.0, reference.0),
                    _ => panic!("policy disagreement"),
                }
            }
            let mut diesel_mysql = RawBytesBindCollector::<Mysql>::new();
            diesel_mysql
                .push_bound_value::<Numeric, _>(&value, &mut ())
                .unwrap();
            assert!(
                format!("{diesel_mysql:?}")
                    .contains(&format!("binds: {:?}", [Some($text.as_bytes().to_vec())]))
            );
            let mut diesel_mysql_string = RawBytesBindCollector::<Mysql>::new();
            diesel_mysql_string
                .push_bound_value::<Text, _>(&received_string, &mut ())
                .unwrap();
            assert!(
                format!("{diesel_mysql_string:?}")
                    .contains(&format!("binds: {:?}", [Some($text.as_bytes().to_vec())]))
            );
            if !verify_only {
                let mut group = c.benchmark_group($name);
                group.bench_function("diagnostic/display_new_string", |b| {
                    b.iter(|| black_box(black_box(value).to_string()))
                });
                group.bench_function("diagnostic/borrowed_parse", |b| {
                    b.iter(|| black_box(<$d>::parse(black_box($text)).unwrap()))
                });
                group.bench_function("sqlx_encode_warm", |b| {
                    let mut out = Vec::with_capacity(128);
                    b.iter(|| {
                        out.clear();
                        let _ = <$d as Encode<MySql>>::encode_by_ref(black_box(&value), &mut out)
                            .unwrap();
                        black_box(&out);
                    })
                });
                group.bench_function("sqlx_encode_cold", |b| {
                    b.iter(|| {
                        let mut out = Vec::new();
                        let _ = <$d as Encode<MySql>>::encode_by_ref(black_box(&value), &mut out)
                            .unwrap();
                        black_box(out)
                    })
                });
                group.bench_function("display_string_bind_warm", |b| {
                    let mut out = Vec::with_capacity(128);
                    b.iter(|| {
                        let text = black_box(value).to_string();
                        out.clear();
                        let _ = <String as Encode<MySql>>::encode_by_ref(&text, &mut out).unwrap();
                        black_box(&out);
                    })
                });
                group.bench_function("display_string_bind_cold", |b| {
                    b.iter(|| {
                        let text = black_box(value).to_string();
                        let mut out = Vec::new();
                        let _ = <String as Encode<MySql>>::encode_by_ref(&text, &mut out).unwrap();
                        black_box(out)
                    })
                });
                group.bench_function("common_encode_owned", |b| {
                    b.iter(|| black_box(Value::try_from(black_box(value)).unwrap()))
                });
                group.bench_function("common_decode_owned_input_prepared", |b| {
                    b.iter_batched(
                        || Value::Bytes($text.as_bytes().to_vec()),
                        |input| black_box(<$d>::from_value_opt(black_box(input)).unwrap()),
                        BatchSize::SmallInput,
                    )
                });
                group.bench_function("diagnostic/owned_string_parse_input_prepared", |b| {
                    b.iter_batched(
                        || String::from($text),
                        |input| black_box(<$d>::parse(black_box(input.as_str())).unwrap()),
                        BatchSize::SmallInput,
                    )
                });
                group.bench_function("diagnostic/value_bytes_policy_intermediate_commit", |b| {
                    b.iter_batched(
                        || Value::Bytes($text.as_bytes().to_vec()),
                        |input| {
                            let (decimal, original) = bytes_policy(black_box(input)).unwrap();
                            drop(original);
                            black_box(decimal)
                        },
                        BatchSize::SmallInput,
                    )
                });
                group.bench_function("diagnostic/value_bytes_utf8_parse_without_db_policy", |b| {
                    b.iter_batched(
                        || Value::Bytes($text.as_bytes().to_vec()),
                        |input| {
                            let input = black_box(input);
                            let Value::Bytes(bytes) = input else {
                                unreachable!()
                            };
                            black_box(<$d>::parse(core::str::from_utf8(&bytes).unwrap()).unwrap())
                        },
                        BatchSize::SmallInput,
                    )
                });
                group.bench_function("common_display_string_encode_owned", |b| {
                    b.iter(|| black_box(Value::from(black_box(value).to_string())))
                });
                // Same owned Value setup as adapter; driver String conversion and drop are timed.
                group.bench_function("common_string_receive_parse_owned_input_prepared", |b| {
                    b.iter_batched(
                        || Value::Bytes($text.as_bytes().to_vec()),
                        |input| {
                            let text = String::from_value_opt(black_box(input)).unwrap();
                            black_box(<$d>::parse(&text).unwrap())
                        },
                        BatchSize::SmallInput,
                    )
                });
                group.bench_function("sqlx_arguments_add_cold", |b| {
                    b.iter(|| {
                        let mut args = MySqlArguments::default();
                        args.add(black_box(value)).unwrap();
                        black_box(args)
                    })
                });
                group.bench_function("sqlx_display_string_arguments_add_cold", |b| {
                    b.iter(|| {
                        let text = black_box(value).to_string();
                        let mut args = MySqlArguments::default();
                        args.add(text).unwrap();
                        black_box(args)
                    })
                });
                group.bench_function("diesel_mysql_numeric_bind_collector", |b| {
                    b.iter(|| {
                        let mut collector = RawBytesBindCollector::<Mysql>::new();
                        collector
                            .push_bound_value::<Numeric, _>(black_box(&value), &mut ())
                            .unwrap();
                        black_box(collector)
                    })
                });
                group.bench_function("diesel_mysql_display_text_bind_collector", |b| {
                    b.iter(|| {
                        let text = black_box(value).to_string();
                        let mut collector = RawBytesBindCollector::<Mysql>::new();
                        collector
                            .push_bound_value::<Text, _>(&text, &mut ())
                            .unwrap();
                        black_box(collector)
                    })
                });
                group.finish();
            }
        }};
    }
    case!(Decimal32, "mysql/d32/short", "1.50");
    case!(Decimal32, "mysql/d32/long", "-1234.567");
    case!(Decimal64, "mysql/d64/short", "1.50");
    case!(Decimal64, "mysql/d64/long", "-12345678.90123456");
    case!(Decimal128, "mysql/d128/short", "1.50");
    case!(
        Decimal128,
        "mysql/d128/long",
        "-12345678901234567.89012345678901234"
    );
}
criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(30).warm_up_time(Duration::from_millis(100)).measurement_time(Duration::from_millis(250));
    targets = database
}
criterion_main!(benches);
