#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use core::cmp::Ordering;

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

use crate::layout::{InRange, Unpacked, W64, Width};
use crate::tables::{pow10, pow10_64, recip, recip_64};
use crate::{Error, Rounding};

#[expect(
    clippy::arithmetic_side_effects,
    reason = "bit_width <= 128, so bit_width*1233 <= 157824, t <= 38 and the result <= 39"
)]
#[cfg_attr(verus_keep_ghost, verus_spec(ret =>
    ensures
        ret <= 39,
        (x as int) < vstd::arithmetic::power::pow(10, ret as nat),
        ret > 0 ==> vstd::arithmetic::power::pow(10, (ret - 1) as nat) <= x as int,
        ret == crate::verus_model::decimal_digits(x as nat),
))]
pub(crate) fn digits(x: u128) -> u32 {
    let b = x.bit_width();
    let t = (b * 1233) >> 12;
    #[cfg(verus_keep_ghost)]
    proof! {
        vstd::arithmetic::power2::lemma2_to64();
        vstd::bits::lemma_u32_shr_is_div((b * 1233) as u32, 12);
        crate::verus_model::lemma_digits_estimate(x as int, b as nat, t as int);
    }
    #[cfg(verus_keep_ghost)]
    proof! { crate::verus_model::lemma_digits_result(x as int, t as nat); }
    t + u32::from(x >= pow10(t))
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "callers pass x < 2^127; with m < 2^128 the quotient is exact, 1 <= k <= 38, post <= 126, and q*10^k <= x, so the remainder subtraction cannot underflow"
)]
#[cfg_attr(verus_keep_ghost, verus_spec(ret =>
    requires
        x < 0x8000_0000_0000_0000_0000_0000_0000_0000u128,
    ensures
        ret.1 < vstd::arithmetic::power::pow(10, k as nat),
        x as int == ret.0 as int * vstd::arithmetic::power::pow(10, k as nat) + ret.1 as int,
))]
pub(crate) fn div_rem_pow10(x: u128, k: u32) -> (u128, u128) {
    #[cfg(verus_keep_ghost)]
    proof! { crate::verus_model::lemma_div_rem_pow10_edges(x, k); }
    if k == 0 {
        return (x, 0);
    }
    if k >= 39 {
        return (0, x);
    }
    let r = recip(k);
    let q = x.carrying_mul(r.m, 0).1 >> r.post;
    #[cfg(verus_keep_ghost)]
    proof! { crate::verus_model::lemma_div_rem_pow10_main(x, k, r.m, r.post, q); }
    (q, x - q * pow10(k))
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "callers pass coeff < 2^127, which has at most 38 trailing decimal zeros; each exact division decreases coeff and zeros never exceeds 38"
)]
#[cfg_attr(verus_keep_ghost, verus_spec(ret =>
    requires
        coeff < 0x8000_0000_0000_0000_0000_0000_0000_0000u128,
    ensures
        coeff == 0 ==> ret.0 == 0 && ret.1 == 0,
        coeff != 0 ==> coeff as int == ret.0 as int * vstd::arithmetic::power::pow(10, ret.1 as nat),
        coeff != 0 ==> ret.0 % 10 != 0,
        ret.1 <= 38,
))]
pub(crate) fn strip_trailing_zeros(mut coeff: u128) -> (u128, u32) {
    if coeff == 0 {
        return (0, 0);
    }
    #[cfg(verus_keep_ghost)]
    proof_decl! {
        let ghost orig: int = coeff as int;
        crate::verus_model::lemma_strip_start(orig);
    }
    let mut zeros = 0;
    let mut k = 32;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            crate::verus_model::strip_step_ok(k as int),
            0 < coeff as int <= orig,
            orig < 0x8000_0000_0000_0000_0000_0000_0000_0000int,
            orig == coeff as int * vstd::arithmetic::power::pow(10, zeros as nat),
            zeros as int + 2 * (k as int) <= 64,
            coeff as int % crate::verus_model::strip_scale(k as int) != 0,
        decreases k,
    ))]
    while k != 0 {
        let (q, r) = div_rem_pow10(coeff, k);
        #[cfg(verus_keep_ghost)]
        proof! { crate::verus_model::lemma_strip_step(orig, coeff as int, zeros as nat, k as nat, q as int, r as int); }
        if r == 0 {
            coeff = q;
            zeros += k;
        }
        k /= 2;
    }
    #[cfg(verus_keep_ghost)]
    proof! { crate::verus_model::lemma_strip_end(orig, coeff as int, zeros as nat); }
    (coeff, zeros)
}

#[inline]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "bit_width <= 64, so bit_width*1233 <= 78912, t <= 19 and the result <= 20"
)]
#[cfg_attr(verus_keep_ghost, verus_spec(ret =>
    ensures
        ret <= 20,
        (x as int) < vstd::arithmetic::power::pow(10, ret as nat),
        ret > 0 ==> vstd::arithmetic::power::pow(10, (ret - 1) as nat) <= x as int,
        ret == crate::verus_model::decimal_digits(x as nat),
))]
pub(crate) fn digits_64(x: u64) -> u32 {
    let b = x.bit_width();
    let t = (b * 1233) >> 12;
    #[cfg(verus_keep_ghost)]
    proof! {
        vstd::arithmetic::power2::lemma2_to64();
        vstd::bits::lemma_u32_shr_is_div((b * 1233) as u32, 12);
        crate::verus_model::lemma_digits_estimate(x as int, b as nat, t as int);
    }
    #[cfg(verus_keep_ghost)]
    proof! { crate::verus_model::lemma_digits_result(x as int, t as nat); }
    t + u32::from(x >= pow10_64(t))
}

#[inline]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "callers pass x < 2^63; with m < 2^64 the quotient is exact, 1 <= k <= 19, post <= 63, and q*10^k <= x, so the remainder subtraction cannot underflow"
)]
#[cfg_attr(verus_keep_ghost, verus_spec(ret =>
    requires
        x < 0x8000_0000_0000_0000u64,
    ensures
        ret.1 < vstd::arithmetic::power::pow(10, k as nat),
        x as int == ret.0 as int * vstd::arithmetic::power::pow(10, k as nat) + ret.1 as int,
))]
pub(crate) fn div_rem_pow10_64(x: u64, k: u32) -> (u64, u64) {
    #[cfg(verus_keep_ghost)]
    proof! { crate::verus_model::lemma_div_rem_pow10_64_edges(x, k); }
    if k == 0 {
        return (x, 0);
    }
    if k >= 20 {
        return (0, x);
    }
    let r = recip_64(k);
    let q = x.carrying_mul(r.m, 0).1 >> r.post;
    #[cfg(verus_keep_ghost)]
    proof! { crate::verus_model::lemma_div_rem_pow10_64_main(x, k, r.m, r.post, q); }
    (q, x - q * pow10_64(k))
}

#[inline]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "callers pass coeff < 10^16, which has at most 15 trailing decimal zeros; divisors are nonzero table constants, q*d <= coeff, and zeros never exceeds 15"
)]
#[cfg_attr(verus_keep_ghost, verus_spec(ret =>
    requires
        coeff < 10_000_000_000_000_000u64,
    ensures
        coeff == 0 ==> ret.0 == 0 && ret.1 == 0,
        coeff != 0 ==> coeff as int == ret.0 as int * vstd::arithmetic::power::pow(10, ret.1 as nat),
        coeff != 0 ==> ret.0 % 10 != 0,
        ret.1 <= 15,
))]
pub(crate) fn strip_trailing_zeros_64(mut coeff: u64) -> (u64, u32) {
    if coeff == 0 {
        return (0, 0);
    }
    #[cfg(verus_keep_ghost)]
    proof_decl! {
        let ghost orig: int = coeff as int;
        crate::verus_model::lemma_strip_start_64(orig);
    }
    let mut zeros = 0;
    let mut k = 8;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            crate::verus_model::strip_step_ok(k as int),
            k <= 8,
            0 < coeff as int <= orig,
            orig < 10_000_000_000_000_000int,
            orig == coeff as int * vstd::arithmetic::power::pow(10, zeros as nat),
            zeros as int + 2 * (k as int) <= 16,
            coeff as int % crate::verus_model::strip_scale(k as int) != 0,
        decreases k,
    ))]
    while k != 0 {
        let d = pow10_64(k);
        #[cfg(verus_keep_ghost)]
        proof! { vstd::arithmetic::power::lemma_pow_positive(10, k as nat); }
        let q = coeff / d;
        #[cfg(verus_keep_ghost)]
        proof! {
            vstd::arithmetic::div_mod::lemma_fundamental_div_mod(coeff as int, d as int);
            crate::verus_model::lemma_strip_step(orig, coeff as int, zeros as nat, k as nat, q as int, (coeff % d) as int);
        }
        if q * d == coeff {
            coeff = q;
            zeros += k;
        }
        k /= 2;
    }
    #[cfg(verus_keep_ghost)]
    proof! { crate::verus_model::lemma_strip_end_64(orig, coeff as int, zeros as nat); }
    (coeff, zeros)
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct Wide {
    pub(crate) neg: bool,
    pub(crate) coeff: u128,
    pub(crate) exp: i64,
    pub(crate) sticky: bool,
}

#[inline(never)]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "coeff < 2^127; sticky implies coeff >= 10^(P-1); P is 7/16/34. i128 exponent differences of i64 inputs fit; k >= 0, capped at 39 only for division. The quotient has <= P digits; exact scaling has <= P digits, increment <= 10^P, carry exponent <= i64::MAX+33. The returned exponent is >= qmin because k >= qmin - exp, and <= qmax by the final check. Callers pass pref_exp <= w.exp; otherwise an exact result would not move to its preferred exponent"
)]
#[cfg_attr(verus_keep_ghost, verus_spec(ret =>
    requires
        crate::layout::bid_width(W::LAYOUT),
        w.coeff < 0x8000_0000_0000_0000_0000_0000_0000_0000u128,
        w.sticky ==> w.coeff as int >= vstd::arithmetic::power::pow(10, (W::LAYOUT.digits - 1) as nat),
        w.coeff != 0 ==> pref_exp <= w.exp,
    ensures
        crate::verus_model::rounded(
            ret,
            w.neg,
            10 * w.coeff as int + if w.sticky { 5int } else { 0int },
            w.exp as int - 1,
            pref_exp as int,
            dir,
        ),
))]
pub(crate) fn fit<W: Width>(
    w: Wide,
    pref_exp: i64,
    dir: Option<Rounding>,
) -> Result<InRange<W>, Error> {
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::layout::lemma_layout_values();
        vstd::arithmetic::power::lemma_pow_positive(10, W::LAYOUT.digits as nat);
        crate::verus_model::lemma_pow10_bounds(W::LAYOUT.digits as nat);
    }
    let lay = &W::LAYOUT;
    let k = i128::from(digits(w.coeff).saturating_sub(lay.digits))
        .max(i128::from(lay.qmin) - i128::from(w.exp));
    let (mut coeff, rem) = div_rem_pow10(w.coeff, k.min(39) as u32);
    let mut exp = i128::from(w.exp) + k;
    let inexact = w.sticky || rem != 0;
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::verus_model::lemma_fit_split(
            w.coeff as int,
            w.sticky,
            w.exp as int,
            W::LAYOUT,
            crate::verus_model::decimal_digits(w.coeff as nat) as int,
            k as int,
            if k < 39 { k as int } else { 39 },
            coeff as int,
            rem as int,
        );
    }
    if !inexact {
        let pref = pref_exp.clamp(i64::from(lay.qmin), i64::from(lay.qmax));
        if coeff == 0 {
            #[cfg(verus_keep_ghost)]
            proof! {
                crate::verus_model::lemma_fit_zero(
                    w.coeff as int,
                    w.sticky,
                    w.exp as int,
                    W::LAYOUT,
                    k as int,
                    coeff as int,
                    rem as int,
                    pref as int,
                );
            }
            return Ok(InRange::unchecked(Unpacked {
                neg: w.neg,
                coeff: 0,
                exp: pref as i32,
            }));
        }
        let pref = i128::from(pref);
        #[cfg(verus_keep_ghost)]
        proof! {
            if exp <= pref {
                crate::verus_model::lemma_fit_exact(
                    w.coeff as int,
                    w.sticky,
                    w.exp as int,
                    W::LAYOUT,
                    k as int,
                    coeff as int,
                    rem as int,
                    pref as int,
                    0,
                    0,
                    coeff as int,
                    exp as int,
                );
            }
        }
        if exp > pref {
            #[cfg(verus_keep_ghost)]
            proof! {
                crate::verus_model::lemma_decimal_digits_le(coeff as nat, W::LAYOUT.digits as nat);
            }
            let shift = (exp - pref).min(i128::from(lay.digits - digits(coeff))) as u32;
            #[cfg(verus_keep_ghost)]
            proof_decl! {
                let ghost q: int = coeff as int;
                let ghost dq: int = crate::verus_model::decimal_digits(coeff as nat) as int;
                crate::verus_model::lemma_fit_scale(q, dq, W::LAYOUT, shift as int);
            }
            coeff *= pow10(shift);
            exp -= i128::from(shift);
            #[cfg(verus_keep_ghost)]
            proof! {
                crate::verus_model::lemma_fit_exact(
                    w.coeff as int,
                    w.sticky,
                    w.exp as int,
                    W::LAYOUT,
                    k as int,
                    q,
                    rem as int,
                    pref as int,
                    shift as int,
                    dq,
                    coeff as int,
                    exp as int,
                );
            }
        }
    } else {
        #[cfg(verus_keep_ghost)]
        proof! {
            crate::verus_model::lemma_fit_none(
                w.coeff as int,
                w.sticky,
                w.exp as int,
                W::LAYOUT,
                k as int,
                coeff as int,
                rem as int,
            );
            crate::verus_model::lemma_fit_down(
                w.coeff as int,
                w.sticky,
                w.exp as int,
                W::LAYOUT,
                k as int,
                coeff as int,
                rem as int,
            );
            crate::verus_model::lemma_fit_up(
                w.coeff as int,
                w.sticky,
                w.exp as int,
                W::LAYOUT,
                k as int,
                coeff as int,
                rem as int,
            );
        }
        let Some(dir) = dir else {
            return Err(
                if exp > i128::from(lay.qmax)
                    || (exp == i128::from(lay.qmax) && coeff >= pow10(lay.digits) - 1)
                {
                    Error::Overflow
                } else {
                    Error::Inexact
                },
            );
        };
        if (dir == Rounding::TowardPositive) != w.neg {
            coeff += 1;
            if coeff == pow10(lay.digits) {
                coeff = pow10(lay.digits - 1);
                exp += 1;
            }
        }
    }
    if exp > i128::from(lay.qmax) {
        return Err(Error::Overflow);
    }
    Ok(InRange::unchecked(Unpacked {
        neg: w.neg,
        coeff,
        exp: exp as i32,
    }))
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "finite decoded coefficients < 10^34; ordered i32 exponent difference fits i64; after the digit guard d <= 34 and scaled coefficient < 10^35 < 2^127"
)]
fn cmp_mag(a: Unpacked, b: Unpacked) -> Ordering {
    if a.coeff == 0 || b.coeff == 0 {
        return a.coeff.cmp(&b.coeff);
    }
    if a.exp < b.exp {
        return cmp_mag(b, a).reverse();
    }
    let d = i64::from(a.exp) - i64::from(b.exp);
    if i64::from(digits(a.coeff)) + d > 35 {
        Ordering::Greater
    } else {
        (a.coeff * pow10(d as u32)).cmp(&b.coeff)
    }
}

#[inline]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "P is 7/16/34 and both coefficients < 10^P; exponent difference is in -12287..=12287; guarded scaling has powers <= 38-P and coefficients < 10^38 < 2^127"
)]
pub(crate) fn eq<W: Width>(a: Unpacked, b: Unpacked) -> bool {
    let gap = a.exp - b.exp;
    let limit = (38 - W::LAYOUT.digits) as i32;
    if (-limit..=limit).contains(&gap) {
        let mut ac = a.coeff;
        let mut bc = b.coeff;
        if gap > 0 {
            ac *= pow10(gap as u32);
        } else if gap < 0 {
            bc *= pow10((-gap) as u32);
        }
        return (ac == bc) & ((a.neg == b.neg) | (ac == 0));
    }
    eq_general(a, b)
}

#[inline]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "finite decoded exponents are in -6176..=6111, so their difference and negation fit i32; the guarded absolute gap is <= 33, within the power table; checked multiplication rejects products outside u128"
)]
fn eq_general(a: Unpacked, b: Unpacked) -> bool {
    if a.coeff == 0 || b.coeff == 0 {
        return a.coeff == b.coeff;
    }
    if a.neg != b.neg {
        return false;
    }
    let gap = a.exp - b.exp;
    if gap > 0 && gap <= 33 {
        a.coeff.checked_mul(pow10(gap as u32)) == Some(b.coeff)
    } else if (-33..0).contains(&gap) {
        b.coeff.checked_mul(pow10((-gap) as u32)) == Some(a.coeff)
    } else {
        false
    }
}

const _: () = assert!(10_u128.pow(38) < (1_u128 << 127));

#[inline]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "P is 7/16/34 and both coefficients < 10^P; exponent difference is in -12287..=12287; guarded scaling uses powers <= 38-P and yields magnitudes < 10^38 < 2^127, so i128 conversion, negation and multiplication are exact"
)]
pub(crate) fn cmp<W: Width>(a: Unpacked, b: Unpacked) -> Ordering {
    let gap = a.exp - b.exp;
    let limit = (38 - W::LAYOUT.digits) as i32;
    if (-limit..=limit).contains(&gap) {
        let ac = a.coeff as i128;
        let bc = b.coeff as i128;
        let mut ac = if a.neg { -ac } else { ac };
        let mut bc = if b.neg { -bc } else { bc };
        if gap > 0 {
            ac *= pow10(gap as u32) as i128;
        } else if gap < 0 {
            bc *= pow10((-gap) as u32) as i128;
        }
        return ac.cmp(&bc);
    }
    cmp_general(a, b)
}

#[inline]
fn cmp_general(a: Unpacked, b: Unpacked) -> Ordering {
    cmp_general_parts(a.neg, a.coeff, a.exp, b.neg, b.coeff, b.exp)
}

#[inline(never)]
fn cmp_general_parts(
    a_neg: bool,
    a_coeff: u128,
    a_exp: i32,
    b_neg: bool,
    b_coeff: u128,
    b_exp: i32,
) -> Ordering {
    let a = Unpacked {
        neg: a_neg,
        coeff: a_coeff,
        exp: a_exp,
    };
    let b = Unpacked {
        neg: b_neg,
        coeff: b_coeff,
        exp: b_exp,
    };
    let a_neg = a.neg && a.coeff != 0;
    let b_neg = b.neg && b.coeff != 0;
    match a_neg.cmp(&b_neg) {
        Ordering::Equal => {
            let order = cmp_mag(a, b);
            if a_neg { order.reverse() } else { order }
        }
        order => order.reverse(),
    }
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "decoded coeff < 10^34 < 2^127, so strip_trailing_zeros applies; at most 33 zeros are added to an i32 exponent in -6176..=6111"
)]
#[cfg_attr(verus_keep_ghost, verus_spec(ret =>
    requires
        value.coeff < 0x8000_0000_0000_0000_0000_0000_0000_0000u128,
        value.exp <= i32::MAX - 38,
    ensures
        value.coeff == 0 ==> ret == (false, 0u128, 0i32),
        value.coeff != 0 ==> {
            &&& ret.0 == value.neg
            &&& ret.2 >= value.exp
            &&& value.coeff as int == ret.1 as int * vstd::arithmetic::power::pow(10, (ret.2 - value.exp) as nat)
            &&& ret.1 % 10 != 0
        },
))]
pub(crate) fn canonical(value: Unpacked) -> (bool, u128, i32) {
    if value.coeff == 0 {
        return (false, 0, 0);
    }
    let (coeff, zeros) = if value.coeff < 10_000_000_000_000_000 {
        let (coeff, zeros) = strip_trailing_zeros_64(value.coeff as u64);
        (u128::from(coeff), zeros)
    } else {
        strip_trailing_zeros(value.coeff)
    };
    (value.neg, coeff, value.exp + zeros as i32)
}

const _: () = assert!((10_u128.pow(34) - 1) * 10_001 < (1_u128 << 127));

#[inline]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "decoded exponents differ by at most 12287; the guarded gap is <= 4, powers <= 10^4, and signed scaled coefficients and sum have magnitude <= (10^34-1)*10001 < 2^127 (checked at compile time); the checked constructor establishes the result layout bounds"
)]
#[cfg_attr(verus_keep_ghost, verus_spec(ret =>
    requires
        crate::layout::bid_width(W::LAYOUT),
        crate::layout::in_range(a, crate::layout::W128::LAYOUT),
        crate::layout::in_range(b, crate::layout::W128::LAYOUT),
    ensures
        crate::verus_model::rounded(
            ret,
            crate::verus_model::sum_sign(a, b, subtract, dir),
            crate::verus_model::abs_int(crate::verus_model::exact_sum(a, b, subtract)),
            crate::verus_model::min_exp(a, b),
            crate::verus_model::min_exp(a, b),
            dir,
        ),
))]
pub(crate) fn add_sub<W: Width>(
    a: Unpacked,
    b: Unpacked,
    subtract: bool,
    dir: Option<Rounding>,
) -> Result<InRange<W>, Error> {
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::verus_model::lemma_add_input::<W>(a, b);
        vstd::arithmetic::power::lemma_pow0(10);
    }
    let gap = a.exp - b.exp;
    if (-4..=4).contains(&gap) {
        let b_neg = b.neg ^ subtract;
        let ac = a.coeff as i128;
        let bc = b.coeff as i128;
        let mut ac = if a.neg { -ac } else { ac };
        let mut bc = if b_neg { -bc } else { bc };
        #[cfg(verus_keep_ghost)]
        proof_decl! {
            let ghost bc0: int = bc as int;
        }
        if gap > 0 {
            #[cfg(verus_keep_ghost)]
            proof! { crate::verus_model::lemma_add_fast_scale(ac as int, gap as nat); }
            ac *= pow10(gap as u32) as i128;
        } else if gap < 0 {
            #[cfg(verus_keep_ghost)]
            proof! { crate::verus_model::lemma_add_fast_scale(bc as int, (-gap) as nat); }
            bc *= pow10((-gap) as u32) as i128;
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            use crate::verus_model::*;
            use vstd::arithmetic::power::*;
            lemma_pow0(10);
            if gap >= 0 {
                assert(bc as int == bc0);
                assert(pow(10, (b.exp as int - min_exp(a, b)) as nat) == 1);
                assert(bc0 * pow(10, 0) == bc0) by (nonlinear_arith)
                    requires pow(10, 0) == 1;
            } else {
                assert(bc as int == bc0 * pow(10, (-gap) as nat));
            }
            lemma_add_signed_product(a.neg, a.coeff as int, a.exp as int, min_exp(a, b), ac as int);
            lemma_add_signed_product(b_neg, b.coeff as int, b.exp as int, min_exp(a, b), bc as int);
        }
        let sum = ac + bc;
        let negative_zero = zero_sign(a.neg, b_neg, dir);
        if let Some(value) = InRange::checked(
            sum < 0 || (sum == 0 && negative_zero),
            sum.unsigned_abs(),
            i128::from(a.exp.min(b.exp)),
        ) {
            #[cfg(verus_keep_ghost)]
            proof! {
                use crate::verus_model::*;
                let f = min_exp(a, b);
                let m = abs_int(exact_sum(a, b, subtract));
                assert(sum as int == exact_sum(a, b, subtract));
                lemma_scaled_same(m, m, f);
                lemma_exact_bounds(m, f, m, f, W::LAYOUT);
                assert(clamp_exp(f, W::LAYOUT) == f);
                assert(rounded(Ok(value), sum_sign(a, b, subtract, dir), m, f, f, dir));
            }
            return Ok(value);
        }
    }
    add_sub_general(a, b, subtract, dir)
}

#[inline]
#[cfg_attr(verus_keep_ghost, verus_spec(ret =>
    ensures
        ret == if a_neg == b_neg { a_neg } else { dir == Some(Rounding::TowardNegative) },
))]
fn zero_sign(a_neg: bool, b_neg: bool, dir: Option<Rounding>) -> bool {
    if a_neg == b_neg {
        a_neg
    } else {
        dir == Some(Rounding::TowardNegative)
    }
}

#[inline(never)]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "decoded coefficients < 10^34 < 2^127 and exponents in -6176..=6111; ordered d <= 12287. Exact guard gives d <= 36 and A*10^d < 10^37, sum < 10^37+10^34 < 2^123; ordered subtraction cannot underflow. Stripping B removes <= 33 zeros, capped at d; the capped reciprocal division is exact with input < 10^34 < 2^127 and pref <= qb'. Compression uses e=35-digits(A) in 1..=34, g=d'-e >= 3, B' < 10^34 < 2^127; s_hi < 10^31, A*10^e in [10^34,10^35), so subtraction is positive and 10^34-10^31 <= H < 10^36 < 2^127. Sticky therefore has H >= 10^33 >= 10^(P-1) for all result precisions. Every Wide exponent >= pref; exact paths have no sticky"
)]
#[cfg_attr(verus_keep_ghost, verus_spec(ret =>
    requires
        crate::layout::bid_width(W::LAYOUT),
        crate::layout::in_range(a, crate::layout::W128::LAYOUT),
        crate::layout::in_range(b, crate::layout::W128::LAYOUT),
    ensures
        crate::verus_model::rounded(
            ret,
            crate::verus_model::sum_sign(a, b, subtract, dir),
            crate::verus_model::abs_int(crate::verus_model::exact_sum(a, b, subtract)),
            crate::verus_model::min_exp(a, b),
            crate::verus_model::min_exp(a, b),
            dir,
        ),
))]
#[cfg_attr(verus_keep_ghost, verifier::rlimit(60))]
fn add_sub_general<W: Width>(
    mut a: Unpacked,
    mut b: Unpacked,
    subtract: bool,
    dir: Option<Rounding>,
) -> Result<InRange<W>, Error> {
    #[cfg(verus_keep_ghost)]
    proof_decl! {
        let ghost f = crate::verus_model::min_exp(a, b);
        let ghost s = crate::verus_model::exact_sum(a, b, subtract);
        let ghost target_neg = crate::verus_model::sum_sign(a, b, subtract, dir);
        crate::verus_model::lemma_add_input::<W>(a, b);
        vstd::arithmetic::power::lemma_pow0(10);
    }
    b.neg ^= subtract;
    let pref = i64::from(a.exp.min(b.exp));
    let negative_zero = zero_sign(a.neg, b.neg, dir);
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(s == crate::verus_model::signed_at(a.neg, a.coeff as int, a.exp as int, f) + crate::verus_model::signed_at(b.neg, b.coeff as int, b.exp as int, f));
        assert(target_neg == (s < 0 || (s == 0 && negative_zero)));
        assert(0int * vstd::arithmetic::power::pow(10, (a.exp as int - f) as nat) == 0) by (nonlinear_arith);
        assert(0int * vstd::arithmetic::power::pow(10, (b.exp as int - f) as nat) == 0) by (nonlinear_arith);
    }
    let w = if a.coeff == 0 && b.coeff == 0 {
        #[cfg(verus_keep_ghost)]
        proof! {
            crate::verus_model::lemma_add_exact_model(negative_zero, 0, pref as int, f, s, negative_zero, W::LAYOUT);
        }
        Wide {
            neg: negative_zero,
            coeff: 0,
            exp: pref,
            sticky: false,
        }
    } else if a.coeff == 0 || b.coeff == 0 {
        let v = if a.coeff == 0 { b } else { a };
        #[cfg(verus_keep_ghost)]
        proof! {
            vstd::arithmetic::power::lemma_pow_positive(10, (v.exp as int - f) as nat);
            vstd::arithmetic::mul::lemma_mul_strictly_positive(v.coeff as int, vstd::arithmetic::power::pow(10, (v.exp as int - f) as nat));
            crate::verus_model::lemma_add_exact_model(v.neg, v.coeff as int, v.exp as int, f, s, negative_zero, W::LAYOUT);
        }
        Wide {
            neg: v.neg,
            coeff: v.coeff,
            exp: i64::from(v.exp),
            sticky: false,
        }
    } else {
        if a.exp < b.exp {
            core::mem::swap(&mut a, &mut b);
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(s == crate::verus_model::signed_at(a.neg, a.coeff as int, a.exp as int, f) + crate::verus_model::signed_at(b.neg, b.coeff as int, b.exp as int, f));
            assert(negative_zero == if a.neg == b.neg { a.neg } else { dir == Some(Rounding::TowardNegative) });
            crate::verus_model::lemma_decimal_digits_le(a.coeff as nat, 34);
        }
        let mut d = (a.exp - b.exp) as u32;
        let n = digits(a.coeff);
        if n + d > 37 {
            #[cfg(verus_keep_ghost)]
            proof_decl! {
                let ghost old_b = b;
            }
            let (stripped, zeros) = strip_trailing_zeros(b.coeff);
            let z = zeros.min(d);
            #[cfg(verus_keep_ghost)]
            proof! {
                vstd::arithmetic::power::lemma_pow_positive(10, zeros as nat);
                assert(0 < stripped <= b.coeff) by (nonlinear_arith)
                    requires b.coeff as int == stripped as int * vstd::arithmetic::power::pow(10, zeros as nat), b.coeff > 0, vstd::arithmetic::power::pow(10, zeros as nat) >= 1;
            }
            b.coeff = if z == zeros {
                stripped
            } else {
                #[cfg(verus_keep_ghost)]
                proof! { vstd::arithmetic::power::lemma_pow_positive(10, z as nat); }
                div_rem_pow10(b.coeff, z).0
            };
            #[cfg(verus_keep_ghost)]
            proof! {
                if z != zeros {
                    let rem = old_b.coeff as int - b.coeff as int * vstd::arithmetic::power::pow(10, z as nat);
                    crate::verus_model::lemma_add_strip(old_b.coeff as int, stripped as int, zeros as nat, z as nat, b.coeff as int, rem);
                }
                crate::verus_model::lemma_add_signed_scale(b.neg, b.coeff as int, old_b.exp as int, f, z as nat);
            }
            b.exp += z as i32;
            d -= z;
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(s == crate::verus_model::signed_at(a.neg, a.coeff as int, a.exp as int, f) + crate::verus_model::signed_at(b.neg, b.coeff as int, b.exp as int, f));
                assert(n + d > 37 ==> b.coeff % 10 != 0);
            }
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(1 <= n <= 34);
            assert(n + d > 37 ==> b.coeff % 10 != 0);
            assert(d == a.exp - b.exp);
        }
        if n + d <= 37 {
            #[cfg(verus_keep_ghost)]
            proof! {
                crate::verus_model::lemma_add_exact_scale(a.coeff as int, n as nat, d as nat);
                crate::verus_model::lemma_add_signed_scale(a.neg, a.coeff as int, b.exp as int, f, d as nat);
            }
            let scaled = a.coeff * pow10(d);
            let (neg, coeff) = if a.neg == b.neg {
                (a.neg, scaled + b.coeff)
            } else {
                match scaled.cmp(&b.coeff) {
                    Ordering::Greater => (a.neg, scaled - b.coeff),
                    Ordering::Less => (b.neg, b.coeff - scaled),
                    Ordering::Equal => (negative_zero, 0),
                }
            };
            #[cfg(verus_keep_ghost)]
            proof! {
                crate::verus_model::lemma_add_exact_sum(a.neg, b.neg, scaled as int, b.coeff as int, b.exp as int, f, s, negative_zero, neg, coeff as int, W::LAYOUT);
            }
            Wide {
                neg,
                coeff,
                exp: i64::from(b.exp),
                sticky: false,
            }
        } else {
            let e = 35 - n;
            let g = d - e;
            let (hi, _) = div_rem_pow10(b.coeff, g);
            #[cfg(verus_keep_ghost)]
            proof! {
                use crate::verus_model::*;
                use vstd::arithmetic::power::*;
                lemma_add_compress_bounds(b.coeff as int, g as nat, hi as int, b.coeff as int - hi as int * pow(10, g as nat));
                lemma_add_compress_scale(a.coeff as int, n as nat, e as nat);
                lemma_add_signed_scale(a.neg, a.coeff as int, b.exp as int + g as int, f, e as nat);
            }
            let scaled = a.coeff * pow10(e);
            let coeff = if a.neg == b.neg {
                scaled + hi
            } else {
                scaled - hi - 1
            };
            #[cfg(verus_keep_ghost)]
            proof! {
                crate::verus_model::lemma_add_compress_model(a.neg, b.neg, scaled as int, b.coeff as int, b.exp as int, f, g as nat, hi as int,
                    b.coeff as int - hi as int * vstd::arithmetic::power::pow(10, g as nat), coeff as int, s, W::LAYOUT);
            }
            Wide {
                neg: a.neg,
                coeff,
                exp: i64::from(b.exp) + i64::from(g),
                sticky: true,
            }
        }
    };
    #[cfg(verus_keep_ghost)]
    proof! { crate::verus_model::lemma_add_fit_transfer::<W>(); }
    fit(w, pref, dir)
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "decoded coefficients are < 10^34 < 2^113; after the swap the fast path runs only when a < 2^64, so the u64 cast is exact and b's high limb < 2^49 fits u64; two u64 carrying_mul steps reconstruct the exact 192-bit product, and the 64-bit shift in u128 joins disjoint limbs; exponent sum in -12352..=12222 fits i64, and the checked constructor establishes the result layout bounds"
)]
#[inline]
pub(crate) fn mul<W: Width>(
    a: Unpacked,
    b: Unpacked,
    dir: Option<Rounding>,
) -> Result<InRange<W>, Error> {
    let (a, b) = if a.coeff >> 64 == 0 { (a, b) } else { (b, a) };
    if a.coeff >> 64 == 0 {
        let pref = i64::from(a.exp) + i64::from(b.exp);
        let neg = a.neg ^ b.neg;
        let a_coeff = a.coeff as u64;
        let (low, carry) = a_coeff.carrying_mul(b.coeff as u64, 0);
        let (middle, high) = a_coeff.carrying_mul((b.coeff >> 64) as u64, carry);
        let coeff = (u128::from(middle) << 64) | u128::from(low);
        if high == 0
            && let Some(value) = InRange::checked(neg, coeff, i128::from(pref))
        {
            return Ok(value);
        }
    }
    mul_general(a, b, dir)
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "decoded coefficients a,b < 10^34 < 2^127; i64 sum of decoded exponents in -12352..=12222 fits. Splitting each at 10^17 gives limbs < 10^17 < 2^57; partial products p00,p11 < 10^34 and p01 < 2*10^34, so p01+c0 < 2*10^34+10^17 < 2^127. The product is H*10^34+R with H=p11+c1 < 10^34 because a*b < 10^68, and R=l1*10^17+l0 < 10^34. For H=0 the coefficient is R. Otherwise sh=digits(H)-1 is in 0..=33, H*10^(34-sh) < 10^35 and adding R/10^sh < 10^(34-sh) keeps the coefficient < 10^35 < 2^127; it is >= 10^34, satisfying sticky's coeff >= 10^(P-1) premise for every P <= 34. Every power index <= 34 <= 38; every Wide exp=pref+sh >= pref, and adding sh <= 33 fits i64"
)]
#[inline(never)]
fn mul_general<W: Width>(
    a: Unpacked,
    b: Unpacked,
    dir: Option<Rounding>,
) -> Result<InRange<W>, Error> {
    let pref = i64::from(a.exp) + i64::from(b.exp);
    let neg = a.neg ^ b.neg;
    let w = if a.coeff == 0 || b.coeff == 0 {
        Wide {
            neg,
            coeff: 0,
            exp: pref,
            sticky: false,
        }
    } else {
        let (a1, a0) = div_rem_pow10(a.coeff, 17);
        let (b1, b0) = div_rem_pow10(b.coeff, 17);
        let p00 = a0 * b0;
        let p01 = a0 * b1 + a1 * b0;
        let p11 = a1 * b1;
        let (c0, l0) = div_rem_pow10(p00, 17);
        let (c1, l1) = div_rem_pow10(p01 + c0, 17);
        let h = p11 + c1;
        let r = l1 * pow10(17) + l0;
        if h == 0 {
            Wide {
                neg,
                coeff: r,
                exp: pref,
                sticky: false,
            }
        } else {
            let sh = digits(h) - 1;
            let (t, s) = div_rem_pow10(r, sh);
            Wide {
                neg,
                coeff: h * pow10(34 - sh) + t,
                exp: pref + i64::from(sh),
                sticky: s != 0,
            }
        }
    };
    fit(w, pref, dir)
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "decoded coefficients a,b < 10^34, and b != 0 after the zero checks; the i64 difference of decoded exponents is in -12287..=12287. digits(b) <= 34 and P <= 34, so k <= P+33 <= 67. For k > 0, 10^(P-1) <= a*10^k/b < 10^(P+1); for k = 0, digits(a) >= P+digits(b) gives 10^(P-1) <= a/b < 10^34. So a*10^k < b*10^35, the quotient is in 10^(P-1)..10^35 < 2^127 as fit requires with sticky, and exp = pref-k and exp+zeros (zeros <= 38) fit i64"
)]
#[inline(never)]
pub(crate) fn div<W: Width>(
    a: Unpacked,
    b: Unpacked,
    dir: Option<Rounding>,
) -> Result<InRange<W>, Error> {
    if b.coeff == 0 {
        return Err(if a.coeff == 0 {
            Error::InvalidInput
        } else {
            Error::DivisionByZero
        });
    }
    let pref = i64::from(a.exp) - i64::from(b.exp);
    let neg = a.neg ^ b.neg;
    let w = if a.coeff == 0 {
        Wide {
            neg,
            coeff: 0,
            exp: pref,
            sticky: false,
        }
    } else {
        let k = (W::LAYOUT.digits + digits(b.coeff)).saturating_sub(digits(a.coeff));
        let exp = pref - i64::from(k);
        let (q, r) = div_rem_scaled(a.coeff, k, b.coeff);
        if r == 0 {
            let (q, zeros) = strip_trailing_zeros(q);
            Wide {
                neg,
                coeff: q,
                exp: exp + i64::from(zeros),
                sticky: false,
            }
        } else {
            Wide {
                neg,
                coeff: q,
                exp,
                sticky: true,
            }
        }
    };
    let pref_exp = pref.min(w.exp);
    fit(w, pref_exp, dir)
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "b > 0 and digits(b) <= 34, so step = 38-digits(b) is in 4..=37 and every chunk c <= step indexes the power table. r < b < 10^digits(b) gives r*10^c < 10^38 < 2^128. The precondition a*10^k < b*10^35 keeps q*10^left < 10^35, so q*10^c + t/b < 10^35 + 10^37 < 2^128; c <= left"
)]
#[cfg_attr(verus_keep_ghost, verus_spec(ret =>
    requires
        a < 10_000_000_000_000_000_000_000_000_000_000_000u128,
        0 < b < 10_000_000_000_000_000_000_000_000_000_000_000u128,
        (a as int) * vstd::arithmetic::power::pow(10, k as nat) < (b as int) * vstd::arithmetic::power::pow(10, 35),
    ensures
        ret.0 as int == (a as int * vstd::arithmetic::power::pow(10, k as nat)) / (b as int),
        ret.1 as int == (a as int * vstd::arithmetic::power::pow(10, k as nat)) % (b as int),
))]
fn div_rem_scaled(a: u128, k: u32, b: u128) -> (u128, u128) {
    #[cfg(verus_keep_ghost)]
    proof! { crate::verus_model::lemma_div_scaled_start(a as int, b as int, crate::verus_model::decimal_digits(b as nat)); }
    let step = 38 - digits(b);
    let mut q = a / b;
    let mut r = a % b;
    let mut left = k;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            left <= k,
            0 < b < 10_000_000_000_000_000_000_000_000_000_000_000u128,
            4 <= step <= 37,
            (b as int) < vstd::arithmetic::power::pow(10, (38 - step) as nat),
            (a as int) * vstd::arithmetic::power::pow(10, k as nat) < (b as int) * vstd::arithmetic::power::pow(10, 35),
            r < b,
            q as int * b as int + r as int == a as int * vstd::arithmetic::power::pow(10, (k - left) as nat),
        decreases left,
    ))]
    while left > 0 {
        let c = left.min(step);
        #[cfg(verus_keep_ghost)]
        proof! {
            crate::verus_model::lemma_div_scaled_step(a as int, b as int, k as nat, (k - left) as nat, c as nat, step as nat, q as int, r as int);
            assert((k - left) as nat + c as nat == (k - (left - c)) as nat);
        }
        let t = r * pow10(c);
        q = q * pow10(c) + t / b;
        r = t % b;
        left -= c;
    }
    #[cfg(verus_keep_ghost)]
    proof! { crate::verus_model::lemma_div_scaled_end(a as int, b as int, k as nat, q as int, r as int); }
    (q, r)
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "decoded coeff < 10^P < 2^127, P in {7,16,34}, decoded and checked target exponents in -6176..=6111, so differences fit i32. Zero bypasses scaling; the digit guard bounds shift <= P-1 <= 33 before pow10 and the product < 10^P. Division receives coeff < 2^127 and k >= 1; quotient <= (10^P-1)/10, so even its increment < 10^P. The returned exponent is the checked target"
)]
pub(crate) fn quantize<W: Width>(
    value: InRange<W>,
    exp: i32,
    dir: Rounding,
) -> Result<InRange<W>, Error> {
    let lay = &W::LAYOUT;
    let value = value.unpacked();
    if !(lay.qmin..=lay.qmax).contains(&exp) {
        return Err(Error::InvalidInput);
    }
    let coeff = if value.coeff == 0 {
        0
    } else if value.exp >= exp {
        let shift = (value.exp - exp) as u32;
        if digits(value.coeff) + shift > lay.digits {
            return Err(Error::InvalidInput);
        }
        value.coeff * pow10(shift)
    } else {
        let (q, r) = div_rem_pow10(value.coeff, (exp - value.exp) as u32);
        q + u128::from(r != 0 && (dir == Rounding::TowardPositive) != value.neg)
    };
    Ok(InRange::unchecked(Unpacked {
        neg: value.neg,
        coeff,
        exp,
    }))
}

#[inline]
pub(crate) fn round_to_multiple<W: Width>(
    x: InRange<W>,
    step: InRange<W>,
    dir: Rounding,
) -> Result<InRange<W>, Error> {
    let x = x.unpacked();
    let step = step.unpacked();
    if step.coeff == 0 {
        return Err(Error::DivisionByZero);
    }
    if step.neg {
        return Err(Error::InvalidInput);
    }
    if W::LAYOUT.digits <= W64::LAYOUT.digits {
        let (unit, zeros) = strip_trailing_zeros_64(step.coeff as u64);
        if unit == 1 || unit == 5 {
            return round_to_unit_multiple_64::<W>(x, step.exp, unit, zeros, dir);
        }
    }
    round_to_multiple_general::<W>(x, step, dir)
}

#[inline]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "the caller runs this only for Decimal32/64 (P <= 16) with a positive step unit*10^zeros, unit in {1,5}; decoded coefficients are < 10^P <= 10^16 < 2^64, so the u64 casts are exact; a step < 10^16 strips at most 15 zeros, so with exponents in -398..=369 the stripped step exponent is in -398..=384 and E in -783..=767, fitting i32; N = c_x*(1|2) < 2*10^16 < 2^63 satisfies the 64-bit division premise; for E<0 the directed quotient satisfies u*I <= c_x+4 < 10^P+4 < 2^64; the only multiple of five above 10^P-1 under that bound is 10^P, reduced exactly to 10^(P-1) at the next exponent, so afterwards the coefficient is < 10^P, P - digits >= 0, digits(c) <= P makes the saturating difference exact, the shift is at most P-1 <= 15 and the scaled coefficient stays < 10^P; every branch has exponent >= the step exponent >= qmin before the range test"
)]
fn round_to_unit_multiple_64<W: Width>(
    x: Unpacked,
    step_exp: i32,
    unit: u64,
    zeros: u32,
    dir: Rounding,
) -> Result<InRange<W>, Error> {
    let lay = &W::LAYOUT;
    let unit_exp = step_exp + zeros as i32;
    let scaled_exp = x.exp - unit_exp - i32::from(unit == 5);
    let (mut coeff, mut exp) = if scaled_exp >= 0 {
        (x.coeff as u64, x.exp)
    } else {
        let numerator = x.coeff as u64 * if unit == 5 { 2 } else { 1 };
        let (q, r) = div_rem_pow10_64(numerator, (-scaled_exp) as u32);
        let integer = q + u64::from(r != 0 && (dir == Rounding::TowardPositive) != x.neg);
        (unit * integer, unit_exp)
    };
    if coeff == pow10_64(lay.digits) {
        coeff = pow10_64(lay.digits - 1);
        exp += 1;
    }
    if coeff == 0 {
        return Ok(InRange::unchecked(Unpacked {
            neg: x.neg,
            coeff: 0,
            exp: step_exp,
        }));
    }
    if exp > step_exp {
        let shift = (exp - step_exp).min(lay.digits.saturating_sub(digits_64(coeff)) as i32) as u32;
        coeff *= pow10_64(shift);
        exp -= shift as i32;
    }
    if exp > lay.qmax {
        return Err(Error::Overflow);
    }
    Ok(InRange::unchecked(Unpacked {
        neg: x.neg,
        coeff: u128::from(coeff),
        exp,
    }))
}

#[inline(never)]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "decoded coefficients cx, cs are < 10^P <= 10^34 < 2^113 with cs > 0; decoded exponents are in -6176..=6111, so d, g <= 12287 and digit sums fit u32. 0 < r < cs, so cs - r cannot underflow, and the down branch subtracts t = r <= cx*10^d scaled by the same 10^z. z <= min(33, d). The representable branch has digits(cx) + d - z <= 38, so cx*10^(d-z) < 10^38 and adding t/10^z < 10^34 stays < 2^127. In the d < 0 branch, g >= 1 and digits(cs) + g <= 38 bounds S = cs*10^g < 10^38 with S > 0; k is the quotient cx/S rounded down plus at most one, so k <= cx/S + 1 and k*cs <= cx/10^g + cs < 2*10^34; otherwise k <= 1 and k*cs < 10^34. Every power index is <= 38, the Wide exponent pref + z or pref fits i64 and is >= pref, and the Wide coefficient < 2^127 is never sticky"
)]
fn round_to_multiple_general<W: Width>(
    x: Unpacked,
    step: Unpacked,
    dir: Rounding,
) -> Result<InRange<W>, Error> {
    let pref = i64::from(step.exp);
    let up = (dir == Rounding::TowardPositive) != x.neg;
    let (cx, cs) = (x.coeff, step.coeff);
    let exact = Wide {
        neg: x.neg,
        coeff: cx,
        exp: i64::from(x.exp),
        sticky: false,
    };
    let w = if cx == 0 {
        Wide {
            neg: x.neg,
            coeff: 0,
            exp: pref,
            sticky: false,
        }
    } else if x.exp >= step.exp {
        let d = (x.exp - step.exp) as u32;
        if d >= 113 {
            if !scaled_is_multiple(cx, cs) {
                return Err(if up && is_largest_finite::<W>(cx, x.exp) {
                    Error::Overflow
                } else {
                    Error::Inexact
                });
            }
            exact
        } else {
            let r = scaled_rem(cx, d, cs);
            if r == 0 {
                exact
            } else {
                let t = if up { cs - r } else { r };
                let z = strip_trailing_zeros(t).1.min(d);
                if digits(cx) + (d - z) > 38 {
                    return Err(if up && exceeds_max::<W>(cx, x.exp, d, t) {
                        Error::Overflow
                    } else {
                        Error::Inexact
                    });
                }
                let scaled = cx * pow10(d - z);
                let t = div_rem_pow10(t, z).0;
                Wide {
                    neg: x.neg,
                    coeff: if up { scaled + t } else { scaled - t },
                    exp: pref + i64::from(z),
                    sticky: false,
                }
            }
        }
    } else {
        let g = (step.exp - x.exp) as u32;
        let k = if digits(cs) + g <= 38 {
            let s = cs * pow10(g);
            u128::from(up && cx % s != 0) + cx / s
        } else {
            u128::from(up)
        };
        Wide {
            neg: x.neg,
            coeff: k * cs,
            exp: pref,
            sticky: false,
        }
    };
    fit(w, pref, None)
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "callers pass d < 113 and cx, cs < 10^34 < 2^113 with cs > 0, so every remainder is by a nonzero divisor; with digits(cx) + d <= 38 the direct product is < 10^38 < 2^128; in the loop r < cs < 10^34 and r*10^k <= r*10^4 < 10^38; left only decreases by k <= left"
)]
#[cfg_attr(verus_keep_ghost, verus_spec(ret =>
    requires
        d < 113,
        0 < cs < 10_000_000_000_000_000_000_000_000_000_000_000u128,
        cx < 10_000_000_000_000_000_000_000_000_000_000_000u128,
    ensures
        ret as int == (cx as int * vstd::arithmetic::power::pow(10, d as nat)) % (cs as int),
))]
fn scaled_rem(cx: u128, d: u32, cs: u128) -> u128 {
    if digits(cx) + d <= 38 {
        #[cfg(verus_keep_ghost)]
        proof! { crate::verus_model::lemma_scaled_rem_direct(cx as int, d as nat, crate::verus_model::decimal_digits(cx as nat)); }
        return cx * pow10(d) % cs;
    }
    let mut r = cx % cs;
    let mut left = d;
    #[cfg(verus_keep_ghost)]
    proof! { crate::verus_model::lemma_scaled_rem_start(cx as int, cs as int); }
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            left <= d,
            0 < cs < 10_000_000_000_000_000_000_000_000_000_000_000u128,
            r < cs,
            r as int == (cx as int * vstd::arithmetic::power::pow(10, (d - left) as nat)) % (cs as int),
        decreases left,
    ))]
    while left > 0 {
        let k = left.min(4);
        #[cfg(verus_keep_ghost)]
        proof! {
            crate::verus_model::lemma_scaled_rem_step(cx as int, cs as int, (d - left) as nat, k as nat, r as int);
            assert((d - left) as nat + k as nat == (d - (left - k)) as nat);
        }
        r = r * pow10(k) % cs;
        left -= k;
    }
    r
}

#[cfg_attr(verus_keep_ghost, verus_spec(ret =>
    requires
        0 < cs < 10_000_000_000_000_000_000_000_000_000_000_000u128,
    ensures
        forall|d: nat| #![trigger vstd::arithmetic::power::pow(10, d)]
            d >= 113 ==> (ret <==> (cx as int * vstd::arithmetic::power::pow(10, d)) % (cs as int) == 0),
))]
fn scaled_is_multiple(cx: u128, cs: u128) -> bool {
    let mut odd = cs >> cs.trailing_zeros();
    #[cfg(verus_keep_ghost)]
    proof_decl! {
        assert(exists|a: u32| #![trigger vstd::arithmetic::power2::pow2(a as nat)]
            a < 128 && odd == cs >> a && (cs as int) % (vstd::arithmetic::power2::pow2(a as nat) as int) == 0
                && ((cs as int) / (vstd::arithmetic::power2::pow2(a as nat) as int)) % 2 == 1);
        let ghost twos: u32 = choose|a: u32| #![trigger vstd::arithmetic::power2::pow2(a as nat)]
            a < 128 && odd == cs >> a && (cs as int) % (vstd::arithmetic::power2::pow2(a as nat) as int) == 0
                && ((cs as int) / (vstd::arithmetic::power2::pow2(a as nat) as int)) % 2 == 1;
        crate::verus_model::lemma_odd_part_start(cs, twos, odd);
        let ghost mut fives: nat = 0;
    }
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            0 < odd,
            odd % 2 == 1,
            0 < cs < 10_000_000_000_000_000_000_000_000_000_000_000u128,
            cs as int == (vstd::arithmetic::power2::pow2(twos as nat) as int) * vstd::arithmetic::power::pow(5, fives) * odd as int,
        decreases odd,
    ))]
    while odd.is_multiple_of(5) {
        #[cfg(verus_keep_ghost)]
        proof! { crate::verus_model::lemma_odd_part_step(cs as int, twos as nat, fives, odd as int); }
        odd /= 5;
        #[cfg(verus_keep_ghost)]
        proof! { fives = fives + 1; }
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::verus_model::lemma_scaled_is_multiple_bounds(cs as int, twos as nat, fives, odd as int);
        assert forall|d: nat| #![trigger vstd::arithmetic::power::pow(10, d)]
            d >= 113 implies ((cx as int * vstd::arithmetic::power::pow(10, d)) % (cs as int) == 0 <==> (cx as int) % (odd as int) == 0) by {
            crate::verus_model::lemma_scaled_is_multiple(cx as int, cs as int, twos as nat, fives, odd as int, d);
        }
    }
    cx.is_multiple_of(odd)
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "P is 7, 16 or 34, so 10^P - 1 does not underflow"
)]
#[cfg_attr(verus_keep_ghost, verus_spec(ret =>
    requires
        crate::layout::bid_width(W::LAYOUT),
    ensures
        ret == (x_exp == W::LAYOUT.qmax && cx as int == vstd::arithmetic::power::pow(10, W::LAYOUT.digits as nat) - 1),
))]
fn is_largest_finite<W: Width>(cx: u128, x_exp: i32) -> bool {
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::layout::lemma_layout_values();
        vstd::arithmetic::power::lemma_pow_positive(10, W::LAYOUT.digits as nat);
    }
    let lay = &W::LAYOUT;
    x_exp == lay.qmax && cx == pow10(lay.digits) - 1
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "cx <= 10^P - 1, so the subtraction cannot underflow; the power index d is checked <= 38 and the product is checked"
)]
#[cfg_attr(verus_keep_ghost, verus_spec(ret =>
    requires
        crate::layout::bid_width(W::LAYOUT),
        cx as int <= vstd::arithmetic::power::pow(10, W::LAYOUT.digits as nat) - 1,
    ensures
        ret == (x_exp == W::LAYOUT.qmax && (
            cx as int == vstd::arithmetic::power::pow(10, W::LAYOUT.digits as nat) - 1
            || (d <= 38 && t as int > (vstd::arithmetic::power::pow(10, W::LAYOUT.digits as nat) - 1 - cx)
                * vstd::arithmetic::power::pow(10, d as nat)))),
))]
fn exceeds_max<W: Width>(cx: u128, x_exp: i32, d: u32, t: u128) -> bool {
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::layout::lemma_layout_values();
        vstd::arithmetic::power::lemma_pow_positive(10, W::LAYOUT.digits as nat);
    }
    let lay = &W::LAYOUT;
    is_largest_finite::<W>(cx, x_exp)
        || (x_exp == lay.qmax
            && d <= 38
            && match (pow10(lay.digits) - 1 - cx).checked_mul(pow10(d)) {
                Some(limit) => t > limit,
                None => false,
            })
}

#[cfg(kani)]
#[path = "kernel_proofs.rs"]
mod proofs;
