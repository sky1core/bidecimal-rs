use vstd::arithmetic::power2::pow2;
use vstd::prelude::*;

verus! {

pub assume_specification[ u64::carrying_mul ](a: u64, b: u64, c: u64) -> (r: (u64, u64))
    ensures
        r.1 as int == (a as int * b as int + c as int) / 0x1_0000_0000_0000_0000int,
        r.0 as int == (a as int * b as int + c as int) % 0x1_0000_0000_0000_0000int,
;

pub assume_specification[ u128::carrying_mul ](a: u128, b: u128, c: u128) -> (r: (u128, u128))
    ensures
        r.1 as int == (a as int * b as int + c as int) / 0x1_0000_0000_0000_0000_0000_0000_0000_0000int,
        r.0 as int == (a as int * b as int + c as int) % 0x1_0000_0000_0000_0000_0000_0000_0000_0000int,
;

pub assume_specification[ u64::bit_width ](x: u64) -> (r: u32)
    ensures
        r <= 64,
        x == 0 ==> r == 0,
        x != 0 ==> r >= 1 && pow2((r - 1) as nat) <= x && (x as int) < pow2(r as nat),
;

pub assume_specification[ u128::bit_width ](x: u128) -> (r: u32)
    ensures
        r <= 128,
        x == 0 ==> r == 0,
        x != 0 ==> r >= 1 && pow2((r - 1) as nat) <= x && (x as int) < pow2(r as nat),
;

pub assume_specification[ <u32 as core::convert::From<bool>>::from ](b: bool) -> (r: u32)
    ensures
        r == if b { 1u32 } else { 0u32 },
;

pub assume_specification[ i128::unsigned_abs ](x: i128) -> (r: u128)
    ensures
        r as int == if x < 0 { -(x as int) } else { x as int },
;

pub assume_specification[ u128::trailing_zeros ](x: u128) -> (r: u32)
    ensures
        x == 0 ==> r == 128,
        x != 0 ==> r < 128 && (x as int) % (pow2(r as nat) as int) == 0 && ((x as int) / (pow2(r as nat) as int)) % 2 == 1,
;
}
