#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// Failure of an operation.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Error {
    /// The exact result is not representable in the result type, and the
    /// operation was called without a rounding direction; or a multiple
    /// computed by `round_to_multiple` needs more significant digits than the
    /// type has.
    Inexact,
    /// The magnitude of the result exceeds the largest finite value of the
    /// result type. Each operation documents the exact condition.
    Overflow,
    /// Rounding to a multiple was given a zero step.
    DivisionByZero,
    /// The input is not accepted: a string outside the parsing grammar, bits
    /// of a non-canonical encoding, a negative step, including −∞, for
    /// rounding to a multiple, or a quantize exponent outside the type's
    /// exponent range or at which the coefficient of the result needs more
    /// digits than the type has.
    InvalidInput,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::Inexact => "result is not exactly representable",
            Self::Overflow => "result exceeds the finite range",
            Self::DivisionByZero => "division by zero",
            Self::InvalidInput => "invalid input",
        })
    }
}

impl core::error::Error for Error {}
