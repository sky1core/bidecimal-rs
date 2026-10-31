#![deny(clippy::arithmetic_side_effects)]

use crate::kernel::{digits_64, div_rem_pow10_64, strip_trailing_zeros_64};
use crate::layout::{InRange, Unpacked, Width};
use crate::tables::pow10_64;
use crate::{Error, Rounding};

#[cfg(verus_keep_ghost)]
use crate::verus_model::*;
#[cfg(verus_keep_ghost)]
use vstd::arithmetic::power::*;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy)]
struct Finite64 {
    neg: bool,
    coeff: u64,
    exp: i32,
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
struct Wide64 {
    neg: bool,
    coeff: u64,
    exp: i32,
    sticky: bool,
}

#[cfg(verus_keep_ghost)]
verus! {
    impl Finite64 {
        spec fn view(self) -> Unpacked {
            Unpacked { neg: self.neg, coeff: self.coeff as u128, exp: self.exp }
        }
    }
}

#[inline]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "canonical W32/W64 inputs c<10^16; gap<=18 and high<10^(18-gap) bound aligned c<10^18, sum<10^18+10^16<2^63; exponents -398..=369"
)]
#[cfg_attr(verus_keep_ghost, verus_spec(ret =>
    requires
        a.coeff < 10_000_000_000_000_000u64,
        b.coeff < 10_000_000_000_000_000u64,
        -398 <= a.exp <= 369,
        -398 <= b.exp <= 369,
    ensures
        ret.is_some() == ({
            let gap = a.exp as int - b.exp as int;
            let distance = crate::verus_model::abs_int(gap);
            distance <= 18 && (if gap >= 0 { a.coeff as int } else { b.coeff as int })
                < vstd::arithmetic::power::pow(10, (18 - distance) as nat)
        }),
        match ret {
            Some(w) => {
                let s = crate::verus_model::exact_sum(a.view(), b.view(), false);
                &&& w.coeff as int == crate::verus_model::abs_int(s)
                &&& w.coeff < 1_010_000_000_000_000_000u64
                &&& w.neg == (s < 0 || (s == 0 && zero_neg))
                &&& w.exp as int == crate::verus_model::min_exp(a.view(), b.view())
                &&& !w.sticky
            },
            None => true,
        },
))]
fn aligned(a: Finite64, b: Finite64, zero_neg: bool) -> Option<Wide64> {
    let gap = a.exp - b.exp;
    let distance = if gap >= 0 { gap as u32 } else { (-gap) as u32 };
    if distance > 18 {
        return None;
    }
    let high = if gap >= 0 { a.coeff } else { b.coeff };
    if high >= pow10_64(18 - distance) {
        return None;
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::verus_model::lemma_narrow_align_scale(high as int, distance as nat);
    }
    let ac = a.coeff as i64;
    let bc = b.coeff as i64;
    let mut ac = if a.neg { -ac } else { ac };
    let mut bc = if b.neg { -bc } else { bc };
    #[cfg(verus_keep_ghost)]
    proof_decl! {
        let ghost ac0: int = ac as int;
        let ghost bc0: int = bc as int;
    }
    if gap > 0 {
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(-1_000_000_000_000_000_000int < (ac as int * vstd::arithmetic::power::pow(10, distance as nat)) < 1_000_000_000_000_000_000int) by (nonlinear_arith)
                requires ac as int == if a.neg { -(high as int) } else { high as int },
                    0 <= high as int * vstd::arithmetic::power::pow(10, distance as nat) < 1_000_000_000_000_000_000int;
        }
        ac *= pow10_64(distance) as i64;
    } else if gap < 0 {
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(-1_000_000_000_000_000_000int < (bc as int * vstd::arithmetic::power::pow(10, distance as nat)) < 1_000_000_000_000_000_000int) by (nonlinear_arith)
                requires bc as int == if b.neg { -(high as int) } else { high as int },
                    0 <= high as int * vstd::arithmetic::power::pow(10, distance as nat) < 1_000_000_000_000_000_000int;
        }
        bc *= pow10_64(distance) as i64;
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        lemma_pow0(10);
        let f = min_exp(a.view(), b.view());
        if gap <= 0 {
            assert(ac as int == ac0);
            assert(pow(10, (a.exp as int - f) as nat) == 1);
            assert(ac0 * pow(10, 0) == ac0) by (nonlinear_arith)
                requires pow(10, 0) == 1;
        } else {
            assert(ac as int == ac0 * pow(10, gap as nat));
        }
        if gap >= 0 {
            assert(bc as int == bc0);
            assert(pow(10, (b.exp as int - f) as nat) == 1);
            assert(bc0 * pow(10, 0) == bc0) by (nonlinear_arith)
                requires pow(10, 0) == 1;
        } else {
            assert(bc as int == bc0 * pow(10, (-gap) as nat));
        }
        lemma_add_signed_product(a.neg, a.coeff as int, a.exp as int, f, ac as int);
        lemma_add_signed_product(b.neg, b.coeff as int, b.exp as int, f, bc as int);
        assert(-1_010_000_000_000_000_000int < (ac as int + bc as int) < 1_010_000_000_000_000_000int);
    }
    let sum = ac + bc;
    Some(Wide64 {
        neg: sum < 0 || (sum == 0 && zero_neg),
        coeff: if sum < 0 { (-sum) as u64 } else { sum as u64 },
        exp: a.exp.min(b.exp),
        sticky: false,
    })
}

#[inline(never)]
#[cfg_attr(verus_keep_ghost, verus_spec(ret =>
    requires
        crate::layout::bid_width(W::LAYOUT),
        W::LAYOUT.digits <= 16,
        crate::layout::in_range(x, crate::layout::W64::LAYOUT),
        crate::layout::in_range(y, crate::layout::W64::LAYOUT),
    ensures
        crate::verus_model::rounded(
            ret,
            crate::verus_model::sum_sign(x, y, subtract, dir),
            crate::verus_model::abs_int(crate::verus_model::exact_sum(x, y, subtract)),
            crate::verus_model::min_exp(x, y),
            crate::verus_model::min_exp(x, y),
            dir,
        ),
        forall|other: Result<InRange<W>, Error>|
            #![trigger crate::verus_model::rounded(other, crate::verus_model::sum_sign(x, y, subtract, dir),
                crate::verus_model::abs_int(crate::verus_model::exact_sum(x, y, subtract)),
                crate::verus_model::min_exp(x, y), crate::verus_model::min_exp(x, y), dir)]
            crate::verus_model::rounded(other, crate::verus_model::sum_sign(x, y, subtract, dir),
                crate::verus_model::abs_int(crate::verus_model::exact_sum(x, y, subtract)),
                crate::verus_model::min_exp(x, y), crate::verus_model::min_exp(x, y), dir)
                ==> crate::verus_model::same_result(ret, other),
))]
#[cfg_attr(verus_keep_ghost, verifier::rlimit(60))]
pub(crate) fn add_sub_general<W: Width>(
    x: Unpacked,
    y: Unpacked,
    subtract: bool,
    dir: Option<Rounding>,
) -> Result<InRange<W>, Error> {
    #[cfg(verus_keep_ghost)]
    proof_decl! {
        let ghost f = min_exp(x, y);
        let ghost target = exact_sum(x, y, subtract);
        let ghost target_neg = sum_sign(x, y, subtract, dir);
        crate::layout::lemma_layout_values();
        reveal(pow);
        assert(pow(10, 16) == 10_000_000_000_000_000int) by (compute_only);
        assert(pow(10, 34) == 10000000000000000000000000000000000int) by (compute_only);
        lemma_add_fit_transfer::<W>();
        lemma_narrow_rounded_deterministic::<W>();
    }
    let mut a = Finite64 {
        neg: x.neg,
        coeff: x.coeff as u64,
        exp: x.exp,
    };
    let mut b = Finite64 {
        neg: y.neg ^ subtract,
        coeff: y.coeff as u64,
        exp: y.exp,
    };
    let pref = a.exp.min(b.exp);
    let zero_neg = if a.neg == b.neg {
        a.neg
    } else {
        dir == Some(Rounding::TowardNegative)
    };
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(target == exact_sum(a.view(), b.view(), false));
        assert(target_neg == (target < 0 || (target == 0 && zero_neg)));
    }
    if let Some(w) = aligned(a, b, zero_neg) {
        #[cfg(verus_keep_ghost)]
        proof! {
            lemma_pow0(10);
            lemma_add_exact_model(w.neg, w.coeff as int, w.exp as int, f, target, zero_neg, W::LAYOUT);
        }
        return fit::<W>(w, pref, dir);
    }
    if a.coeff == 0 || b.coeff == 0 {
        let v = if a.coeff == 0 { b } else { a };
        #[cfg(verus_keep_ghost)]
        proof! {
            lemma_pow_positive(10, (v.exp as int - f) as nat);
            assert(target == signed_at(v.neg, v.coeff as int, v.exp as int, f)) by (nonlinear_arith)
                requires target == signed_at(a.neg, a.coeff as int, a.exp as int, f) + signed_at(b.neg, b.coeff as int, b.exp as int, f),
                    a.coeff == 0 || b.coeff == 0, v == if a.coeff == 0 { b } else { a };
            assert(target == 0 ==> v.coeff == 0) by (nonlinear_arith)
                requires target == signed_at(v.neg, v.coeff as int, v.exp as int, f),
                    pow(10, (v.exp as int - f) as nat) > 0, v.coeff >= 0;
            if v.coeff == 0 {
                assert(signed_at(zero_neg, 0, v.exp as int, f) == 0) by (nonlinear_arith);
                assert(target == 0) by (nonlinear_arith)
                    requires target == signed_at(v.neg, v.coeff as int, v.exp as int, f), v.coeff == 0;
            }
            lemma_add_exact_model(if v.coeff == 0 { zero_neg } else { v.neg }, v.coeff as int, v.exp as int, f, target, zero_neg, W::LAYOUT);
        }
        return fit::<W>(
            Wide64 {
                neg: if v.coeff == 0 { zero_neg } else { v.neg },
                coeff: v.coeff,
                exp: v.exp,
                sticky: false,
            },
            pref,
            dir,
        );
    }
    if a.exp < b.exp {
        core::mem::swap(&mut a, &mut b);
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(target == exact_sum(a.view(), b.view(), false));
        assert(target_neg == sum_sign(a.view(), b.view(), false, dir));
    }
    match compressed::<W>(a, b, pref, zero_neg, dir) {
        Some(result) => result,
        None => crate::kernel::add_sub_general::<W>(x, y, subtract, dir),
    }
}

#[inline(never)]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "inputs c<10^16 and exponents -398..=369; stripping z<=15 is capped at gap. Exact retry n+d<=18 bounds sum below 2^63. Otherwise n<=P<=16 and b<10^P are checked; e=P+1-n>=1, g=d-e>=2, scaled in [10^P,10^(P+1)), hi<10^(P-2); H>=10^P-10^(P-2)-1>=10^(P-1), H<10^(P+1)+10^(P-2)<2^63. All exponents remain between original exponents"
)]
#[cfg_attr(verus_keep_ghost, verus_spec(ret =>
    requires
        crate::layout::bid_width(W::LAYOUT),
        W::LAYOUT.digits <= 16,
        0 < a.coeff < 10_000_000_000_000_000u64,
        0 < b.coeff < 10_000_000_000_000_000u64,
        -398 <= b.exp <= a.exp <= 369,
        pref == b.exp,
        zero_neg == if a.neg == b.neg { a.neg } else { dir == Some(Rounding::TowardNegative) },
    ensures
        match ret {
            Some(result) => crate::verus_model::rounded(
                result,
                crate::verus_model::sum_sign(a.view(), b.view(), false, dir),
                crate::verus_model::abs_int(crate::verus_model::exact_sum(a.view(), b.view(), false)),
                pref as int, pref as int, dir,
            ),
            None => a.coeff as int >= vstd::arithmetic::power::pow(10, W::LAYOUT.digits as nat)
                || exists|z: nat| #![trigger vstd::arithmetic::power::pow(10, z)] z <= 15 && z <= a.exp as int - b.exp as int
                    && b.coeff as int % vstd::arithmetic::power::pow(10, z) == 0
                    && b.coeff as int / vstd::arithmetic::power::pow(10, z) >= vstd::arithmetic::power::pow(10, W::LAYOUT.digits as nat)
                    && (z == a.exp as int - b.exp as int || (b.coeff as int / vstd::arithmetic::power::pow(10, z)) % 10 != 0),
        },
))]
#[cfg_attr(verus_keep_ghost, verifier::rlimit(60))]
fn compressed<W: Width>(
    a: Finite64,
    mut b: Finite64,
    pref: i32,
    zero_neg: bool,
    dir: Option<Rounding>,
) -> Option<Result<InRange<W>, Error>> {
    #[cfg(verus_keep_ghost)]
    proof_decl! {
        let ghost orig_b = b;
        let ghost target = exact_sum(a.view(), b.view(), false);
        let ghost target_neg = sum_sign(a.view(), b.view(), false, dir);
        crate::layout::lemma_layout_values();
        reveal(pow);
        assert(pow(10, 16) == 10_000_000_000_000_000int) by (compute_only);
        lemma_decimal_digits_le(a.coeff as nat, 16);
        lemma_add_fit_transfer::<W>();
    }
    let mut d = (a.exp - b.exp) as u32;
    let n = digits_64(a.coeff);
    let (stripped, zeros) = strip_trailing_zeros_64(b.coeff);
    let z = zeros.min(d);
    #[cfg(verus_keep_ghost)]
    proof! {
        vstd::arithmetic::power::lemma_pow_positive(10, zeros as nat);
        assert(0 < stripped <= b.coeff) by (nonlinear_arith)
            requires b.coeff as int == stripped as int * pow(10, zeros as nat), b.coeff > 0, pow(10, zeros as nat) >= 1;
    }
    b.coeff = if z == zeros {
        stripped
    } else {
        div_rem_pow10_64(b.coeff, z).0
    };
    #[cfg(verus_keep_ghost)]
    proof! {
        if z != zeros {
            lemma_add_strip(orig_b.coeff as int, stripped as int, zeros as nat, z as nat, b.coeff as int,
                orig_b.coeff as int - b.coeff as int * pow(10, z as nat));
        }
        lemma_add_signed_scale(b.neg, b.coeff as int, orig_b.exp as int, pref as int, z as nat);
    }
    b.exp += z as i32;
    d -= z;
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(target == signed_at(a.neg, a.coeff as int, a.exp as int, pref as int)
            + signed_at(b.neg, b.coeff as int, b.exp as int, pref as int));
        assert(d == a.exp - b.exp);
        assert(d != 0 ==> b.coeff % 10 != 0);
    }
    let w = if let Some(w) = aligned(a, b, zero_neg) {
        #[cfg(verus_keep_ghost)]
        proof! {
            lemma_add_signed_scale(a.neg, a.coeff as int, b.exp as int, pref as int, d as nat);
            vstd::arithmetic::power::lemma_pow0(10);
            let local = exact_sum(a.view(), b.view(), false);
            let t = pow(10, (b.exp as int - pref) as nat);
            lemma_pow_positive(10, (b.exp as int - pref) as nat);
            assert(target == local * t) by (nonlinear_arith)
                requires target == signed_at(a.neg, a.coeff as int * pow(10, d as nat), b.exp as int, pref as int)
                        + signed_at(b.neg, b.coeff as int, b.exp as int, pref as int),
                    local == signed_at(a.neg, a.coeff as int, a.exp as int, b.exp as int)
                        + signed_at(b.neg, b.coeff as int, b.exp as int, b.exp as int),
                    d == a.exp - b.exp, t == pow(10, (b.exp as int - pref) as nat), pow(10, 0) == 1;
            assert(target == signed_at(w.neg, w.coeff as int, w.exp as int, pref as int)) by (nonlinear_arith)
                requires target == local * t, w.coeff as int == abs_int(local),
                    w.neg == (local < 0 || (local == 0 && zero_neg)), w.exp == b.exp,
                    t == pow(10, (b.exp as int - pref) as nat);
            assert(target == 0 ==> w.neg == zero_neg) by (nonlinear_arith)
                requires target == local * t, t > 0, w.neg == (local < 0 || (local == 0 && zero_neg));
            lemma_add_exact_model(w.neg, w.coeff as int, w.exp as int, pref as int, target, zero_neg, W::LAYOUT);
        }
        w
    } else {
        let p = W::LAYOUT.digits;
        if n > p || b.coeff >= pow10_64(p) {
            #[cfg(verus_keep_ghost)]
            proof! {
                lemma_pow_positive(10, z as nat);
                vstd::arithmetic::div_mod::lemma_fundamental_div_mod_converse(orig_b.coeff as int, pow(10, z as nat), b.coeff as int, 0);
                assert(n > p ==> a.coeff as int >= pow(10, p as nat)) by {
                    if n > p { lemma_pow_increases(10, p as nat, (n - 1) as nat); }
                }
                if n <= p { assert(exists|zz: nat| #![trigger pow(10, zz)] zz <= 15 && zz <= a.exp as int - orig_b.exp as int
                    && orig_b.coeff as int % pow(10, zz) == 0
                    && orig_b.coeff as int / pow(10, zz) >= pow(10, p as nat)
                    && (zz == a.exp as int - orig_b.exp as int || (orig_b.coeff as int / pow(10, zz)) % 10 != 0)) by {
                        assert(orig_b.coeff as int / pow(10, z as nat) == b.coeff as int);
                    } }
            }
            return None;
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            lemma_narrow_compress_scale(a.coeff as int, n as nat, d as nat, p as nat);
            assert(d > 0);
        }
        let e = p + 1 - n;
        let g = d - e;
        let (hi, rem) = div_rem_pow10_64(b.coeff, g);
        #[cfg(verus_keep_ghost)]
        proof! {
            lemma_narrow_compress_bounds(b.coeff as int, g as nat, p as nat, hi as int, rem as int);
            lemma_add_signed_scale(a.neg, a.coeff as int, b.exp as int + g as int, pref as int, e as nat);
        }
        let scaled = a.coeff * pow10_64(e);
        #[cfg(verus_keep_ghost)]
        proof! {
            lemma_narrow_compress_model(a.neg, b.neg, scaled as int, b.coeff as int, b.exp as int, pref as int,
                g as nat, hi as int, rem as int, if a.neg == b.neg { scaled as int + hi as int } else { scaled as int - hi as int - 1 }, target, W::LAYOUT);
        }
        let coeff = if a.neg == b.neg {
            scaled + hi
        } else {
            scaled - hi - if rem != 0 { 1 } else { 0 }
        };
        Wide64 {
            neg: a.neg,
            coeff,
            exp: b.exp + g as i32,
            sticky: rem != 0,
        }
    };
    #[cfg(verus_keep_ghost)]
    proof! { assert(w.neg == target_neg); }
    Some(fit::<W>(w, pref, dir))
}

#[inline(never)]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "W32/W64, c<2^63, exp/pref -398..=369 with pref<=exp for nonzero; sticky implies c>=10^(P-1). k<=767, quotient <=P digits, exact rescaling <=P digits, increment <=10^P; exponent including carry fits i32 and checked range fits output"
)]
#[cfg_attr(verus_keep_ghost, verus_spec(ret =>
    requires
        crate::layout::bid_width(W::LAYOUT),
        W::LAYOUT.digits <= 16,
        w.coeff < 0x8000_0000_0000_0000u64,
        -398 <= w.exp <= 369,
        -398 <= pref <= 369,
        w.sticky ==> w.coeff as int >= vstd::arithmetic::power::pow(10, (W::LAYOUT.digits - 1) as nat),
        w.coeff != 0 ==> pref <= w.exp,
    ensures
        crate::verus_model::rounded(
            ret,
            w.neg,
            10 * w.coeff as int + if w.sticky { 5int } else { 0int },
            w.exp as int - 1,
            pref as int,
            dir,
        ),
))]
fn fit<W: Width>(w: Wide64, pref: i32, dir: Option<Rounding>) -> Result<InRange<W>, Error> {
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::layout::lemma_layout_values();
        vstd::arithmetic::power::lemma_pow_positive(10, W::LAYOUT.digits as nat);
        crate::verus_model::lemma_pow10_bounds(W::LAYOUT.digits as nat);
    }
    let lay = &W::LAYOUT;
    let k = (digits_64(w.coeff).saturating_sub(lay.digits) as i32).max(lay.qmin - w.exp);
    let (mut coeff, rem) = div_rem_pow10_64(w.coeff, k as u32);
    let mut exp = w.exp + k;
    #[cfg(verus_keep_ghost)]
    proof! {
        crate::verus_model::lemma_narrow_fit_split(
            w.coeff as int,
            w.sticky,
            w.exp as int,
            W::LAYOUT,
            crate::verus_model::decimal_digits(w.coeff as nat) as int,
            k as int,
            coeff as int,
            rem as int,
        );
    }
    if !w.sticky && rem == 0 {
        let pref = pref.clamp(lay.qmin, lay.qmax);
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
                exp: pref,
            }));
        }
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
            let shift = (exp - pref).min((lay.digits - digits_64(coeff)) as i32) as u32;
            #[cfg(verus_keep_ghost)]
            proof_decl! {
                let ghost q: int = coeff as int;
                let ghost dq: int = crate::verus_model::decimal_digits(coeff as nat) as int;
                crate::verus_model::lemma_fit_scale(q, dq, W::LAYOUT, shift as int);
            }
            coeff *= pow10_64(shift);
            exp -= shift as i32;
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
                if exp > lay.qmax || (exp == lay.qmax && coeff >= pow10_64(lay.digits) - 1) {
                    Error::Overflow
                } else {
                    Error::Inexact
                },
            );
        };
        if (dir == Rounding::TowardPositive) != w.neg {
            coeff += 1;
            if coeff == pow10_64(lay.digits) {
                coeff = pow10_64(lay.digits - 1);
                exp += 1;
            }
        }
    }
    if exp > lay.qmax {
        return Err(Error::Overflow);
    }
    Ok(InRange::unchecked(Unpacked {
        neg: w.neg,
        coeff: u128::from(coeff),
        exp,
    }))
}
