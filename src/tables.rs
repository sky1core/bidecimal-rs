#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

#[cfg(verus_keep_ghost)]
use crate::verus_model::{recip_spec, recip64_spec};
#[cfg(verus_keep_ghost)]
use vstd::arithmetic::power::pow;
#[cfg(verus_keep_ghost)]
use vstd::arithmetic::power2::pow2;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_spec(
    ensures
        forall|i: int| 0 <= i < 39 ==> POW10[i] as int == pow(10, i as nat),
))]
const POW10: [u128; 39] = build_pow10();
#[cfg_attr(verus_keep_ghost, verus_spec(
    ensures
        forall|i: int| #![trigger RECIP[i]] 0 <= i < 38 ==> recip_spec(i + 1, RECIP[i].m as int, RECIP[i].post as int),
))]
const RECIP: [Recip; 38] = build_reciprocals();
#[cfg_attr(verus_keep_ghost, verus_spec(
    ensures
        forall|i: int| 0 <= i < 20 ==> POW10_64[i] as int == pow(10, i as nat),
))]
const POW10_64: [u64; 20] = build_pow10_64();
#[cfg_attr(verus_keep_ghost, verus_spec(
    ensures
        forall|i: int| #![trigger RECIP_64[i]] 0 <= i < 19 ==> recip64_spec(i + 1, RECIP_64[i].m as int, RECIP_64[i].post as int),
))]
const RECIP_64: [Recip64; 19] = build_reciprocals_64();

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy)]
pub(crate) struct Recip {
    pub(crate) m: u128,
    pub(crate) post: u32,
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy)]
pub(crate) struct Recip64 {
    pub(crate) m: u64,
    pub(crate) post: u32,
}

#[cfg_attr(verus_keep_ghost, verus_spec(table =>
    ensures
        forall|j: int| 0 <= j < 39 ==> table[j] as int == pow(10, j as nat),
))]
const fn build_pow10() -> [u128; 39] {
    #[cfg(verus_keep_ghost)]
    proof! { crate::verus_model::lemma_pow10_step(0); }
    fill_pow10([1; 39], 1)
}

#[expect(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    reason = "1 <= i < 39 indexes the 39-entry table; entry i - 1 is 10^(i-1) <= 10^37, so the product is at most 10^38 < 2^127"
)]
#[cfg_attr(verus_keep_ghost, verus_spec(ret =>
    requires
        1 <= i <= 39,
        forall|j: int| 0 <= j < i ==> table[j] as int == pow(10, j as nat),
    ensures
        forall|j: int| 0 <= j < 39 ==> ret[j] as int == pow(10, j as nat),
    decreases 39 - i,
))]
const fn fill_pow10(mut table: [u128; 39], i: usize) -> [u128; 39] {
    if i >= 39 {
        return table;
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::verus_model::lemma_pow10_step((i - 1) as nat);
        crate::verus_model::lemma_pow10_bounds((i - 1) as nat);
    }
    table[i] = table[i - 1] * 10;
    fill_pow10(table, i + 1)
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "remainder < d < 2^127, so remainder * 2 cannot overflow and doubled - d is taken only when doubled >= d; the final quotient is below 2^128 and every intermediate quotient * 2 + 1 is at most it; 2 <= steps, so half and steps - half are below steps"
)]
#[cfg_attr(verus_keep_ghost, verus_spec(ret =>
    requires
        1 < d < 0x8000_0000_0000_0000_0000_0000_0000_0000u128,
        remainder < d,
        (quotient as int * d as int + remainder as int) * pow2(steps as nat) < d as int * pow2(128),
    ensures
        ret.1 < d,
        ret.0 as int * d as int + ret.1 as int == (quotient as int * d as int + remainder as int) * pow2(steps as nat),
    decreases steps,
))]
const fn long_divide(d: u128, steps: u32, quotient: u128, remainder: u128) -> (u128, u128) {
    #[cfg(verus_keep_ghost)]
    proof! { vstd::arithmetic::power2::lemma2_to64(); }
    if steps == 0 {
        #[cfg(verus_keep_ghost)]
        proof! {
            let n = quotient as int * d as int + remainder as int;
            assert(n * pow2(steps as nat) == n) by (nonlinear_arith) requires pow2(steps as nat) == 1;
        }
        return (quotient, remainder);
    }
    if steps == 1 {
        #[cfg(verus_keep_ghost)]
        proof! { crate::verus_model::lemma_long_divide_step(d as int, 1, quotient as int, remainder as int); }
        let doubled = remainder * 2;
        return if doubled >= d {
            (quotient * 2 + 1, doubled - d)
        } else {
            (quotient * 2, doubled)
        };
    }
    let half = steps / 2;
    #[cfg(verus_keep_ghost)]
    proof! { crate::verus_model::lemma_long_divide_halves(steps as nat, half as nat, quotient as int * d as int + remainder as int); }
    let (quotient, remainder) = long_divide(d, half, quotient, remainder);
    long_divide(d, steps - half, quotient, remainder)
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "1 <= k <= 38; d = 10^k < 2^127 has bit width 4..=127, and d is not a power of two, so ceil(2^(127+width)/d) < 2^128 as asserted"
)]
#[cfg_attr(verus_keep_ghost, verus_spec(r =>
    requires
        1 <= k <= 38,
    ensures
        recip_spec(k as int, r.m as int, r.post as int),
))]
const fn build_reciprocal(k: u32) -> Recip {
    let d = pow10(k);
    #[cfg(verus_keep_ghost)]
    proof! { crate::verus_model::lemma_pow10_facts_128(k as nat); }
    let width = d.bit_width();
    #[cfg(verus_keep_ghost)]
    proof! { crate::verus_model::lemma_reciprocal_start(k as nat, d as int, width as nat); }
    let steps = 127 + width;
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(steps as nat == 127 + width as nat);
        let t = pow2(steps as nat) as int;
        assert((0 * d as int + 1) * t == t) by (nonlinear_arith);
    }
    let (quotient, remainder) = long_divide(d, steps, 0, 1);
    #[cfg(verus_keep_ghost)]
    proof! { crate::verus_model::lemma_reciprocal_entry(k as nat, d as int, width as nat, quotient as int, remainder as int); }
    let round_up = (remainder != 0) as u128;
    assert!(quotient < u128::MAX);
    Recip {
        m: quotient + round_up,
        post: width - 1,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(table =>
    ensures
        forall|i: int| #![trigger table[i]] 0 <= i < 38 ==> recip_spec(i + 1, table[i].m as int, table[i].post as int),
))]
const fn build_reciprocals() -> [Recip; 38] {
    fill_reciprocals([Recip { m: 0, post: 0 }; 38], 1)
}

#[expect(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    reason = "1 <= k <= 38, so k - 1 indexes the 38-entry table; k increments to at most 39"
)]
#[cfg_attr(verus_keep_ghost, verus_spec(ret =>
    requires
        1 <= k <= 39,
        forall|i: int| #![trigger table[i]] 0 <= i < k - 1 ==> recip_spec(i + 1, table[i].m as int, table[i].post as int),
    ensures
        forall|i: int| #![trigger ret[i]] 0 <= i < 38 ==> recip_spec(i + 1, ret[i].m as int, ret[i].post as int),
    decreases 39 - k,
))]
const fn fill_reciprocals(mut table: [Recip; 38], k: usize) -> [Recip; 38] {
    if k > 38 {
        return table;
    }
    table[k - 1] = build_reciprocal(k as u32);
    fill_reciprocals(table, k + 1)
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "64-bit limbs have products < 2^128; middle < 3*2^64; the high sum is floor(a*b/2^128) < 2^128; all shifts are 64"
)]
#[cfg_attr(verus_keep_ghost, verus_spec(ret =>
    ensures
        ret.0 as int + ret.1 as int * pow2(128) == a as int * b as int,
))]
const fn const_mul_128_to_256(a: u128, b: u128) -> (u128, u128) {
    let mask = u64::MAX as u128;
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::verus_model::lemma_u128_limbs(a);
        crate::verus_model::lemma_u128_limbs(b);
        crate::verus_model::lemma_mul_limbs(a as int, b as int);
    }
    let low_low = (a & mask) * (b & mask);
    let high_low = (a >> 64) * (b & mask);
    let low_high = (a & mask) * (b >> 64);
    let high_high = (a >> 64) * (b >> 64);
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::verus_model::lemma_u128_limbs(low_low);
        crate::verus_model::lemma_u128_limbs(high_low);
        crate::verus_model::lemma_u128_limbs(low_high);
    }
    let middle = (low_low >> 64) + (high_low & mask) + (low_high & mask);
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::verus_model::lemma_u128_limbs(middle);
        crate::verus_model::lemma_u128_join(low_low, middle);
    }
    let low = (low_low & mask) | ((middle & mask) << 64);
    let high = high_high + (high_low >> 64) + (low_high >> 64) + (middle >> 64);
    (low, high)
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "1 <= k <= 38, so post = bit_width(10^k) - 1 is in 3..=126; k increments to at most 39"
)]
#[cfg_attr(verus_keep_ghost, verus_spec(
    requires
        1 <= k <= 39,
    decreases 39 - k,
))]
const fn verify_reciprocals_from(k: u32) {
    if k > 38 {
        return;
    }
    let d = pow10(k);
    let r = recip(k);
    #[cfg(verus_keep_ghost)]
    proof! { crate::verus_model::lemma_pow10_facts_128(k as nat); }
    let width = d.bit_width();
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::verus_model::lemma_bit_width_is_post(d as int, width as nat, r.post as nat);
        crate::verus_model::lemma_u128_shl_one(r.post);
    }
    assert!(r.post == width - 1);
    let (low, high) = const_mul_128_to_256(r.m, d);
    #[cfg(verus_keep_ghost)]
    proof! { crate::verus_model::lemma_pow2_128(); crate::verus_model::lemma_verify_reciprocal(k as nat, r.m as int, r.post as nat, low as int, high as int); }
    assert!(high == 1_u128 << r.post);
    assert!(low < d);
    verify_reciprocals_from(k + 1)
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
const fn verify_reciprocals() {
    verify_reciprocals_from(1)
}

const _: () = verify_reciprocals();

#[cfg_attr(verus_keep_ghost, verus_spec(table =>
    ensures
        forall|j: int| 0 <= j < 20 ==> table[j] as int == pow(10, j as nat),
))]
const fn build_pow10_64() -> [u64; 20] {
    #[cfg(verus_keep_ghost)]
    proof! { crate::verus_model::lemma_pow10_step(0); }
    fill_pow10_64([1; 20], 1)
}

#[expect(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    reason = "1 <= i < 20 indexes the 20-entry table; entry i - 1 is 10^(i-1) <= 10^18, so the product is at most 10^19 < 2^64"
)]
#[cfg_attr(verus_keep_ghost, verus_spec(ret =>
    requires
        1 <= i <= 20,
        forall|j: int| 0 <= j < i ==> table[j] as int == pow(10, j as nat),
    ensures
        forall|j: int| 0 <= j < 20 ==> ret[j] as int == pow(10, j as nat),
    decreases 20 - i,
))]
const fn fill_pow10_64(mut table: [u64; 20], i: usize) -> [u64; 20] {
    if i >= 20 {
        return table;
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::verus_model::lemma_pow10_step((i - 1) as nat);
        crate::verus_model::lemma_pow10_bounds((i - 1) as nat);
    }
    table[i] = table[i - 1] * 10;
    fill_pow10_64(table, i + 1)
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "power <= d / 2 < 2^63 before doubling, so power * 2 and post + 1 cannot overflow"
)]
#[cfg_attr(verus_keep_ghost, verus_spec(ret =>
    requires
        power as int == vstd::arithmetic::power2::pow2(post as nat),
        1 <= power <= d,
        post <= 63,
    ensures
        ret.0 as int == vstd::arithmetic::power2::pow2(ret.1 as nat),
        1 <= ret.0 <= d,
        d < 2 * ret.0,
        ret.1 <= 63,
    decreases d - power,
))]
const fn floor_log2_64(d: u64, power: u64, post: u32) -> (u64, u32) {
    if power > d / 2 {
        return (power, post);
    }
    #[cfg(verus_keep_ghost)]
    proof! { crate::verus_model::lemma_double_power(power, post, d); }
    floor_log2_64(d, power * 2, post + 1)
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "1 <= k <= 19; power = 2^post <= d = 10^k < 2^64 is doubled only while 2*power <= d, so post <= 63 and power*2^64 < 2^128; d is not a power of two, so ceil(2^(64+post)/d) < 2^64 as asserted"
)]
#[cfg_attr(verus_keep_ghost, verus_spec(r =>
    requires
        1 <= k <= 19,
    ensures
        recip64_spec(k as int, r.m as int, r.post as int),
))]
const fn build_reciprocal_64(k: u32) -> Recip64 {
    let d = pow10_64(k);
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::verus_model::lemma_pow10_facts(k as nat);
        vstd::arithmetic::power2::lemma2_to64();
    }
    let (power, post) = floor_log2_64(d, 1, 0);
    let numerator = (power as u128) << 64;
    let d = d as u128;
    #[cfg(verus_keep_ghost)]
    proof! { crate::verus_model::lemma_reciprocal_64_entry(k, power, post, numerator, d); }
    let m = numerator / d + (!numerator.is_multiple_of(d)) as u128;
    #[cfg(verus_keep_ghost)]
    proof! { assert(1_u128 << 64 == 0x1_0000_0000_0000_0000u128) by (bit_vector); }
    assert!(m < (1_u128 << 64));
    Recip64 { m: m as u64, post }
}

#[cfg_attr(verus_keep_ghost, verus_spec(table =>
    ensures
        forall|i: int| #![trigger table[i]] 0 <= i < 19 ==> recip64_spec(i + 1, table[i].m as int, table[i].post as int),
))]
const fn build_reciprocals_64() -> [Recip64; 19] {
    fill_reciprocals_64([Recip64 { m: 0, post: 0 }; 19], 1)
}

#[expect(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    reason = "1 <= k <= 19, so k - 1 indexes the 19-entry table; k increments to at most 20"
)]
#[cfg_attr(verus_keep_ghost, verus_spec(ret =>
    requires
        1 <= k <= 20,
        forall|i: int| #![trigger table[i]] 0 <= i < k - 1 ==> recip64_spec(i + 1, table[i].m as int, table[i].post as int),
    ensures
        forall|i: int| #![trigger ret[i]] 0 <= i < 19 ==> recip64_spec(i + 1, ret[i].m as int, ret[i].post as int),
    decreases 20 - k,
))]
const fn fill_reciprocals_64(mut table: [Recip64; 19], k: usize) -> [Recip64; 19] {
    if k > 19 {
        return table;
    }
    table[k - 1] = build_reciprocal_64(k as u32);
    fill_reciprocals_64(table, k + 1)
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "1 <= k <= 19, so post = bit_width(10^k) - 1 is in 3..=63; m < 2^64 and d < 2^64 give m*d < 2^128; the product is asserted to be at least 2^(64+post); k increments to at most 20"
)]
#[cfg_attr(verus_keep_ghost, verus_spec(
    requires
        1 <= k <= 20,
    decreases 20 - k,
))]
const fn verify_reciprocals_64_from(k: u32) {
    if k > 19 {
        return;
    }
    let d = pow10_64(k);
    let r = recip_64(k);
    #[cfg(verus_keep_ghost)]
    proof! { crate::verus_model::lemma_pow10_facts(k as nat); }
    let width = d.bit_width();
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::verus_model::lemma_bit_width_is_post(d as int, width as nat, r.post as nat);
        crate::verus_model::lemma_u128_shl_one((64 + r.post) as u32);
        crate::verus_model::lemma_verify_reciprocal_64(k as nat, r.m as int, r.post as nat);
    }
    assert!(r.post == width - 1);
    let product = (r.m as u128) * (d as u128);
    let power = 1_u128 << (64 + r.post);
    assert!(product >= power);
    assert!(product - power < d as u128);
    verify_reciprocals_64_from(k + 1)
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
const fn verify_reciprocals_64() {
    verify_reciprocals_64_from(1)
}

const _: () = verify_reciprocals_64();

#[expect(
    clippy::indexing_slicing,
    reason = "callers establish k <= 38, within the 39-entry power table"
)]
#[cfg_attr(verus_keep_ghost, verus_spec(ret =>
    requires
        k < 39,
    ensures
        ret as int == pow(10, k as nat),
))]
pub(crate) const fn pow10(k: u32) -> u128 {
    POW10[k as usize]
}

#[expect(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    reason = "callers establish 1 <= k <= 38, so k - 1 is within the 38-entry reciprocal table"
)]
#[cfg_attr(verus_keep_ghost, verus_spec(ret =>
    requires
        1 <= k <= 38,
    ensures
        recip_spec(k as int, ret.m as int, ret.post as int),
))]
pub(crate) const fn recip(k: u32) -> Recip {
    RECIP[(k - 1) as usize]
}

#[expect(
    clippy::indexing_slicing,
    reason = "callers establish k <= 19, within the 20-entry power table"
)]
#[cfg_attr(verus_keep_ghost, verus_spec(ret =>
    requires
        k < 20,
    ensures
        ret as int == pow(10, k as nat),
))]
pub(crate) const fn pow10_64(k: u32) -> u64 {
    POW10_64[k as usize]
}

#[expect(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    reason = "callers establish 1 <= k <= 19, so k - 1 is within the 19-entry reciprocal table"
)]
#[cfg_attr(verus_keep_ghost, verus_spec(ret =>
    requires
        1 <= k <= 19,
    ensures
        recip64_spec(k as int, ret.m as int, ret.post as int),
))]
pub(crate) const fn recip_64(k: u32) -> Recip64 {
    RECIP_64[(k - 1) as usize]
}
