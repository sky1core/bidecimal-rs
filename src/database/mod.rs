//! Feature-gated database conversion. The codecs use only `core` and borrowed
//! slices; ownership and error boxing are confined to driver adapters.
use crate::{Decimal32, Decimal64, Decimal128, Error};

#[cfg(feature = "diesel-2-mysql")]
mod diesel_mysql;
#[cfg(feature = "mysql_common-0_37")]
mod mysql_common;
#[cfg(feature = "sqlx-0_9-mysql")]
mod sqlx_mysql;

#[cfg(feature = "diesel-2-postgres")]
mod diesel_postgres;
#[cfg(any(
    feature = "sqlx-0_9-postgres",
    feature = "diesel-2-postgres",
    feature = "postgres-types-0_2"
))]
mod pg_numeric;
#[cfg(all(
    test,
    any(
        feature = "sqlx-0_9-postgres",
        feature = "diesel-2-postgres",
        feature = "postgres-types-0_2"
    )
))]
mod pg_tests;
#[cfg(feature = "postgres-types-0_2")]
mod postgres_types;
#[cfg(feature = "sqlx-0_9-postgres")]
mod sqlx_postgres;

pub(crate) trait DbDecimal: Copy {
    #[cfg(any(
        test,
        feature = "diesel-2-mysql",
        feature = "mysql_common-0_37",
        feature = "sqlx-0_9-postgres"
    ))]
    fn parse_db(text: &str) -> Result<Self, Error>;
    #[cfg(any(
        test,
        feature = "sqlx-0_9-mysql",
        feature = "diesel-2-mysql",
        feature = "mysql_common-0_37",
        feature = "sqlx-0_9-postgres"
    ))]
    fn parse_decimal_db(text: &str) -> Result<Self, Error>;
    fn parts(self) -> Result<(bool, u128, i32), Error>;
    #[cfg(any(
        test,
        feature = "sqlx-0_9-mysql",
        feature = "diesel-2-mysql",
        feature = "mysql_common-0_37",
        feature = "sqlx-0_9-postgres"
    ))]
    fn finite(self) -> bool;
    #[cfg(any(
        test,
        feature = "sqlx-0_9-mysql",
        feature = "diesel-2-mysql",
        feature = "mysql_common-0_37"
    ))]
    fn trim_db(self) -> Self;
    #[cfg(any(
        feature = "sqlx-0_9-postgres",
        feature = "diesel-2-postgres",
        feature = "postgres-types-0_2"
    ))]
    fn from_wire(
        neg: bool,
        coeff: u128,
        exp: i64,
        sticky: bool,
        preferred: i64,
    ) -> Result<Self, Error>;
    #[cfg(any(
        feature = "sqlx-0_9-postgres",
        feature = "diesel-2-postgres",
        feature = "postgres-types-0_2"
    ))]
    fn pg_special(self) -> Result<Option<u16>, Error>;
    #[cfg(any(
        feature = "sqlx-0_9-postgres",
        feature = "diesel-2-postgres",
        feature = "postgres-types-0_2"
    ))]
    fn special(sign: u16) -> Result<Self, Error>;
}
macro_rules! decimal {
    ($($d:ty => $width:ident, $encode:ident, $parse:path);+ $(;)?) => { $(
        impl DbDecimal for $d {
        #[cfg(any(test, feature = "diesel-2-mysql", feature = "mysql_common-0_37", feature = "sqlx-0_9-postgres"))]
    fn parse_db(text: &str) -> Result<Self, Error> { Self::parse(text) }
        #[cfg(any(test, feature = "sqlx-0_9-mysql", feature = "diesel-2-mysql", feature = "mysql_common-0_37", feature = "sqlx-0_9-postgres"))]
    fn parse_decimal_db(text: &str) -> Result<Self, Error> {
        $parse(text, None).map(Self::from_in_range)
    }
        #[cfg(any(test, feature = "sqlx-0_9-mysql", feature = "diesel-2-mysql", feature = "mysql_common-0_37", feature = "sqlx-0_9-postgres"))]
    fn finite(self) -> bool { self.is_finite() }
            #[cfg(any(test, feature = "sqlx-0_9-mysql", feature = "diesel-2-mysql", feature = "mysql_common-0_37"))]
    fn trim_db(self) -> Self { self.trim_fraction_zeros() }
            fn parts(self) -> Result<(bool, u128, i32), Error> {
                Ok((self.is_sign_negative(), self.coefficient()?.unsigned_abs() as u128, self.exponent()?))
            }
            #[cfg(any(feature = "sqlx-0_9-postgres", feature = "diesel-2-postgres", feature = "postgres-types-0_2"))]
    #[inline]
    fn from_wire(neg: bool, coeff: u128, exp: i64, sticky: bool, preferred: i64) -> Result<Self, Error> {
                // The preferred cohort is already exact and in range. Keep
                // the original fitter for every other case, including zeros
                // whose preferred exponent requires clamping.
                if !sticky && exp == preferred
                    && let Some(fitted) = crate::layout::InRange::<crate::layout::$width>::checked(
                        neg, coeff, i128::from(exp),
                    )
                {
                    return Ok(Self::from_in_range(fitted));
                }
                let fitted = crate::kernel::fit::<crate::layout::$width>(
                    crate::kernel::Wide { neg, coeff, exp, sticky }, preferred, None,
                )?;
                Ok(Self::from_in_range(fitted))
            }
            #[cfg(any(feature = "sqlx-0_9-postgres", feature = "diesel-2-postgres", feature = "postgres-types-0_2"))]
    fn pg_special(self) -> Result<Option<u16>, Error> {
                if self.is_finite() { return Ok(None); }
                if self.to_bits() == Self::NAN.to_bits() { return Ok(Some(0xc000)); }
                if self.is_nan() { return Err(Error::InvalidInput); }
                Ok(Some(if self.is_sign_negative() { 0xf000 } else { 0xd000 }))
            }
            #[cfg(any(feature = "sqlx-0_9-postgres", feature = "diesel-2-postgres", feature = "postgres-types-0_2"))]
    fn special(sign: u16) -> Result<Self, Error> {
                match sign {
                    0xc000 => Ok(Self::NAN),
                    0xd000 => Ok(Self::INFINITY),
                    0xf000 => Ok(Self::NEG_INFINITY),
                    _ => Err(Error::InvalidInput),
                }
            }
        }
    )+ };
}
decimal!(
    Decimal32 => W32, encode_32, crate::parse::parse_32_with_grammar::<false>;
    Decimal64 => W64, encode_64, crate::parse::parse_64_with_grammar::<false>;
    Decimal128 => W128, encode_128, crate::parse::parse_with_grammar::<crate::layout::W128, false>
);

/// Accept only database decimal ASCII, not the wider public parse grammar.
/// Syntax is checked to the end before exact conversion, including huge inputs.
#[cfg(any(
    test,
    feature = "diesel-2-mysql",
    feature = "mysql_common-0_37",
    feature = "sqlx-0_9-postgres"
))]
pub(crate) fn decode_text<D: DbDecimal>(bytes: &[u8], allow_pg_special: bool) -> Result<D, Error> {
    if allow_pg_special {
        match bytes {
            b"NaN" => return D::parse_db("NaN"),
            b"Infinity" => return D::parse_db("inf"),
            b"-Infinity" => return D::parse_db("-inf"),
            _ => {}
        }
    }
    // The DB-only grammar rejects exponent syntax within the existing parser
    // fallback. Checked UTF-8 and syntax-before-fit error precedence remain.
    decode_decimal_str(core::str::from_utf8(bytes).map_err(|_| Error::InvalidInput)?)
}

/// Shared strict decimal policy after UTF-8 validation; PostgreSQL specials
/// remain handled by `decode_text` before this finite-only path.
#[cfg(any(
    test,
    feature = "sqlx-0_9-mysql",
    feature = "diesel-2-mysql",
    feature = "mysql_common-0_37",
    feature = "sqlx-0_9-postgres"
))]
pub(crate) fn decode_decimal_str<D: DbDecimal>(text: &str) -> Result<D, Error> {
    let value = D::parse_decimal_db(text)?;
    if !value.finite() {
        return Err(Error::InvalidInput);
    }
    Ok(value)
}

#[cfg(any(
    test,
    feature = "sqlx-0_9-mysql",
    feature = "diesel-2-mysql",
    feature = "mysql_common-0_37"
))]
#[cfg(feature = "serde")]
use crate::serde_support::lane;
#[cfg(any(
    test,
    feature = "sqlx-0_9-mysql",
    feature = "diesel-2-mysql",
    feature = "mysql_common-0_37"
))]
#[cfg(not(feature = "serde"))]
#[path = "../serde_support/lane.rs"]
mod lane;

#[cfg(any(
    test,
    feature = "sqlx-0_9-mysql",
    feature = "diesel-2-mysql",
    feature = "mysql_common-0_37"
))]
#[derive(Debug)]
pub(crate) enum TextWriteError<E> {
    Codec(Error),
    Sink(E),
}
#[cfg(any(
    test,
    feature = "sqlx-0_9-mysql",
    feature = "diesel-2-mysql",
    feature = "mysql_common-0_37"
))]
impl<E: core::fmt::Display> core::fmt::Display for TextWriteError<E> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Codec(e) => e.fmt(f),
            Self::Sink(e) => e.fmt(f),
        }
    }
}
#[cfg(any(
    test,
    feature = "sqlx-0_9-mysql",
    feature = "diesel-2-mysql",
    feature = "mysql_common-0_37"
))]
impl<E: core::error::Error + 'static> core::error::Error for TextWriteError<E> {}
#[cfg(any(
    test,
    feature = "sqlx-0_9-mysql",
    feature = "diesel-2-mysql",
    feature = "mysql_common-0_37"
))]
impl From<TextWriteError<Error>> for Error {
    fn from(e: TextWriteError<Error>) -> Self {
        match e {
            TextWriteError::Codec(e) | TextWriteError::Sink(e) => e,
        }
    }
}

/// Range-check the existing coefficient/exponent before any formatting.
/// The existing serde short lane and Display Parts perform all text assembly.
#[cfg(any(
    test,
    feature = "sqlx-0_9-mysql",
    feature = "diesel-2-mysql",
    feature = "mysql_common-0_37"
))]
pub(crate) struct MysqlText<const WITH_LEN: bool = true> {
    value: crate::layout::Unpacked,
    #[cfg(any(test, feature = "sqlx-0_9-mysql", feature = "mysql_common-0_37"))]
    len: usize,
}
#[cfg(any(
    test,
    feature = "sqlx-0_9-mysql",
    feature = "diesel-2-mysql",
    feature = "mysql_common-0_37"
))]
#[cfg(any(test, feature = "sqlx-0_9-mysql", feature = "mysql_common-0_37"))]
impl MysqlText<true> {
    #[inline]
    pub(crate) fn new<D: DbDecimal>(value: D) -> Result<Self, Error> {
        Self::new_inner(value)
    }
    #[cfg(any(test, feature = "sqlx-0_9-mysql", feature = "mysql_common-0_37"))]
    pub(crate) fn len(&self) -> usize {
        self.len
    }
}
#[cfg(feature = "diesel-2-mysql")]
impl MysqlText<false> {
    #[inline]
    pub(crate) fn new_diesel<D: DbDecimal>(value: D) -> Result<Self, Error> {
        Self::new_inner(value)
    }
}
#[cfg(any(
    test,
    feature = "sqlx-0_9-mysql",
    feature = "diesel-2-mysql",
    feature = "mysql_common-0_37"
))]
impl<const WITH_LEN: bool> MysqlText<WITH_LEN> {
    #[inline]
    fn new_inner<D: DbDecimal>(mut value: D) -> Result<Self, Error> {
        let (mut neg, mut coeff, mut exp) = value.parts()?;
        let digits = |c: u128| -> i32 {
            let log = if let Ok(narrow) = u64::try_from(c) {
                narrow.checked_ilog10()
            } else {
                c.checked_ilog10()
            };
            log.unwrap_or(0) as i32 + 1
        };
        let fits = |c: u128, e: i32, n: i32| {
            let scale = (-e).max(0);
            scale <= 30 && (c == 0 || (n + e).max(0) + scale <= 65)
        };
        // Diesel never requests len. Every finite decimal coefficient has at
        // most 34 digits. For scale 0..=30, both SQL limits hold regardless
        // of coefficient, so no digit count is needed for this domain.
        let guaranteed_fit = !WITH_LEN && (-30..=0).contains(&exp);
        let mut n = if guaranteed_fit { 0 } else { digits(coeff) };
        if !guaranteed_fit && !fits(coeff, exp, n) {
            value = value.trim_db();
            (neg, coeff, exp) = value.parts()?;
            n = digits(coeff);
            if !fits(coeff, exp, n) {
                return Err(Error::Overflow);
            }
        }
        if coeff == 0 && exp > 0 {
            exp = 0;
        }
        #[cfg(any(test, feature = "sqlx-0_9-mysql", feature = "mysql_common-0_37"))]
        let n = n as usize;
        #[cfg(any(test, feature = "sqlx-0_9-mysql", feature = "mysql_common-0_37"))]
        let len = if WITH_LEN {
            usize::from(neg)
                + if exp >= 0 {
                    n + exp as usize
                } else if n > (-exp) as usize {
                    n + 1
                } else {
                    (-exp) as usize + 2
                }
        } else {
            0
        };
        Ok(Self {
            value: crate::layout::Unpacked { neg, coeff, exp },
            #[cfg(any(test, feature = "sqlx-0_9-mysql", feature = "mysql_common-0_37"))]
            len,
        })
    }
    #[cfg(any(test, feature = "sqlx-0_9-mysql", feature = "mysql_common-0_37"))]
    pub(crate) fn write<E>(
        &self,
        sink: impl FnMut(&[u8]) -> Result<(), E>,
    ) -> Result<(), TextWriteError<E>> {
        self.write_with::<E, false>(sink)
    }
    #[cfg(feature = "diesel-2-mysql")]
    pub(crate) fn write_contiguous<E>(
        &self,
        sink: impl FnMut(&[u8]) -> Result<(), E>,
    ) -> Result<(), TextWriteError<E>> {
        self.write_with::<E, true>(sink)
    }
    fn write_with<E, const CONTIGUOUS: bool>(
        &self,
        mut sink: impl FnMut(&[u8]) -> Result<(), E>,
    ) -> Result<(), TextWriteError<E>> {
        let v = self.value;
        if let Ok(coeff) = u64::try_from(v.coeff)
            && let Some(small) = lane::SmallValue::new(v.neg, coeff, v.exp)
        {
            use lane::SmallValue;
            return match small {
                SmallValue::Unsigned(n) => {
                    sink(n.format_into(&mut core::fmt::NumBuffer::new()).as_bytes())
                        .map_err(TextWriteError::Sink)
                }
                SmallValue::Signed(n) => {
                    sink(n.format_into(&mut core::fmt::NumBuffer::new()).as_bytes())
                        .map_err(TextWriteError::Sink)
                }
                SmallValue::NegativeZero => sink(b"-0").map_err(TextWriteError::Sink),
                SmallValue::Fraction { neg, coeff, exp } => {
                    let mut text = lane::SmallText::new();
                    let text = text
                        .bytes(neg, coeff, exp)
                        .map_err(|_| TextWriteError::Codec(Error::InvalidInput))?;
                    sink(text).map_err(TextWriteError::Sink)
                }
            };
        }
        struct Writer<'a, F, E> {
            sink: &'a mut F,
            error: Option<E>,
        }
        impl<F: FnMut(&[u8]) -> Result<(), E>, E> core::fmt::Write for Writer<'_, F, E> {
            fn write_str(&mut self, text: &str) -> core::fmt::Result {
                (self.sink)(text.as_bytes()).map_err(|e| {
                    self.error = Some(e);
                    core::fmt::Error
                })
            }
        }
        #[cfg(feature = "diesel-2-mysql")]
        if CONTIGUOUS {
            // Preserve the one-piece small lane. Only this fragmented fallback
            // is assembled through the same Parts emitter and Writer adapter.
            let mut bytes = [0_u8; 65 + 2];
            let mut remaining = bytes.as_mut_slice();
            {
                let mut buffer_sink = |part: &[u8]| -> Result<(), Error> {
                    let (dest, rest) = core::mem::take(&mut remaining)
                        .split_at_mut_checked(part.len())
                        .ok_or(Error::InvalidInput)?;
                    dest.copy_from_slice(part);
                    remaining = rest;
                    Ok(())
                };
                let mut writer = Writer {
                    sink: &mut buffer_sink,
                    error: None,
                };
                let mut digits = crate::display::Digits::new();
                digits
                    .parts(v, false)
                    .write_text(&mut writer)
                    .map_err(|_| TextWriteError::Codec(Error::InvalidInput))?;
            }
            let len = (65 + 2) - remaining.len();
            return sink(&bytes[..len]).map_err(TextWriteError::Sink);
        }
        let mut writer = Writer {
            sink: &mut sink,
            error: None,
        };
        let mut digits = crate::display::Digits::new();
        let result = digits.parts(v, false).write_text(&mut writer);
        match result {
            Ok(()) => Ok(()),
            Err(_) => Err(match writer.error {
                Some(e) => TextWriteError::Sink(e),
                None => TextWriteError::Codec(Error::InvalidInput),
            }),
        }
    }
}
#[cfg(test)]
mod tests;
