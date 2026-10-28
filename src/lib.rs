//! Fixed-width IEEE 754-2019 decimal types with exact or explicitly rounded
//! results.
//!
//! | Type | Storage | Significant digits | Exponent q (coefficient × 10^q) |
//! |---|---|---|---|
//! | [`Decimal32`] | 4 bytes | 7 | −101 … 90 |
//! | [`Decimal64`] | 8 bytes | 16 | −398 … 369 |
//! | [`Decimal128`] | 16 bytes | 34 | −6176 … 6111 |
//!
//! A value is the BID encoding of the IEEE 754-2019 interchange format of its
//! width. Every finite value is supported: zeros of either sign and every
//! representation (cohort member) of every finite number. So are both
//! infinities and quiet and signaling NaNs, with the IEEE 754-2019 results
//! for operations on them and no status flags.
//! [`OrderedDecimal`] gives `Eq`, `Ord`, and `Hash` for use as a key, as
//! `OrderedFloat` does for `f64`.
//!
//! # Results
//!
//! Every operation returns the exact result, a result rounded in a direction
//! the caller named, an IEEE 754-2019 infinity or NaN, or an [`Error`]. There
//! is no default rounding. Division by zero gives an infinity, or NaN for
//! `0 / 0`, also with finite operands.
//!
//! - Addition, subtraction, multiplication, division, narrowing conversion,
//!   and parsing have a form without a direction and a `_rounded` form that
//!   takes a [`Rounding`].
//! - Without a direction, a result whose exact magnitude exceeds the largest
//!   finite value of the result type is [`Error::Overflow`]; any other result
//!   that is not exactly representable is [`Error::Inexact`].
//! - With a direction, a result that is not exactly representable is rounded
//!   once in that direction and uses the least possible exponent. When the
//!   result, rounded in that direction as if the exponent range were
//!   unbounded, exceeds the largest finite value of the result type in
//!   magnitude, it is the IEEE 754-2019 overflow result for that direction:
//!   an infinity or the largest finite value of the result's sign.
//! - Quantize and rounding to a multiple always take a [`Rounding`] and round
//!   once in that direction. Their result exponent and errors are documented
//!   on each method.
//! - An exact result uses the preferred exponent of the operation, or the
//!   representable exponent closest to it.
//! - The sign of a zero result follows IEEE 754-2019; where that sign depends
//!   on the rounding direction, a call without a direction gives +0.
//! - No operation panics; failures are reported through [`Result`].
//!
//! # Example
//!
//! ```
//! use bidecimal::{Decimal32, Decimal64, Decimal128, Error, Rounding};
//!
//! let a = Decimal32::parse("1.53")?;
//! let b = Decimal64::parse("2.5")?;
//! let product = Decimal128::mul(a, b)?;
//! assert_eq!(product.to_string(), "3.825");
//!
//! let step = Decimal32::parse("0.05")?;
//! let floor = a.round_to_multiple(step, Rounding::TowardNegative)?;
//! assert_eq!(floor.to_string(), "1.50");
//!
//! assert_eq!(Decimal32::parse("1.23456789"), Err(Error::Inexact));
//! # Ok::<(), Error>(())
//! ```

#![no_std]
#![cfg_attr(not(verus_keep_ghost), forbid(unsafe_code))]
#![cfg_attr(verus_keep_ghost, feature(proc_macro_hygiene))]
#![deny(missing_docs)]

#[cfg(verus_keep_ghost)]
#[allow(unused_imports)]
use vstd::prelude::*;

mod display;
mod error;
mod kernel;
mod layout;
mod operand;
mod ordered;
mod parse;
mod rounding;
mod tables;
mod types;
#[cfg(verus_keep_ghost)]
mod verus_model;
#[cfg(verus_keep_ghost)]
mod verus_std;

pub use error::Error;
pub use operand::Operand;
pub use ordered::OrderedDecimal;
pub use rounding::Rounding;
pub use types::{Decimal32, Decimal64, Decimal128, Stripped};

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod addsub_tests;
#[cfg(test)]
mod bits_tests;
#[cfg(test)]
mod compare_tests;
#[cfg(test)]
mod convert_tests;
#[cfg(test)]
mod div_tests;
#[cfg(test)]
mod fit_tests;
#[cfg(test)]
mod format_tests;
#[cfg(test)]
mod kernel_tests;
#[cfg(kani)]
mod layout_proofs;
#[cfg(test)]
mod mul_tests;
#[cfg(test)]
mod multiple_tests;
#[cfg(test)]
mod parse_tests;
#[cfg(test)]
mod quantize_tests;
#[cfg(test)]
mod refbid;
#[cfg(test)]
mod special_tests;
#[cfg(test)]
mod test_support;
