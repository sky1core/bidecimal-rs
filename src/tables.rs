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

pub(crate) const BINARY_POW5: [u128; 56] = build_binary_pow5();
pub(crate) const F64_POW10: [f64; 23] = build_f64_pow10();
pub(crate) const F32_POW10: [f32; 11] = build_f32_pow10();

#[expect(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    reason = "i is in 1..56 and 5^55 < 2^128; the integer recurrence produces each exact entry"
)]
const fn build_binary_pow5() -> [u128; 56] {
    let mut table = [1; 56];
    let mut i = 1;
    while i < table.len() {
        table[i] = table[i - 1] * 5;
        i += 1;
    }
    table
}

#[expect(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    reason = "i<23 and 10^22 fits u128 and f64 exactly because 5^22<2^53; each entry comes from the existing integer table"
)]
const fn build_f64_pow10() -> [f64; 23] {
    let mut table = [0.0; 23];
    let mut i = 0;
    while i < table.len() {
        table[i] = pow10(i as u32) as f64;
        i += 1;
    }
    table
}

#[expect(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    reason = "i<11 and 10^10 fits u64 and f32 exactly because 5^10<2^24; each entry comes from the existing integer table"
)]
const fn build_f32_pow10() -> [f32; 11] {
    let mut table = [0.0; 11];
    let mut i = 0;
    while i < table.len() {
        table[i] = pow10_64(i as u32) as f32;
        i += 1;
    }
    table
}

pub(crate) const BINARY_POW5_CACHE: [u128; 651] = [
    0xeef453d6923bd65a113faa2906a13b3f,
    0x9558b4661b6565f84ac7ca59a424c507,
    0xbaaee17fa23ebf765d79bcf00d2df649,
    0xe95a99df8ace6f53f4d82c2c107973dc,
    0x91d8a02bb6c1059479071b9b8a4be869,
    0xb64ec836a47146f99748e2826cdee284,
    0xe3e27a444d8d98b7fd1b1b2308169b25,
    0x8e6d8c6ab0787f72fe30f0f5e50e20f7,
    0xb208ef855c969f4fbdbd2d335e51a935,
    0xde8b2b66b3bc4723ad2c788035e61382,
    0x8b16fb203055ac764c3bcb5021afcc31,
    0xaddcb9e83c6b1793df4abe242a1bbf3d,
    0xd953e8624b85dd78d71d6dad34a2af0d,
    0x87d4713d6f33aa6b8672648c40e5ad68,
    0xa9c98d8ccb009506680efdaf511f18c2,
    0xd43bf0effdc0ba480212bd1b2566def2,
    0x84a57695fe98746d014bb630f7604b57,
    0xa5ced43b7e3e9188419ea3bd35385e2d,
    0xcf42894a5dce35ea52064cac828675b9,
    0x818995ce7aa0e1b27343efebd1940993,
    0xa1ebfb4219491a1f1014ebe6c5f90bf8,
    0xca66fa129f9b60a6d41a26e077774ef6,
    0xfd00b897478238d08920b098955522b4,
    0x9e20735e8cb1638255b46e5f5d5535b0,
    0xc5a890362fddbc62eb2189f734aa831d,
    0xf712b443bbd52b7ba5e9ec7501d523e4,
    0x9a6bb0aa55653b2d47b233c92125366e,
    0xc1069cd4eabe89f8999ec0bb696e840a,
    0xf148440a256e2c76c00670ea43ca250d,
    0x96cd2a865764dbca380406926a5e5728,
    0xbc807527ed3e12bcc605083704f5ecf2,
    0xeba09271e88d976bf7864a44c633682e,
    0x93445b8731587ea37ab3ee6afbe0211d,
    0xb8157268fdae9e4c5960ea05bad82964,
    0xe61acf033d1a45df6fb92487298e33bd,
    0x8fd0c16206306baba5d3b6d479f8e056,
    0xb3c4f1ba87bc86968f48a4899877186c,
    0xe0b62e2929aba83c331acdabfe94de87,
    0x8c71dcd9ba0b49259ff0c08b7f1d0b14,
    0xaf8e5410288e1b6f07ecf0ae5ee44dd9,
    0xdb71e91432b1a24ac9e82cd9f69d6150,
    0x892731ac9faf056ebe311c083a225cd2,
    0xab70fe17c79ac6ca6dbd630a48aaf406,
    0xd64d3d9db981787d092cbbccdad5b108,
    0x85f0468293f0eb4e25bbf56008c58ea5,
    0xa76c582338ed2621af2af2b80af6f24e,
    0xd1476e2c07286faa1af5af660db4aee1,
    0x82cca4db847945ca50d98d9fc890ed4d,
    0xa37fce126597973ce50ff107bab528a0,
    0xcc5fc196fefd7d0c1e53ed49a96272c8,
    0xff77b1fcbebcdc4f25e8e89c13bb0f7a,
    0x9faacf3df73609b177b191618c54e9ac,
    0xc795830d75038c1dd59df5b9ef6a2417,
    0xf97ae3d0d2446f254b0573286b44ad1d,
    0x9becce62836ac5774ee367f9430aec32,
    0xc2e801fb244576d5229c41f793cda73f,
    0xf3a20279ed56d48a6b43527578c1110f,
    0x9845418c345644d6830a13896b78aaa9,
    0xbe5691ef416bd60c23cc986bc656d553,
    0xedec366b11c6cb8f2cbfbe86b7ec8aa8,
    0x94b3a202eb1c3f397bf7d71432f3d6a9,
    0xb9e08a83a5e34f07daf5ccd93fb0cc53,
    0xe858ad248f5c22c9d1b3400f8f9cff68,
    0x91376c36d99995be23100809b9c21fa1,
    0xb58547448ffffb2dabd40a0c2832a78a,
    0xe2e69915b3fff9f916c90c8f323f516c,
    0x8dd01fad907ffc3bae3da7d97f6792e3,
    0xb1442798f49ffb4a99cd11cfdf41779c,
    0xdd95317f31c7fa1d40405643d711d583,
    0x8a7d3eef7f1cfc52482835ea666b2572,
    0xad1c8eab5ee43b66da3243650005eecf,
    0xd863b256369d4a4090bed43e40076a82,
    0x873e4f75e2224e685a7744a6e804a291,
    0xa90de3535aaae202711515d0a205cb36,
    0xd3515c2831559a830d5a5b44ca873e03,
    0x8412d9991ed58091e858790afe9486c2,
    0xa5178fff668ae0b6626e974dbe39a872,
    0xce5d73ff402d98e3fb0a3d212dc8128f,
    0x80fa687f881c7f8e7ce66634bc9d0b99,
    0xa139029f6a239f721c1fffc1ebc44e80,
    0xc987434744ac874ea327ffb266b56220,
    0xfbe9141915d7a9224bf1ff9f0062baa8,
    0x9d71ac8fada6c9b56f773fc3603db4a9,
    0xc4ce17b399107c22cb550fb4384d21d3,
    0xf6019da07f549b2b7e2a53a146606a48,
    0x99c102844f94e0fb2eda7444cbfc426d,
    0xc0314325637a1939fa911155fefb5308,
    0xf03d93eebc589f88793555ab7eba27ca,
    0x96267c7535b763b54bc1558b2f3458de,
    0xbbb01b9283253ca29eb1aaedfb016f16,
    0xea9c227723ee8bcb465e15a979c1cadc,
    0x92a1958a7675175f0bfacd89ec191ec9,
    0xb749faed14125d36cef980ec671f667b,
    0xe51c79a85916f48482b7e12780e7401a,
    0x8f31cc0937ae58d2d1b2ecb8b0908810,
    0xb2fe3f0b8599ef07861fa7e6dcb4aa15,
    0xdfbdcece67006ac967a791e093e1d49a,
    0x8bd6a141006042bde0c8bb2c5c6d24e0,
    0xaecc49914078536d58fae9f773886e18,
    0xda7f5bf590966848af39a475506a899e,
    0x888f99797a5e012d6d8406c952429603,
    0xaab37fd7d8f58178c8e5087ba6d33b83,
    0xd5605fcdcf32e1d6fb1e4a9a90880a64,
    0x855c3be0a17fcd265cf2eea09a55067f,
    0xa6b34ad8c9dfc06ff42faa48c0ea481e,
    0xd0601d8efc57b08bf13b94daf124da26,
    0x823c12795db6ce5776c53d08d6b70858,
    0xa2cb1717b52481ed54768c4b0c64ca6e,
    0xcb7ddcdda26da268a9942f5dcf7dfd09,
    0xfe5d54150b090b02d3f93b35435d7c4c,
    0x9efa548d26e5a6e1c47bc5014a1a6daf,
    0xc6b8e9b0709f109a359ab6419ca1091b,
    0xf867241c8cc6d4c0c30163d203c94b62,
    0x9b407691d7fc44f879e0de63425dcf1d,
    0xc21094364dfb5636985915fc12f542e4,
    0xf294b943e17a2bc43e6f5b7b17b2939d,
    0x979cf3ca6cec5b5aa705992ceecf9c42,
    0xbd8430bd0827723150c6ff782a838353,
    0xece53cec4a314ebda4f8bf5635246428,
    0x940f4613ae5ed136871b7795e136be99,
    0xb913179899f6858428e2557b59846e3f,
    0xe757dd7ec07426e5331aeada2fe589cf,
    0x9096ea6f3848984f3ff0d2c85def7621,
    0xb4bca50b065abe630fed077a756b53a9,
    0xe1ebce4dc7f16dfbd3e8495912c62894,
    0x8d3360f09cf6e4bd64712dd7abbbd95c,
    0xb080392cc4349decbd8d794d96aacfb3,
    0xdca04777f541c567ecf0d7a0fc5583a0,
    0x89e42caaf9491b60f41686c49db57244,
    0xac5d37d5b79b6239311c2875c522ced5,
    0xd77485cb25823ac77d633293366b828b,
    0x86a8d39ef77164bcae5dff9c02033197,
    0xa8530886b54dbdebd9f57f830283fdfc,
    0xd267caa862a12d66d072df63c324fd7b,
    0x8380dea93da4bc604247cb9e59f71e6d,
    0xa46116538d0deb7852d9be85f074e608,
    0xcd795be87051665667902e276c921f8b,
    0x806bd9714632dff600ba1cd8a3db53b6,
    0xa086cfcd97bf97f380e8a40eccd228a4,
    0xc8a883c0fdaf7df06122cd128006b2cd,
    0xfad2a4b13d1b5d6c796b805720085f81,
    0x9cc3a6eec6311a63cbe3303674053bb0,
    0xc3f490aa77bd60fcbedbfc4411068a9c,
    0xf4f1b4d515acb93bee92fb5515482d44,
    0x991711052d8bf3c5751bdd152d4d1c4a,
    0xbf5cd54678eef0b6d262d45a78a0635d,
    0xef340a98172aace486fb897116c87c34,
    0x9580869f0e7aac0ed45d35e6ae3d4da0,
    0xbae0a846d21957128974836059cca109,
    0xe998d258869facd72bd1a438703fc94b,
    0x91ff83775423cc067b6306a34627ddcf,
    0xb67f6455292cbf081a3bc84c17b1d542,
    0xe41f3d6a7377eeca20caba5f1d9e4a93,
    0x8e938662882af53e547eb47b7282ee9c,
    0xb23867fb2a35b28de99e619a4f23aa43,
    0xdec681f9f4c31f316405fa00e2ec94d4,
    0x8b3c113c38f9f37ede83bc408dd3dd04,
    0xae0b158b4738705e9624ab50b148d445,
    0xd98ddaee19068c763badd624dd9b0957,
    0x87f8a8d4cfa417c9e54ca5d70a80e5d6,
    0xa9f6d30a038d1dbc5e9fcf4ccd211f4c,
    0xd47487cc8470652b7647c3200069671f,
    0x84c8d4dfd2c63f3b29ecd9f40041e073,
    0xa5fb0a17c777cf09f468107100525890,
    0xcf79cc9db955c2cc7182148d4066eeb4,
    0x81ac1fe293d599bfc6f14cd848405530,
    0xa21727db38cb002fb8ada00e5a506a7c,
    0xca9cf1d206fdc03ba6d90811f0e4851c,
    0xfd442e4688bd304a908f4a166d1da663,
    0x9e4a9cec15763e2e9a598e4e043287fe,
    0xc5dd44271ad3cdba40eff1e1853f29fd,
    0xf7549530e188c128d12bee59e68ef47c,
    0x9a94dd3e8cf578b982bb74f8301958ce,
    0xc13a148e3032d6e7e36a52363c1faf01,
    0xf18899b1bc3f8ca1dc44e6c3cb279ac1,
    0x96f5600f15a7b7e529ab103a5ef8c0b9,
    0xbcb2b812db11a5de7415d448f6b6f0e7,
    0xebdf661791d60f56111b495b3464ad21,
    0x936b9fcebb25c995cab10dd900beec34,
    0xb84687c269ef3bfb3d5d514f40eea742,
    0xe65829b3046b0afa0cb4a5a3112a5112,
    0x8ff71a0fe2c2e6dc47f0e785eaba72ab,
    0xb3f4e093db73a09359ed216765690f56,
    0xe0f218b8d25088b8306869c13ec3532c,
    0x8c974f73837255731e414218c73a13fb,
    0xafbd2350644eeacfe5d1929ef90898fa,
    0xdbac6c247d62a583df45f746b74abf39,
    0x894bc396ce5da7726b8bba8c328eb783,
    0xab9eb47c81f5114f066ea92f3f326564,
    0xd686619ba27255a2c80a537b0efefebd,
    0x8613fd0145877585bd06742ce95f5f36,
    0xa798fc4196e952e72c48113823b73704,
    0xd17f3b51fca3a7a0f75a15862ca504c5,
    0x82ef85133de648c49a984d73dbe722fb,
    0xa3ab66580d5fdaf5c13e60d0d2e0ebba,
    0xcc963fee10b7d1b3318df905079926a8,
    0xffbbcfe994e5c61ffdf17746497f7052,
    0x9fd561f1fd0f9bd3feb6ea8bedefa633,
    0xc7caba6e7c5382c8fe64a52ee96b8fc0,
    0xf9bd690a1b68637b3dfdce7aa3c673b0,
    0x9c1661a651213e2d06bea10ca65c084e,
    0xc31bfa0fe5698db8486e494fcff30a62,
    0xf3e2f893dec3f1265a89dba3c3efccfa,
    0x986ddb5c6b3a76b7f89629465a75e01c,
    0xbe89523386091465f6bbb397f1135823,
    0xee2ba6c0678b597f746aa07ded582e2c,
    0x94db483840b717efa8c2a44eb4571cdc,
    0xba121a4650e4ddeb92f34d62616ce413,
    0xe896a0d7e51e156677b020baf9c81d17,
    0x915e2486ef32cd600ace1474dc1d122e,
    0xb5b5ada8aaff80b80d819992132456ba,
    0xe3231912d5bf60e610e1fff697ed6c69,
    0x8df5efabc5979c8fca8d3ffa1ef463c1,
    0xb1736b96b6fd83b3bd308ff8a6b17cb2,
    0xddd0467c64bce4a0ac7cb3f6d05ddbde,
    0x8aa22c0dbef60ee46bcdf07a423aa96b,
    0xad4ab7112eb3929d86c16c98d2c953c6,
    0xd89d64d57a607744e871c7bf077ba8b7,
    0x87625f056c7c4a8b11471cd764ad4972,
    0xa93af6c6c79b5d2dd598e40d3dd89bcf,
    0xd389b478798234794aff1d108d4ec2c3,
    0x843610cb4bf160cbcedf722a585139ba,
    0xa54394fe1eedb8fec2974eb4ee658828,
    0xce947a3da6a9273e733d226229feea32,
    0x811ccc668829b8870806357d5a3f525f,
    0xa163ff802a3426a8ca07c2dcb0cf26f7,
    0xc9bcff6034c13052fc89b393dd02f0b5,
    0xfc2c3f3841f17c67bbac2078d443ace2,
    0x9d9ba7832936edc0d54b944b84aa4c0d,
    0xc5029163f384a9310a9e795e65d4df11,
    0xf64335bcf065d37d4d4617b5ff4a16d5,
    0x99ea0196163fa42e504bced1bf8e4e45,
    0xc06481fb9bcf8d39e45ec2862f71e1d6,
    0xf07da27a82c370885d767327bb4e5a4c,
    0x964e858c91ba26553a6a07f8d510f86f,
    0xbbe226efb628afea890489f70a55368b,
    0xeadab0aba3b2dbe52b45ac74ccea842e,
    0x92c8ae6b464fc96f3b0b8bc90012929d,
    0xb77ada0617e3bbcb09ce6ebb40173744,
    0xe55990879ddcaabdcc420a6a101d0515,
    0x8f57fa54c2a9eab69fa946824a12232d,
    0xb32df8e9f354656447939822dc96abf9,
    0xdff9772470297ebd59787e2b93bc56f7,
    0x8bfbea76c619ef3657eb4edb3c55b65a,
    0xaefae51477a06b03ede622920b6b23f1,
    0xdab99e59958885c4e95fab368e45eced,
    0x88b402f7fd75539b11dbcb0218ebb414,
    0xaae103b5fcd2a881d652bdc29f26a119,
    0xd59944a37c0752a24be76d3346f0495f,
    0x857fcae62d8493a56f70a4400c562ddb,
    0xa6dfbd9fb8e5b88ecb4ccd500f6bb952,
    0xd097ad07a71f26b27e2000a41346a7a7,
    0x825ecc24c873782f8ed400668c0c28c8,
    0xa2f67f2dfa90563b728900802f0f32fa,
    0xcbb41ef979346bca4f2b40a03ad2ffb9,
    0xfea126b7d78186bce2f610c84987bfa8,
    0x9f24b832e6b0f4360dd9ca7d2df4d7c9,
    0xc6ede63fa05d314391503d1c79720dbb,
    0xf8a95fcf88747d9475a44c6397ce912a,
    0x9b69dbe1b548ce7cc986afbe3ee11aba,
    0xc24452da229b021bfbe85badce996168,
    0xf2d56790ab41c2a2fae27299423fb9c3,
    0x97c560ba6b0919a5dccd879fc967d41a,
    0xbdb6b8e905cb600f5400e987bbc1c920,
    0xed246723473e3813290123e9aab23b68,
    0x9436c0760c86e30bf9a0b6720aaf6521,
    0xb94470938fa89bcef808e40e8d5b3e69,
    0xe7958cb87392c2c2b60b1d1230b20e04,
    0x90bd77f3483bb9b9b1c6f22b5e6f48c2,
    0xb4ecd5f01a4aa8281e38aeb6360b1af3,
    0xe2280b6c20dd523225c6da63c38de1b0,
    0x8d590723948a535f579c487e5a38ad0e,
    0xb0af48ec79ace8372d835a9df0c6d851,
    0xdcdb1b2798182244f8e431456cf88e65,
    0x8a08f0f8bf0f156b1b8e9ecb641b58ff,
    0xac8b2d36eed2dac5e272467e3d222f3f,
    0xd7adf884aa8791775b0ed81dcc6abb0f,
    0x86ccbb52ea94baea98e947129fc2b4e9,
    0xa87fea27a539e9a53f2398d747b36224,
    0xd29fe4b18e88640e8eec7f0d19a03aad,
    0x83a3eeeef9153e891953cf68300424ac,
    0xa48ceaaab75a8e2b5fa8c3423c052dd7,
    0xcdb02555653131b63792f412cb06794d,
    0x808e17555f3ebf11e2bbd88bbee40bd0,
    0xa0b19d2ab70e6ed65b6aceaeae9d0ec4,
    0xc8de047564d20a8bf245825a5a445275,
    0xfb158592be068d2eeed6e2f0f0d56712,
    0x9ced737bb6c4183d55464dd69685606b,
    0xc428d05aa4751e4caa97e14c3c26b886,
    0xf53304714d9265dfd53dd99f4b3066a8,
    0x993fe2c6d07b7fabe546a8038efe4029,
    0xbf8fdb78849a5f96de98520472bdd033,
    0xef73d256a5c0f77c963e66858f6d4440,
    0x95a8637627989aaddde7001379a44aa8,
    0xbb127c53b17ec1595560c018580d5d52,
    0xe9d71b689dde71afaab8f01e6e10b4a6,
    0x9226712162ab070dcab3961304ca70e8,
    0xb6b00d69bb55c8d13d607b97c5fd0d22,
    0xe45c10c42a2b3b058cb89a7db77c506a,
    0x8eb98a7a9a5b04e377f3608e92adb242,
    0xb267ed1940f1c61c55f038b237591ed3,
    0xdf01e85f912e37a36b6c46dec52f6688,
    0x8b61313bbabce2c62323ac4b3b3da015,
    0xae397d8aa96c1b77abec975e0a0d081a,
    0xd9c7dced53c7225596e7bd358c904a21,
    0x881cea14545c75757e50d64177da2e54,
    0xaa242499697392d2dde50bd1d5d0b9e9,
    0xd4ad2dbfc3d07787955e4ec64b44e864,
    0x84ec3c97da624ab4bd5af13bef0b113e,
    0xa6274bbdd0fadd61ecb1ad8aeacdd58e,
    0xcfb11ead453994ba67de18eda5814af2,
    0x81ceb32c4b43fcf480eacf948770ced7,
    0xa2425ff75e14fc31a1258379a94d028d,
    0xcad2f7f5359a3b3e096ee45813a04330,
    0xfd87b5f28300ca0d8bca9d6e188853fc,
    0x9e74d1b791e07e48775ea264cf55347d,
    0xc612062576589dda95364afe032a819d,
    0xf79687aed3eec5513a83ddbd83f52204,
    0x9abe14cd44753b52c4926a9672793542,
    0xc16d9a0095928a2775b7053c0f178293,
    0xf1c90080baf72cb15324c68b12dd6338,
    0x971da05074da7beed3f6fc16ebca5e03,
    0xbce5086492111aea88f4bb1ca6bcf584,
    0xec1e4a7db69561a52b31e9e3d06c32e5,
    0x9392ee8e921d5d073aff322e62439fcf,
    0xb877aa3236a4b44909befeb9fad487c2,
    0xe69594bec44de15b4c2ebe687989a9b3,
    0x901d7cf73ab0acd90f9d37014bf60a10,
    0xb424dc35095cd80f538484c19ef38c94,
    0xe12e13424bb40e132865a5f206b06fb9,
    0x8cbccc096f5088cbf93f87b7442e45d3,
    0xafebff0bcb24aafef78f69a51539d748,
    0xdbe6fecebdedd5beb573440e5a884d1b,
    0x89705f4136b4a59731680a88f8953030,
    0xabcc77118461cefcfdc20d2b36ba7c3d,
    0xd6bf94d5e57a42bc3d32907604691b4c,
    0x8637bd05af6c69b5a63f9a49c2c1b10f,
    0xa7c5ac471b4784230fcf80dc33721d53,
    0xd1b71758e219652bd3c36113404ea4a8,
    0x83126e978d4fdf3b645a1cac083126e9,
    0xa3d70a3d70a3d70a3d70a3d70a3d70a3,
    0xcccccccccccccccccccccccccccccccc,
    0x80000000000000000000000000000000,
    0xa0000000000000000000000000000000,
    0xc8000000000000000000000000000000,
    0xfa000000000000000000000000000000,
    0x9c400000000000000000000000000000,
    0xc3500000000000000000000000000000,
    0xf4240000000000000000000000000000,
    0x98968000000000000000000000000000,
    0xbebc2000000000000000000000000000,
    0xee6b2800000000000000000000000000,
    0x9502f900000000000000000000000000,
    0xba43b740000000000000000000000000,
    0xe8d4a510000000000000000000000000,
    0x9184e72a000000000000000000000000,
    0xb5e620f4800000000000000000000000,
    0xe35fa931a00000000000000000000000,
    0x8e1bc9bf040000000000000000000000,
    0xb1a2bc2ec50000000000000000000000,
    0xde0b6b3a764000000000000000000000,
    0x8ac7230489e800000000000000000000,
    0xad78ebc5ac6200000000000000000000,
    0xd8d726b7177a80000000000000000000,
    0x878678326eac90000000000000000000,
    0xa968163f0a57b4000000000000000000,
    0xd3c21bcecceda1000000000000000000,
    0x84595161401484a00000000000000000,
    0xa56fa5b99019a5c80000000000000000,
    0xcecb8f27f4200f3a0000000000000000,
    0x813f3978f89409844000000000000000,
    0xa18f07d736b90be55000000000000000,
    0xc9f2c9cd04674edea400000000000000,
    0xfc6f7c40458122964d00000000000000,
    0x9dc5ada82b70b59df020000000000000,
    0xc5371912364ce3056c28000000000000,
    0xf684df56c3e01bc6c732000000000000,
    0x9a130b963a6c115c3c7f400000000000,
    0xc097ce7bc90715b34b9f100000000000,
    0xf0bdc21abb48db201e86d40000000000,
    0x96769950b50d88f41314448000000000,
    0xbc143fa4e250eb3117d955a000000000,
    0xeb194f8e1ae525fd5dcfab0800000000,
    0x92efd1b8d0cf37be5aa1cae500000000,
    0xb7abc627050305adf14a3d9e40000000,
    0xe596b7b0c643c7196d9ccd05d0000000,
    0x8f7e32ce7bea5c6fe4820023a2000000,
    0xb35dbf821ae4f38bdda2802c8a800000,
    0xe0352f62a19e306ed50b2037ad200000,
    0x8c213d9da502de454526f422cc340000,
    0xaf298d050e4395d69670b12b7f410000,
    0xdaf3f04651d47b4c3c0cdd765f114000,
    0x88d8762bf324cd0fa5880a69fb6ac800,
    0xab0e93b6efee00538eea0d047a457a00,
    0xd5d238a4abe9806872a4904598d6d880,
    0x85a36366eb71f04147a6da2b7f864750,
    0xa70c3c40a64e6c51999090b65f67d924,
    0xd0cf4b50cfe20765fff4b4e3f741cf6d,
    0x82818f1281ed449fbff8f10e7a8921a4,
    0xa321f2d7226895c7aff72d52192b6a0d,
    0xcbea6f8ceb02bb399bf4f8a69f764490,
    0xfee50b7025c36a0802f236d04753d5b4,
    0x9f4f2726179a224501d762422c946590,
    0xc722f0ef9d80aad6424d3ad2b7b97ef5,
    0xf8ebad2b84e0d58bd2e0898765a7deb2,
    0x9b934c3b330c857763cc55f49f88eb2f,
    0xc2781f49ffcfa6d53cbf6b71c76b25fb,
    0xf316271c7fc3908a8bef464e3945ef7a,
    0x97edd871cfda3a5697758bf0e3cbb5ac,
    0xbde94e8e43d0c8ec3d52eeed1cbea317,
    0xed63a231d4c4fb274ca7aaa863ee4bdd,
    0x945e455f24fb1cf88fe8caa93e74ef6a,
    0xb975d6b6ee39e436b3e2fd538e122b44,
    0xe7d34c64a9c85d4460dbbca87196b616,
    0x90e40fbeea1d3a4abc8955e946fe31cd,
    0xb51d13aea4a488dd6babab6398bdbe41,
    0xe264589a4dcdab14c696963c7eed2dd1,
    0x8d7eb76070a08aecfc1e1de5cf543ca2,
    0xb0de65388cc8ada83b25a55f43294bcb,
    0xdd15fe86affad91249ef0eb713f39ebe,
    0x8a2dbf142dfcc7ab6e3569326c784337,
    0xacb92ed9397bf99649c2c37f07965404,
    0xd7e77a8f87daf7fbdc33745ec97be906,
    0x86f0ac99b4e8dafd69a028bb3ded71a3,
    0xa8acd7c0222311bcc40832ea0d68ce0c,
    0xd2d80db02aabd62bf50a3fa490c30190,
    0x83c7088e1aab65db792667c6da79e0fa,
    0xa4b8cab1a1563f52577001b891185938,
    0xcde6fd5e09abcf26ed4c0226b55e6f86,
    0x80b05e5ac60b6178544f8158315b05b4,
    0xa0dc75f1778e39d6696361ae3db1c721,
    0xc913936dd571c84c03bc3a19cd1e38e9,
    0xfb5878494ace3a5f04ab48a04065c723,
    0x9d174b2dcec0e47b62eb0d64283f9c76,
    0xc45d1df942711d9a3ba5d0bd324f8394,
    0xf5746577930d6500ca8f44ec7ee36479,
    0x9968bf6abbe85f207e998b13cf4e1ecb,
    0xbfc2ef456ae276e89e3fedd8c321a67e,
    0xefb3ab16c59b14a2c5cfe94ef3ea101e,
    0x95d04aee3b80ece5bba1f1d158724a12,
    0xbb445da9ca61281f2a8a6e45ae8edc97,
    0xea1575143cf97226f52d09d71a3293bd,
    0x924d692ca61be758593c2626705f9c56,
    0xb6e0c377cfa2e12e6f8b2fb00c77836c,
    0xe498f455c38b997a0b6dfb9c0f956447,
    0x8edf98b59a373fec4724bd4189bd5eac,
    0xb2977ee300c50fe758edec91ec2cb657,
    0xdf3d5e9bc0f653e12f2967b66737e3ed,
    0x8b865b215899f46cbd79e0d20082ee74,
    0xae67f1e9aec07187ecd8590680a3aa11,
    0xda01ee641a708de9e80e6f4820cc9495,
    0x884134fe908658b23109058d147fdcdd,
    0xaa51823e34a7eedebd4b46f0599fd415,
    0xd4e5e2cdc1d1ea966c9e18ac7007c91a,
    0x850fadc09923329e03e2cf6bc604ddb0,
    0xa6539930bf6bff4584db8346b786151c,
    0xcfe87f7cef46ff16e612641865679a63,
    0x81f14fae158c5f6e4fcb7e8f3f60c07e,
    0xa26da3999aef7749e3be5e330f38f09d,
    0xcb090c8001ab551c5cadf5bfd3072cc5,
    0xfdcb4fa002162a6373d9732fc7c8f7f6,
    0x9e9f11c4014dda7e2867e7fddcdd9afa,
    0xc646d63501a1511db281e1fd541501b8,
    0xf7d88bc24209a5651f225a7ca91a4226,
    0x9ae757596946075f3375788de9b06958,
    0xc1a12d2fc39789370052d6b1641c83ae,
    0xf209787bb47d6b84c0678c5dbd23a49a,
    0x9745eb4d50ce6332f840b7ba963646e0,
    0xbd176620a501fbffb650e5a93bc3d898,
    0xec5d3fa8ce427affa3e51f138ab4cebe,
    0x93ba47c980e98cdfc66f336c36b10137,
    0xb8a8d9bbe123f017b80b0047445d4184,
    0xe6d3102ad96cec1da60dc059157491e5,
    0x9043ea1ac7e4139287c89837ad68db2f,
    0xb454e4a179dd187729babe4598c311fb,
    0xe16a1dc9d8545e94f4296dd6fef3d67a,
    0x8ce2529e2734bb1d1899e4a65f58660c,
    0xb01ae745b101e9e45ec05dcff72e7f8f,
    0xdc21a1171d42645d76707543f4fa1f73,
    0x899504ae72497eba6a06494a791c53a8,
    0xabfa45da0edbde690487db9d17636892,
    0xd6f8d7509292d60345a9d2845d3c42b6,
    0x865b86925b9bc5c20b8a2392ba45a9b2,
    0xa7f26836f282b7328e6cac7768d7141e,
    0xd1ef0244af2364ff3207d795430cd926,
    0x8335616aed761f1f7f44e6bd49e807b8,
    0xa402b9c5a8d3a6e75f16206c9c6209a6,
    0xcd036837130890a136dba887c37a8c0f,
    0x802221226be55a64c2494954da2c9789,
    0xa02aa96b06deb0fdf2db9baa10b7bd6c,
    0xc83553c5c8965d3d6f92829494e5acc7,
    0xfa42a8b73abbf48ccb772339ba1f17f9,
    0x9c69a97284b578d7ff2a760414536efb,
    0xc38413cf25e2d70dfef5138519684aba,
    0xf46518c2ef5b8cd17eb258665fc25d69,
    0x98bf2f79d5993802ef2f773ffbd97a61,
    0xbeeefb584aff8603aafb550ffacfd8fa,
    0xeeaaba2e5dbf678495ba2a53f983cf38,
    0x952ab45cfa97a0b2dd945a747bf26183,
    0xba756174393d88df94f971119aeef9e4,
    0xe912b9d1478ceb177a37cd5601aab85d,
    0x91abb422ccb812eeac62e055c10ab33a,
    0xb616a12b7fe617aa577b986b314d6009,
    0xe39c49765fdf9d94ed5a7e85fda0b80b,
    0x8e41ade9fbebc27d14588f13be847307,
    0xb1d219647ae6b31c596eb2d8ae258fc8,
    0xde469fbd99a05fe36fca5f8ed9aef3bb,
    0x8aec23d680043bee25de7bb9480d5854,
    0xada72ccc20054ae9af561aa79a10ae6a,
    0xd910f7ff28069da41b2ba1518094da04,
    0x87aa9aff7904228690fb44d2f05d0842,
    0xa99541bf57452b28353a1607ac744a53,
    0xd3fa922f2d1675f242889b8997915ce8,
    0x847c9b5d7c2e09b769956135febada11,
    0xa59bc234db398c2543fab9837e699095,
    0xcf02b2c21207ef2e94f967e45e03f4bb,
    0x8161afb94b44f57d1d1be0eebac278f5,
    0xa1ba1ba79e1632dc6462d92a69731732,
    0xca28a291859bbf937d7b8f7503cfdcfe,
    0xfcb2cb35e702af785cda735244c3d43e,
    0x9defbf01b061adab3a0888136afa64a7,
    0xc56baec21c7a1916088aaa1845b8fdd0,
    0xf6c69a72a3989f5b8aad549e57273d45,
    0x9a3c2087a63f639936ac54e2f678864b,
    0xc0cb28a98fcf3c7f84576a1bb416a7dd,
    0xf0fdf2d3f3c30b9f656d44a2a11c51d5,
    0x969eb7c47859e7439f644ae5a4b1b325,
    0xbc4665b596706114873d5d9f0dde1fee,
    0xeb57ff22fc0c7959a90cb506d155a7ea,
    0x9316ff75dd87cbd809a7f12442d588f2,
    0xb7dcbf5354e9bece0c11ed6d538aeb2f,
    0xe5d3ef282a242e818f1668c8a86da5fa,
    0x8fa475791a569d10f96e017d694487bc,
    0xb38d92d760ec445537c981dcc395a9ac,
    0xe070f78d3927556a85bbe253f47b1417,
    0x8c469ab843b8956293956d7478ccec8e,
    0xaf58416654a6babb387ac8d1970027b2,
    0xdb2e51bfe9d0696a06997b05fcc0319e,
    0x88fcf317f22241e2441fece3bdf81f03,
    0xab3c2fddeeaad25ad527e81cad7626c3,
    0xd60b3bd56a5586f18a71e223d8d3b074,
    0x85c7056562757456f6872d5667844e49,
    0xa738c6bebb12d16cb428f8ac016561db,
    0xd106f86e69d785c7e13336d701beba52,
    0x82a45b450226b39cecc0024661173473,
    0xa34d721642b0608427f002d7f95d0190,
    0xcc20ce9bd35c78a531ec038df7b441f4,
    0xff290242c83396ce7e67047175a15271,
    0x9f79a169bd203e410f0062c6e984d386,
    0xc75809c42c684dd152c07b78a3e60868,
    0xf92e0c3537826145a7709a56ccdf8a82,
    0x9bbcc7a142b17ccb88a66076400bb691,
    0xc2abf989935ddbfe6acff893d00ea435,
    0xf356f7ebf83552fe0583f6b8c4124d43,
    0x98165af37b2153dec3727a337a8b704a,
    0xbe1bf1b059e9a8d6744f18c0592e4c5c,
    0xeda2ee1c7064130c1162def06f79df73,
    0x9485d4d1c63e8be78addcb5645ac2ba8,
    0xb9a74a0637ce2ee16d953e2bd7173692,
    0xe8111c87c5c1ba99c8fa8db6ccdd0437,
    0x910ab1d4db9914a01d9c9892400a22a2,
    0xb54d5e4a127f59c82503beb6d00cab4b,
    0xe2a0b5dc971f303a2e44ae64840fd61d,
    0x8da471a9de737e245ceaecfed289e5d2,
    0xb10d8e1456105dad7425a83e872c5f47,
    0xdd50f1996b947518d12f124e28f77719,
    0x8a5296ffe33cc92f82bd6b70d99aaa6f,
    0xace73cbfdc0bfb7b636cc64d1001550b,
    0xd8210befd30efa5a3c47f7e05401aa4e,
    0x8714a775e3e95c7865acfaec34810a71,
    0xa8d9d1535ce3b3967f1839a741a14d0d,
    0xd31045a8341ca07c1ede48111209a050,
    0x83ea2b892091e44d934aed0aab460432,
    0xa4e4b66b68b65d60f81da84d5617853f,
    0xce1de40642e3f4b936251260ab9d668e,
    0x80d2ae83e9ce78f3c1d72b7c6b426019,
    0xa1075a24e4421730b24cf65b8612f81f,
    0xc94930ae1d529cfcdee033f26797b627,
    0xfb9b7cd9a4a7443c169840ef017da3b1,
    0x9d412e0806e88aa58e1f289560ee864e,
    0xc491798a08a2ad4ef1a6f2bab92a27e2,
    0xf5b5d7ec8acb58a2ae10af696774b1db,
    0x9991a6f3d6bf1765acca6da1e0a8ef29,
    0xbff610b0cc6edd3f17fd090a58d32af3,
    0xeff394dcff8a948eddfc4b4cef07f5b0,
    0x95f83d0a1fb69cd94abdaf101564f98e,
    0xbb764c4ca7a4440f9d6d1ad41abe37f1,
    0xea53df5fd18d551384c86189216dc5ed,
    0x92746b9be2f8552c32fd3cf5b4e49bb4,
    0xb7118682dbb66a773fbc8c33221dc2a1,
    0xe4d5e82392a405150fabaf3feaa5334a,
    0x8f05b1163ba6832d29cb4d87f2a7400e,
    0xb2c71d5bca9023f8743e20e9ef511012,
    0xdf78e4b2bd342cf6914da9246b255416,
    0x8bab8eefb6409c1a1ad089b6c2f7548e,
    0xae9672aba3d0c320a184ac2473b529b1,
    0xda3c0f568cc4f3e8c9e5d72d90a2741e,
    0x8865899617fb18717e2fa67c7a658892,
    0xaa7eebfb9df9de8dddbb901b98feeab7,
    0xd51ea6fa85785631552a74227f3ea565,
    0x8533285c936b35ded53a88958f87275f,
    0xa67ff273b84603568a892abaf368f137,
    0xd01fef10a657842c2d2b7569b0432d85,
    0x8213f56a67f6b29b9c3b29620e29fc73,
    0xa298f2c501f45f428349f3ba91b47b8f,
    0xcb3f2f7642717713241c70a936219a73,
    0xfe0efb53d30dd4d7ed238cd383aa0110,
    0x9ec95d1463e8a506f4363804324a40aa,
    0xc67bb4597ce2ce48b143c6053edcd0d5,
    0xf81aa16fdc1b81dadd94b7868e94050a,
    0x9b10a4e5e9913128ca7cf2b4191c8326,
    0xc1d4ce1f63f57d72fd1c2f611f63a3f0,
    0xf24a01a73cf2dccfbc633b39673c8cec,
    0x976e41088617ca01d5be0503e085d813,
    0xbd49d14aa79dbc824b2d8644d8a74e18,
    0xec9c459d51852ba2ddf8e7d60ed1219e,
    0x93e1ab8252f33b45cabb90e5c942b503,
    0xb8da1662e7b00a173d6a751f3b936243,
    0xe7109bfba19c0c9d0cc512670a783ad4,
    0x906a617d450187e227fb2b80668b24c5,
    0xb484f9dc9641e9dab1f9f660802dedf6,
    0xe1a63853bbd264515e7873f8a0396973,
    0x8d07e33455637eb2db0b487b6423e1e8,
    0xb049dc016abc5e5f91ce1a9a3d2cda62,
    0xdc5c5301c56b75f77641a140cc7810fb,
    0x89b9b3e11b6329baa9e904c87fcb0a9d,
    0xac2820d9623bf429546345fa9fbdcd44,
    0xd732290fbacaf133a97c177947ad4095,
    0x867f59a9d4bed6c049ed8eabcccc485d,
    0xa81f301449ee8c705c68f256bfff5a74,
    0xd226fc195c6a2f8c73832eec6fff3111,
    0x83585d8fd9c25db7c831fd53c5ff7eab,
    0xa42e74f3d032f525ba3e7ca8b77f5e55,
    0xcd3a1230c43fb26f28ce1bd2e55f35eb,
    0x80444b5e7aa7cf857980d163cf5b81b3,
    0xa0555e361951c366d7e105bcc332621f,
    0xc86ab5c39fa634408dd9472bf3fefaa7,
    0xfa856334878fc150b14f98f6f0feb951,
    0x9c935e00d4b9d8d26ed1bf9a569f33d3,
    0xc3b8358109e84f070a862f80ec4700c8,
    0xf4a642e14c6262c8cd27bb612758c0fa,
    0x98e7e9cccfbd7dbd8038d51cb897789c,
    0xbf21e44003acdd2ce0470a63e6bd56c3,
    0xeeea5d50049814781858ccfce06cac74,
    0x95527a5202df0ccb0f37801e0c43ebc8,
    0xbaa718e68396cffdd30560258f54e6ba,
    0xe950df20247c83fd47c6b82ef32a2069,
    0x91d28b7416cdd27e4cdc331d57fa5441,
    0xb6472e511c81471de0133fe4adf8e952,
    0xe3d8f9e563a198e558180fddd97723a6,
    0x8e679c2f5e44ff8f570f09eaa7ea7648,
];

pub(crate) const BINARY_SUBNORMAL_POW5: [([u32; 86], usize); 3] = build_binary_subnormal_pow5();

#[expect(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    reason = "39 multiplications build at most 5^1053 (737 digits, 82 limbs); a base-10^9 limb times 5^27 plus carry fits u128; n=37..39 indexes three entries"
)]
const fn build_binary_subnormal_pow5() -> [([u32; 86], usize); 3] {
    let mut result = [([0; 86], 0); 3];
    let mut limbs = [0; 86];
    limbs[0] = 1;
    let mut used = 1;
    let mut n = 1;
    while n <= 39 {
        let mut carry = 0_u128;
        let mut i = 0;
        while i < used {
            let product = limbs[i] as u128 * BINARY_POW5[27] + carry;
            limbs[i] = (product % 1_000_000_000) as u32;
            carry = product / 1_000_000_000;
            i += 1;
        }
        while carry != 0 {
            limbs[used] = (carry % 1_000_000_000) as u32;
            carry /= 1_000_000_000;
            used += 1;
        }
        if n >= 37 {
            result[n - 37] = (limbs, used);
        }
        n += 1;
    }
    result
}
