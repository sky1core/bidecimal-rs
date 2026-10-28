#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// Rounding direction for an operation that rounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(verus_keep_ghost, derive(Structural))]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub enum Rounding {
    /// Round toward negative infinity (IEEE 754-2019 roundTowardNegative,
    /// floor).
    TowardNegative,
    /// Round toward positive infinity (IEEE 754-2019 roundTowardPositive,
    /// ceiling).
    TowardPositive,
}
