use vstd::arithmetic::div_mod::*;
use vstd::arithmetic::mul::*;
use vstd::arithmetic::power::*;
use vstd::arithmetic::power2::*;
use vstd::prelude::*;

verus! {

pub(crate) open spec fn scaled_le(c1: int, e1: int, c2: int, e2: int) -> bool {
    if e1 >= e2 {
        c1 * pow(10, (e1 - e2) as nat) <= c2
    } else {
        c1 <= c2 * pow(10, (e2 - e1) as nat)
    }
}

pub(crate) open spec fn scaled_eq(c1: int, e1: int, c2: int, e2: int) -> bool {
    scaled_le(c1, e1, c2, e2) && scaled_le(c2, e2, c1, e1)
}

pub(crate) open spec fn fits_above(c: int, e: int, lay: crate::layout::Layout) -> bool {
    0 <= c < pow(10, lay.digits as nat) && lay.qmin <= e
}

pub(crate) open spec fn fits(c: int, e: int, lay: crate::layout::Layout) -> bool {
    fits_above(c, e, lay) && e <= lay.qmax
}

pub(crate) open spec fn is_floor(c: int, e: int, m: int, f: int, lay: crate::layout::Layout) -> bool {
    &&& fits_above(c, e, lay)
    &&& scaled_le(c, e, m, f)
    &&& forall|c2: int, e2: int|
        #![trigger fits_above(c2, e2, lay)]
        fits_above(c2, e2, lay) && scaled_le(c2, e2, m, f) ==> scaled_le(c2, e2, c, e)
}

pub(crate) open spec fn is_ceil(c: int, e: int, m: int, f: int, lay: crate::layout::Layout) -> bool {
    &&& fits_above(c, e, lay)
    &&& scaled_le(m, f, c, e)
    &&& forall|c2: int, e2: int|
        #![trigger fits_above(c2, e2, lay)]
        fits_above(c2, e2, lay) && scaled_le(m, f, c2, e2) ==> scaled_le(c, e, c2, e2)
}

pub(crate) open spec fn least_exp(c: int, e: int, lay: crate::layout::Layout) -> bool {
    e == lay.qmin || 10 * c >= pow(10, lay.digits as nat)
}

pub(crate) open spec fn closest_exp(c: int, e: int, pref: int, lay: crate::layout::Layout) -> bool {
    ||| e == pref
    ||| e > pref && (e == lay.qmin || 10 * c >= pow(10, lay.digits as nat))
    ||| e < pref && (e == lay.qmax || c % 10 != 0)
}

pub(crate) open spec fn clamp_exp(pref: int, lay: crate::layout::Layout) -> int {
    if pref < lay.qmin {
        lay.qmin as int
    } else if pref > lay.qmax {
        lay.qmax as int
    } else {
        pref
    }
}

pub(crate) open spec fn fit_layout(lay: crate::layout::Layout) -> bool {
    1 <= lay.digits <= 34 && lay.qmin < lay.qmax
}

pub(crate) open spec fn fit_split(c: int, s: bool, e: int, lay: crate::layout::Layout, k: int, q: int, rem: int) -> bool {
    &&& k >= 0
    &&& c == q * pow(10, k as nat) + rem
    &&& 0 <= rem < pow(10, k as nat)
    &&& 0 <= q < pow(10, lay.digits as nat)
    &&& q <= c
    &&& lay.qmin <= e + k
    &&& (rem != 0 || s) ==> (e + k == lay.qmin || pow(10, (lay.digits - 1) as nat) <= q)
}

pub(crate) open spec fn exact_result(c: int, e: int, m: int, f: int, pref: int, lay: crate::layout::Layout) -> bool {
    &&& fits_above(c, e, lay)
    &&& scaled_eq(c, e, m, f)
    &&& is_floor(c, e, m, f, lay)
    &&& is_ceil(c, e, m, f, lay)
    &&& closest_exp(c, e, pref, lay)
    &&& e > lay.qmax ==> scaled_le(pow(10, lay.digits as nat), lay.qmax as int, m, f)
        && !scaled_le(m, f, pow(10, lay.digits as nat) - 1, lay.qmax as int)
}

pub(crate) open spec fn floor_result(c: int, e: int, m: int, f: int, lay: crate::layout::Layout) -> bool {
    &&& fits_above(c, e, lay)
    &&& is_floor(c, e, m, f, lay)
    &&& !scaled_eq(c, e, m, f)
    &&& least_exp(c, e, lay)
    &&& e > lay.qmax ==> scaled_le(pow(10, lay.digits as nat), lay.qmax as int, m, f)
}

pub(crate) open spec fn ceil_result(c: int, e: int, m: int, f: int, lay: crate::layout::Layout) -> bool {
    &&& fits_above(c, e, lay)
    &&& is_ceil(c, e, m, f, lay)
    &&& !scaled_eq(c, e, m, f)
    &&& least_exp(c, e, lay)
    &&& e > lay.qmax ==> !scaled_le(m, f, pow(10, lay.digits as nat) - 1, lay.qmax as int)
}

pub(crate) open spec fn rounded_up(neg: bool, dir: Option<crate::Rounding>) -> bool {
    match dir {
        Some(d) => (d == crate::Rounding::TowardPositive) != neg,
        None => false,
    }
}

pub(crate) open spec fn rounded<W: crate::layout::Width>(
    ret: Result<crate::layout::InRange<W>, crate::Error>,
    neg: bool,
    m: int,
    f: int,
    pref: int,
    dir: Option<crate::Rounding>,
) -> bool {
    let lay = W::LAYOUT;
    let p10 = pow(10, lay.digits as nat);
    let up = rounded_up(neg, dir);
    match ret {
        Ok(r) => {
            let c = r.view().coeff as int;
            let e = r.view().exp as int;
            &&& crate::layout::in_range(r.view(), lay)
            &&& r.view().neg == neg
            &&& dir is None ==> scaled_eq(c, e, m, f)
            &&& dir is Some && up ==> is_ceil(c, e, m, f, lay)
            &&& dir is Some && !up ==> is_floor(c, e, m, f, lay)
            &&& scaled_eq(c, e, m, f) ==> closest_exp(c, e, clamp_exp(pref, lay), lay)
            &&& !scaled_eq(c, e, m, f) ==> least_exp(c, e, lay)
        },
        Err(crate::Error::Overflow) => if dir is Some && !up {
            scaled_le(p10, lay.qmax as int, m, f)
        } else {
            !scaled_le(m, f, p10 - 1, lay.qmax as int)
        },
        Err(crate::Error::Inexact) => {
            &&& dir is None
            &&& scaled_le(m, f, p10 - 1, lay.qmax as int)
            &&& forall|c2: int, e2: int| #![trigger fits(c2, e2, lay)] fits(c2, e2, lay) ==> !scaled_eq(c2, e2, m, f)
        },
        Err(_) => false,
    }
}

pub(crate) open spec fn same_result<W: crate::layout::Width>(a: Result<crate::layout::InRange<W>, crate::Error>, b: Result<crate::layout::InRange<W>, crate::Error>) -> bool {
    match (a, b) {
        (Ok(x), Ok(y)) => x.view() == y.view(),
        (Err(x), Err(y)) => x == y,
        _ => false,
    }
}

pub(crate) proof fn lemma_narrow_cohort_order(c1: int, e1: int, c2: int, e2: int, pref: int, exact: bool, lay: crate::layout::Layout)
    requires
        fit_layout(lay),
        fits(c1, e1, lay), fits(c2, e2, lay),
        lay.qmin <= pref <= lay.qmax,
        scaled_eq(c1, e1, c2, e2),
        if exact { closest_exp(c1, e1, pref, lay) && closest_exp(c2, e2, pref, lay) }
        else { least_exp(c1, e1, lay) && least_exp(c2, e2, lay) },
        e1 < e2,
    ensures false,
{
    let g = (e2 - e1) as nat;
    let t = pow(10, g);
    lemma_pow0(10);
    lemma_pow1(10);
    lemma_pow_increases(10, 1, g);
    lemma_scaled_base(c1, e1, c2, e2, e1);
    lemma_scaled_base(c2, e2, c1, e1, e1);
    assert(c1 == c2 * t);
    if !exact || e2 > pref {
        assert(10 * c2 >= pow(10, lay.digits as nat));
        assert(c1 >= 10 * c2) by (nonlinear_arith)
            requires c1 == c2 * t, t >= 10, c2 >= 0;
    } else {
        assert(c1 % 10 != 0);
        lemma_pow10_step((g - 1) as nat);
        assert(c1 == (c2 * pow(10, (g - 1) as nat)) * 10) by (nonlinear_arith)
            requires c1 == c2 * pow(10, g), pow(10, g) == 10 * pow(10, (g - 1) as nat);
        lemma_mod_multiples_basic(c2 * pow(10, (g - 1) as nat), 10);
    }
}

pub(crate) proof fn lemma_narrow_finite_bound(c: int, e: int, lay: crate::layout::Layout)
    requires fit_layout(lay), fits(c, e, lay),
    ensures
        scaled_le(c, e, pow(10, lay.digits as nat) - 1, lay.qmax as int),
        !scaled_le(pow(10, lay.digits as nat), lay.qmax as int, c, e),
{
    let p10 = pow(10, lay.digits as nat);
    let g = (lay.qmax - e) as nat;
    lemma_pow0(10);
    lemma_scaled_base(c, e, p10 - 1, lay.qmax as int, e);
    lemma_scaled_base(p10, lay.qmax as int, c, e, e);
    assert(c * pow(10, 0) == c) by (nonlinear_arith)
        requires pow(10, 0) == 1;
    lemma_pow_positive(10, g);
    lemma_pow_positive(10, lay.digits as nat);
    assert(c <= (p10 - 1) * pow(10, g)) by (nonlinear_arith)
        requires 0 <= c < p10, pow(10, g) >= 1;
    assert(p10 * pow(10, g) > c) by (nonlinear_arith)
        requires 0 <= c < p10, pow(10, g) >= 1;
}

pub(crate) proof fn lemma_narrow_ok_excludes_error<W: crate::layout::Width>(r: crate::layout::InRange<W>, err: crate::Error, neg: bool, m: int, f: int, pref: int, dir: Option<crate::Rounding>)
    requires
        crate::layout::bid_width(W::LAYOUT),
        rounded(Ok(r), neg, m, f, pref, dir),
        rounded::<W>(Err(err), neg, m, f, pref, dir),
    ensures false,
{
    crate::layout::lemma_layout_values();
    let lay = W::LAYOUT;
    let c = r.view().coeff as int;
    let e = r.view().exp as int;
    let p10 = pow(10, lay.digits as nat);
    let qmax = lay.qmax as int;
    lemma_narrow_finite_bound(c, e, lay);
    match err {
        crate::Error::Inexact => { assert(fits(c, e, lay)); },
        crate::Error::Overflow => {
            if dir is None || rounded_up(neg, dir) {
                lemma_scaled_trans(m, f, c, e, p10 - 1, qmax);
            } else {
                let low = pow(10, (lay.digits - 1) as nat);
                lemma_pow10_step((lay.digits - 1) as nat);
                lemma_pow0(10);
                lemma_pow1(10);
                lemma_scaled_base(p10, qmax, low, qmax + 1, qmax);
                lemma_scaled_base(low, qmax + 1, p10, qmax, qmax);
                assert(scaled_eq(p10, qmax, low, qmax + 1));
                lemma_scaled_trans(low, qmax + 1, p10, qmax, m, f);
                assert(fits_above(low, qmax + 1, lay));
                assert(scaled_le(low, qmax + 1, c, e));
                lemma_scaled_trans(p10, qmax, low, qmax + 1, c, e);
            }
        },
        _ => {},
    }
}

pub(crate) proof fn lemma_narrow_rounded_unique<W: crate::layout::Width>(r1: Result<crate::layout::InRange<W>, crate::Error>, r2: Result<crate::layout::InRange<W>, crate::Error>, neg: bool, m: int, f: int, pref: int, dir: Option<crate::Rounding>)
    requires
        crate::layout::bid_width(W::LAYOUT),
        rounded(r1, neg, m, f, pref, dir),
        rounded(r2, neg, m, f, pref, dir),
    ensures same_result(r1, r2),
{
    crate::layout::lemma_layout_values();
    let lay = W::LAYOUT;
    match (r1, r2) {
        (Ok(a), Ok(b)) => {
            let c1 = a.view().coeff as int;
            let e1 = a.view().exp as int;
            let c2 = b.view().coeff as int;
            let e2 = b.view().exp as int;
            assert(fits_above(c1, e1, lay));
            assert(fits_above(c2, e2, lay));
            if dir is None {
                lemma_scaled_trans(c1, e1, m, f, c2, e2);
                lemma_scaled_trans(c2, e2, m, f, c1, e1);
            }
            assert(scaled_eq(c1, e1, c2, e2));
            if scaled_eq(c1, e1, m, f) {
                lemma_scaled_trans(c2, e2, c1, e1, m, f);
                lemma_scaled_trans(m, f, c1, e1, c2, e2);
            }
            if scaled_eq(c2, e2, m, f) {
                lemma_scaled_trans(c1, e1, c2, e2, m, f);
                lemma_scaled_trans(m, f, c2, e2, c1, e1);
            }
            let exact = scaled_eq(c1, e1, m, f);
            let preference = clamp_exp(pref, lay);
            if e1 < e2 { lemma_narrow_cohort_order(c1, e1, c2, e2, preference, exact, lay); }
            if e2 < e1 { lemma_narrow_cohort_order(c2, e2, c1, e1, preference, exact, lay); }
            lemma_scaled_same(c1, c2, e1);
            lemma_scaled_same(c2, c1, e1);
        },
        (Ok(a), Err(err)) => { lemma_narrow_ok_excludes_error(a, err, neg, m, f, pref, dir); },
        (Err(err), Ok(b)) => { lemma_narrow_ok_excludes_error(b, err, neg, m, f, pref, dir); },
        (Err(_), Err(_)) => {},
    }
}

pub(crate) proof fn lemma_narrow_rounded_deterministic<W: crate::layout::Width>()
    requires crate::layout::bid_width(W::LAYOUT),
    ensures
        forall|r1: Result<crate::layout::InRange<W>, crate::Error>, r2: Result<crate::layout::InRange<W>, crate::Error>, neg: bool, m: int, f: int, pref: int, dir: Option<crate::Rounding>|
            #![trigger rounded(r1, neg, m, f, pref, dir), rounded(r2, neg, m, f, pref, dir)]
            rounded(r1, neg, m, f, pref, dir) && rounded(r2, neg, m, f, pref, dir) ==> same_result(r1, r2),
{
    assert forall|r1: Result<crate::layout::InRange<W>, crate::Error>, r2: Result<crate::layout::InRange<W>, crate::Error>, neg: bool, m: int, f: int, pref: int, dir: Option<crate::Rounding>|
        #![trigger rounded(r1, neg, m, f, pref, dir), rounded(r2, neg, m, f, pref, dir)]
        rounded(r1, neg, m, f, pref, dir) && rounded(r2, neg, m, f, pref, dir) implies same_result(r1, r2) by {
        lemma_narrow_rounded_unique(r1, r2, neg, m, f, pref, dir);
    }
}

pub(crate) open spec fn signed_at(neg: bool, c: int, e: int, f: int) -> int {
    if neg {
        -(c * pow(10, (e - f) as nat))
    } else {
        c * pow(10, (e - f) as nat)
    }
}

pub(crate) open spec fn min_exp(a: crate::layout::Unpacked, b: crate::layout::Unpacked) -> int {
    if a.exp <= b.exp {
        a.exp as int
    } else {
        b.exp as int
    }
}

pub(crate) open spec fn exact_sum(a: crate::layout::Unpacked, b: crate::layout::Unpacked, subtract: bool) -> int {
    signed_at(a.neg, a.coeff as int, a.exp as int, min_exp(a, b)) + signed_at(
        b.neg != subtract,
        b.coeff as int,
        b.exp as int,
        min_exp(a, b),
    )
}

pub(crate) open spec fn sum_sign(
    a: crate::layout::Unpacked,
    b: crate::layout::Unpacked,
    subtract: bool,
    dir: Option<crate::Rounding>,
) -> bool {
    let s = exact_sum(a, b, subtract);
    let b_neg = b.neg != subtract;
    s < 0 || (s == 0 && if a.neg == b_neg {
        a.neg
    } else {
        dir == Some(crate::Rounding::TowardNegative)
    })
}

pub(crate) open spec fn abs_int(x: int) -> int {
    if x < 0 {
        -x
    } else {
        x
    }
}

pub(crate) open spec fn same_side(m1: int, f1: int, m2: int, f2: int, lay: crate::layout::Layout) -> bool {
    forall|c: int, e: int|
        #![trigger fits_above(c, e, lay)]
        fits_above(c, e, lay) ==> (scaled_le(c, e, m1, f1) == scaled_le(c, e, m2, f2)) && (scaled_le(
            m1,
            f1,
            c,
            e,
        ) == scaled_le(m2, f2, c, e))
}

pub(crate) proof fn lemma_add_input<W: crate::layout::Width>(a: crate::layout::Unpacked, b: crate::layout::Unpacked)
    requires
        crate::layout::bid_width(W::LAYOUT),
        crate::layout::in_range(a, <crate::layout::W128 as crate::layout::Width>::LAYOUT),
        crate::layout::in_range(b, <crate::layout::W128 as crate::layout::Width>::LAYOUT),
    ensures
        fit_layout(W::LAYOUT),
        pow(10, 34) == 10000000000000000000000000000000000int,
        1 <= W::LAYOUT.digits <= 34,
        a.coeff < 10000000000000000000000000000000000int,
        b.coeff < 10000000000000000000000000000000000int,
        -6176 <= a.exp <= 6111,
        -6176 <= b.exp <= 6111,
{
    crate::layout::lemma_layout_values();
    reveal(pow);
    assert(pow(10, 34) == 10000000000000000000000000000000000int) by (compute_only);
}

pub(crate) proof fn lemma_add_fast_scale(c: int, k: nat)
    requires
        -9999999999999999999999999999999999int <= c <= 9999999999999999999999999999999999int,
        k <= 4,
    ensures
        1 <= pow(10, k) <= 10000,
        -99999999999999999999999999999999990000int <= c * pow(10, k) <= 99999999999999999999999999999999990000int,
{
    lemma_pow_positive(10, k);
    lemma_pow_increases(10, k, 4);
    reveal(pow);
    assert(pow(10, 4) == 10000) by (compute_only);
    assert(-99999999999999999999999999999999990000int <= c * pow(10, k) <= 99999999999999999999999999999999990000int) by (nonlinear_arith)
        requires -9999999999999999999999999999999999int <= c <= 9999999999999999999999999999999999int, 1 <= pow(10, k) <= 10000;
}

pub(crate) proof fn lemma_add_pow10_bit_bound(k: nat)
    requires k <= 37,
    ensures pow(10, k) <= pow2(((10 * k + 2) / 3) as nat),
    decreases k,
{
    if k < 3 {
        reveal(pow);
        reveal(pow2);
        if k == 0 { assert(pow(10, 0) <= pow2(0)) by (compute_only); }
        else if k == 1 { assert(pow(10, 1) <= pow2(4)) by (compute_only); }
        else { assert(pow(10, 2) <= pow2(7)) by (compute_only); }
    } else {
        let previous = (k - 3) as nat;
        let width = ((10 * previous + 2) / 3) as nat;
        lemma_add_pow10_bit_bound(previous);
        lemma_pow_adds(10, previous, 3);
        lemma_pow2_adds(width, 10);
        assert((10 * k + 2) / 3 == width + 10) by (nonlinear_arith)
            requires k >= 3, previous == k - 3, width == (10 * previous + 2) / 3;
        assert(pow(10, 3) == 1000) by (compute_only);
        assert(pow2(10) == 1024) by (compute_only);
        lemma_pow2_pos(width);
        assert(pow(10, previous) * 1000 <= pow2(width) * 1024) by (nonlinear_arith)
            requires pow(10, previous) <= pow2(width), pow2(width) > 0;
    }
}

pub(crate) proof fn lemma_add_bit_scale(c: int, bits: nat, k: nat)
    requires
        c == 0 ==> bits == 0,
        c != 0 ==> abs_int(c) < pow2(bits),
        10 * k + 3 * bits <= 378,
    ensures
        k <= 37,
        1 <= pow(10, k) < 85070591730234615865843651857942052864int,
        -85070591730234615865843651857942052864int < c * pow(10, k) < 85070591730234615865843651857942052864int,
{
    let width = ((10 * k + 2) / 3) as nat;
    lemma_add_pow10_bit_bound(k);
    lemma_pow_positive(10, k);
    assert(width + bits <= 126) by (nonlinear_arith)
        requires 10 * k + 3 * bits <= 378, width == (10 * k + 2) / 3;
    lemma_pow2_adds(bits, width);
    if bits + width < 126 { lemma_pow2_strictly_increases(bits + width, 126); }
    assert(pow2(126) == 85070591730234615865843651857942052864int) by (compute_only);
    if c != 0 {
        lemma_mul_strict_inequality(abs_int(c), pow2(bits) as int, pow(10, k));
        lemma_mul_inequality(pow(10, k), pow2(width) as int, pow2(bits) as int);
    }
    assert(-85070591730234615865843651857942052864int < c * pow(10, k) < 85070591730234615865843651857942052864int) by (nonlinear_arith)
        requires 0 <= abs_int(c) * pow(10, k) < 85070591730234615865843651857942052864int,
            abs_int(c) == if c < 0 { -c } else { c };
    lemma_pow10_bounds(k);
    lemma_pow_increases(10, k, 37);
    assert(pow(10, 37) < 85070591730234615865843651857942052864int) by (compute_only);
}

pub(crate) proof fn lemma_add_sum_bound(a: int, b: int)
    requires
        -99999999999999999999999999999999990000int <= a <= 99999999999999999999999999999999990000int,
        -99999999999999999999999999999999990000int <= b <= 99999999999999999999999999999999990000int,
        abs_int(a) <= 9999999999999999999999999999999999int
            || abs_int(b) <= 9999999999999999999999999999999999int,
    ensures
        -170141183460469231731687303715884105728int < a + b < 170141183460469231731687303715884105728int,
{
}

pub(crate) proof fn lemma_narrow_align_scale(c: int, k: nat)
    requires
        0 <= c < pow(10, (18 - k) as nat),
        k <= 18,
    ensures
        pow(10, k) <= 1_000_000_000_000_000_000int,
        0 <= c * pow(10, k) < 1_000_000_000_000_000_000int,
{
    lemma_pow10_bounds(k);
    lemma_pow_positive(10, k);
    lemma_pow_adds(10, (18 - k) as nat, k);
    assert(pow(10, 18) == 1_000_000_000_000_000_000int) by (compute_only);
    lemma_mul_nonnegative(c, pow(10, k));
    lemma_mul_strict_inequality(c, pow(10, (18 - k) as nat), pow(10, k));
}

pub(crate) proof fn lemma_add_signed_product(neg: bool, c: int, e: int, f: int, v: int)
    requires
        v == (if neg { -c } else { c }) * pow(10, (e - f) as nat),
    ensures
        v == signed_at(neg, c, e, f),
{
    assert(v == signed_at(neg, c, e, f)) by (nonlinear_arith)
        requires v == (if neg { -c } else { c }) * pow(10, (e - f) as nat);
}

pub(crate) proof fn lemma_add_digit_scale(c: int, n: nat, k: nat)
    requires
        1 <= n <= 34,
        pow(10, (n - 1) as nat) <= c < pow(10, n),
    ensures
        0 < c * pow(10, k),
        pow(10, (n + k - 1) as nat) <= c * pow(10, k) < pow(10, n + k),
{
    lemma_pow_positive(10, k);
    lemma_pow_positive(10, (n - 1) as nat);
    lemma_mul_strictly_positive(c, pow(10, k));
    lemma_pow_adds(10, (n - 1) as nat, k);
    lemma_pow_adds(10, n, k);
    lemma_mul_inequality(pow(10, (n - 1) as nat), c, pow(10, k));
    lemma_mul_strict_inequality(c, pow(10, n), pow(10, k));
}

pub(crate) proof fn lemma_add_signed_scale(neg: bool, c: int, e: int, f: int, k: nat)
    requires
        f <= e,
    ensures
        signed_at(neg, c, e + k, f) == signed_at(neg, c * pow(10, k), e, f),
{
    lemma_pow_adds(10, k, (e - f) as nat);
    assert((e + k - f) as nat == k + (e - f) as nat);
    lemma_mul_is_associative(c, pow(10, k), pow(10, (e - f) as nat));
}

pub(crate) proof fn lemma_add_exact_scale(c: int, n: nat, k: nat)
    requires
        1 <= n <= 34,
        pow(10, (n - 1) as nat) <= c < pow(10, n),
        n + k <= 37,
    ensures
        0 < c * pow(10, k) < 10000000000000000000000000000000000000int,
{
    lemma_add_digit_scale(c, n, k);
    lemma_pow10_bounds(n + k);
}

pub(crate) proof fn lemma_add_exact_model(neg: bool, c: int, e: int, f: int, s: int, zero: bool, lay: crate::layout::Layout)
    requires
        0 <= c,
        f <= e,
        s == signed_at(neg, c, e, f),
        s == 0 ==> neg == zero,
    ensures
        neg == (s < 0 || (s == 0 && zero)),
        scaled_eq(10 * c, e - 1, abs_int(s), f),
        same_side(10 * c, e - 1, abs_int(s), f, lay),
{
    lemma_pow_positive(10, (e - f) as nat);
    lemma_mul_nonnegative(c, pow(10, (e - f) as nat));
    if c > 0 {
        lemma_mul_strictly_positive(c, pow(10, (e - f) as nat));
    }
    assert(abs_int(s) == c * pow(10, (e - f) as nat));
    lemma_pow0(10);
    lemma_scaled_base(c, e, abs_int(s), f, f);
    lemma_scaled_base(abs_int(s), f, c, e, f);
    assert(abs_int(s) * pow(10, 0) == abs_int(s));
    lemma_fit_between(c, false, e, 0, c, 0);
    lemma_scaled_trans(10 * c, e - 1, c, e, abs_int(s), f);
    lemma_scaled_trans(abs_int(s), f, c, e, 10 * c, e - 1);
    lemma_same_side_eq(10 * c, e - 1, abs_int(s), f, lay);
}

pub(crate) proof fn lemma_add_exact_sum(an: bool, bn: bool, a: int, b: int, e: int, f: int, s: int, zero: bool, neg: bool, c: int, lay: crate::layout::Layout)
    requires
        0 < a,
        0 < b,
        f <= e,
        s == signed_at(an, a, e, f) + signed_at(bn, b, e, f),
        if an == bn { neg == an && c == a + b }
        else if a > b { neg == an && c == a - b }
        else if a < b { neg == bn && c == b - a }
        else { neg == zero && c == 0 },
    ensures
        neg == (s < 0 || (s == 0 && zero)),
        same_side(10 * c, e - 1, abs_int(s), f, lay),
{
    assert(s == signed_at(neg, c, e, f)) by (nonlinear_arith)
        requires s == signed_at(an, a, e, f) + signed_at(bn, b, e, f),
            if an == bn { neg == an && c == a + b }
            else if a > b { neg == an && c == a - b }
            else if a < b { neg == bn && c == b - a }
            else { neg == zero && c == 0 };
    lemma_pow_positive(10, (e - f) as nat);
    assert(s == 0 ==> neg == zero) by (nonlinear_arith)
        requires s == signed_at(neg, c, e, f), pow(10, (e - f) as nat) > 0, c == 0 ==> neg == zero;
    lemma_add_exact_model(neg, c, e, f, s, zero, lay);
}

pub(crate) proof fn lemma_add_compress_scale(c: int, n: nat, k: nat)
    requires
        1 <= n <= 34,
        pow(10, (n - 1) as nat) <= c < pow(10, n),
        n + k == 35,
    ensures
        10000000000000000000000000000000000int <= c * pow(10, k) < 100000000000000000000000000000000000int,
{
    lemma_add_digit_scale(c, n, k);
    reveal(pow);
    assert(pow(10, 34) == 10000000000000000000000000000000000int) by (compute_only);
    assert(pow(10, 35) == 100000000000000000000000000000000000int) by (compute_only);
}

pub(crate) proof fn lemma_add_strip(c: int, stripped: int, zeros: nat, z: nat, q: int, rem: int)
    requires
        0 < c,
        0 <= stripped,
        c == stripped * pow(10, zeros),
        z <= zeros,
        0 <= q,
        c == q * pow(10, z) + rem,
        0 <= rem < pow(10, z),
    ensures
        rem == 0,
        0 < q <= c,
        c == q * pow(10, z),
{
    lemma_pow_adds(10, z, (zeros - z) as nat);
    lemma_pow_positive(10, z);
    lemma_pow_positive(10, (zeros - z) as nat);
    let t = stripped * pow(10, (zeros - z) as nat);
    lemma_mul_nonnegative(stripped, pow(10, (zeros - z) as nat));
    assert(c == t * pow(10, z)) by (nonlinear_arith)
        requires c == stripped * pow(10, zeros), pow(10, zeros) == pow(10, z) * pow(10, (zeros - z) as nat), t == stripped * pow(10, (zeros - z) as nat);
    lemma_fundamental_div_mod_converse(c, pow(10, z), t, 0);
    lemma_fundamental_div_mod_converse(c, pow(10, z), q, rem);
    assert(0 < q <= c) by (nonlinear_arith) requires c == q * pow(10, z), 0 < c, pow(10, z) >= 1;
}

pub(crate) proof fn lemma_add_compress_bounds(b: int, g: nat, hi: int, lo: int)
    requires
        0 < b < 10000000000000000000000000000000000int,
        b % 10 != 0,
        3 <= g,
        0 <= hi,
        b == hi * pow(10, g) + lo,
        0 <= lo < pow(10, g),
    ensures
        hi < 10000000000000000000000000000000int,
        0 < lo < pow(10, g),
{
    lemma_pow_increases(10, 3, g);
    reveal(pow);
    assert(pow(10, 3) == 1000) by (compute_only);
    assert(hi < 10000000000000000000000000000000int) by (nonlinear_arith)
        requires b < 10000000000000000000000000000000000int, b == hi * pow(10, g) + lo, 0 <= lo, 0 <= hi, 1000 <= pow(10, g);
    if lo == 0 {
        lemma_pow10_step((g - 1) as nat);
        assert(b == (hi * pow(10, (g - 1) as nat)) * 10) by (nonlinear_arith)
            requires b == hi * pow(10, g), pow(10, g) == 10 * pow(10, (g - 1) as nat);
        lemma_mod_multiples_basic(hi * pow(10, (g - 1) as nat), 10);
    }
}

pub(crate) proof fn lemma_add_compress_model(an: bool, bn: bool, a: int, b: int, e: int, f: int, g: nat, hi: int, lo: int, c: int, s: int, lay: crate::layout::Layout)
    requires
        fit_layout(lay),
        10000000000000000000000000000000000int <= a < 100000000000000000000000000000000000int,
        0 <= hi < 10000000000000000000000000000000int,
        b == hi * pow(10, g) + lo,
        0 < lo < pow(10, g),
        f <= e,
        c == if an == bn { a + hi } else { a - hi - 1 },
        s == signed_at(an, a, e + g, f) + signed_at(bn, b, e, f),
    ensures
        s != 0,
        an == (s < 0),
        pow(10, (lay.digits - 1) as nat) <= c < 0x8000_0000_0000_0000_0000_0000_0000_0000int,
        same_side(10 * c + 5, e + g - 1, abs_int(s), f, lay),
{
    let t = pow(10, g);
    let p = pow(10, (e - f) as nat);
    let r = if an == bn { lo } else { t - lo };
    let y = c * t + r;
    lemma_pow_positive(10, g);
    lemma_pow_positive(10, (e - f) as nat);
    lemma_pow_adds(10, g, (e - f) as nat);
    assert((e + g - f) as nat == g + (e - f) as nat);
    assert(0 < r < t);
    assert(1000000000000000000000000000000000int <= c);
    lemma_pow_increases(10, (lay.digits - 1) as nat, 33);
    reveal(pow);
    assert(pow(10, 33) == 1000000000000000000000000000000000int) by (compute_only);
    assert(y > 0) by (nonlinear_arith) requires y == c * t + r, c > 0, t > 0, r > 0;
    assert(s == if an { -(y * p) } else { y * p }) by (nonlinear_arith)
        requires
            s == signed_at(an, a, e + g, f) + signed_at(bn, b, e, f),
            pow(10, (e + g - f) as nat) == t * p,
            p == pow(10, (e - f) as nat),
            b == hi * t + lo,
            c == if an == bn { a + hi } else { a - hi - 1 },
            r == if an == bn { lo } else { t - lo }, y == c * t + r;
    lemma_mul_strictly_positive(y, p);
    assert(abs_int(s) == y * p);
    lemma_pow0(10);
    lemma_scaled_base(abs_int(s), f, c, e + g, f);
    lemma_scaled_base(c + 1, e + g, abs_int(s), f, f);
    assert(abs_int(s) * pow(10, 0) == abs_int(s));
    assert(!scaled_le(abs_int(s), f, c, e + g)) by (nonlinear_arith)
        requires
            scaled_le(abs_int(s), f, c, e + g) == (abs_int(s) <= c * pow(10, (e + g - f) as nat)),
            abs_int(s) == (c * t + r) * p, pow(10, (e + g - f) as nat) == t * p, 0 < r, p > 0;
    assert(!scaled_le(c + 1, e + g, abs_int(s), f)) by (nonlinear_arith)
        requires
            scaled_le(c + 1, e + g, abs_int(s), f) == ((c + 1) * pow(10, (e + g - f) as nat) <= abs_int(s)),
            abs_int(s) == (c * t + r) * p, pow(10, (e + g - f) as nat) == t * p, r < t, p > 0;
    lemma_fit_between(c, true, e + g, 0, c, 0);
    lemma_same_side_gap(c, e + g, 10 * c + 5, e + g - 1, abs_int(s), f, lay);
}

pub(crate) proof fn lemma_narrow_compress_scale(a: int, n: nat, d: nat, p: nat)
    requires
        p == 7 || p == 16,
        1 <= n <= p,
        pow(10, (n - 1) as nat) <= a < pow(10, n),
        d > 18 || a >= pow(10, (18 - d) as nat),
    ensures
        d >= p + 3 - n,
        2 <= d - (p + 1 - n),
        1 <= p + 1 - n <= 16,
        pow(10, p) <= a * pow(10, (p + 1 - n) as nat) < pow(10, p + 1),
        a * pow(10, (p + 1 - n) as nat) < 100000000000000000int,
{
    if n + d <= 18 {
        lemma_pow_increases(10, n, (18 - d) as nat);
    }
    lemma_add_digit_scale(a, n, (p + 1 - n) as nat);
    lemma_pow_increases(10, p + 1, 17);
    assert(pow(10, 17) == 100000000000000000int) by (compute_only);
}

pub(crate) proof fn lemma_narrow_compress_bounds(b: int, g: nat, p: nat, hi: int, rem: int)
    requires
        p == 7 || p == 16,
        0 < b < pow(10, p),
        b % 10 != 0,
        2 <= g,
        0 <= hi,
        b == hi * pow(10, g) + rem,
        0 <= rem < pow(10, g),
    ensures
        hi < pow(10, (p - 2) as nat),
        0 < rem,
{
    lemma_pow_positive(10, g);
    lemma_pow_adds(10, (p - 2) as nat, g);
    lemma_pow_increases(10, p, (p - 2) as nat + g);
    if hi >= pow(10, (p - 2) as nat) {
        lemma_mul_inequality(pow(10, (p - 2) as nat), hi, pow(10, g));
    }
    if rem == 0 {
        lemma_pow10_step((g - 1) as nat);
        assert(b == (hi * pow(10, (g - 1) as nat)) * 10) by (nonlinear_arith)
            requires b == hi * pow(10, g), pow(10, g) == 10 * pow(10, (g - 1) as nat);
        lemma_mod_multiples_basic(hi * pow(10, (g - 1) as nat), 10);
    }
}

pub(crate) proof fn lemma_narrow_compress_model(an: bool, bn: bool, a: int, b: int, e: int, f: int, g: nat, hi: int, lo: int, c: int, s: int, lay: crate::layout::Layout)
    requires
        fit_layout(lay),
        pow(10, lay.digits as nat) <= a < pow(10, lay.digits as nat + 1),
        lay.digits == 7 || lay.digits == 16,
        0 <= hi < pow(10, (lay.digits - 2) as nat),
        b == hi * pow(10, g) + lo,
        0 < lo < pow(10, g),
        f <= e,
        c == if an == bn { a + hi } else { a - hi - 1 },
        s == signed_at(an, a, e + g, f) + signed_at(bn, b, e, f),
    ensures
        s != 0,
        an == (s < 0),
        pow(10, (lay.digits - 1) as nat) <= c < 0x8000_0000_0000_0000int,
        same_side(10 * c + 5, e + g - 1, abs_int(s), f, lay),
{
    let t = pow(10, g);
    let p = pow(10, (e - f) as nat);
    let r = if an == bn { lo } else { t - lo };
    let y = c * t + r;
    lemma_pow_positive(10, g);
    lemma_pow_positive(10, (e - f) as nat);
    lemma_pow_adds(10, g, (e - f) as nat);
    assert((e + g - f) as nat == g + (e - f) as nat);
    assert(0 < r < t);
    if lay.digits == 7 {
        assert(pow(10, 5) == 100000int) by (compute_only);
        assert(pow(10, 6) == 1000000int) by (compute_only);
        assert(pow(10, 7) == 10000000int) by (compute_only);
        assert(pow(10, 8) == 100000000int) by (compute_only);
    } else {
        assert(pow(10, 14) == 100000000000000int) by (compute_only);
        assert(pow(10, 15) == 1000000000000000int) by (compute_only);
        assert(pow(10, 16) == 10000000000000000int) by (compute_only);
        assert(pow(10, 17) == 100000000000000000int) by (compute_only);
    }
    assert(pow(10, (lay.digits - 1) as nat) <= c < 0x8000_0000_0000_0000int);
    assert(y > 0) by (nonlinear_arith) requires y == c * t + r, c > 0, t > 0, r > 0;
    assert(s == if an { -(y * p) } else { y * p }) by (nonlinear_arith)
        requires
            s == signed_at(an, a, e + g, f) + signed_at(bn, b, e, f),
            pow(10, (e + g - f) as nat) == t * p,
            p == pow(10, (e - f) as nat),
            b == hi * t + lo,
            c == if an == bn { a + hi } else { a - hi - 1 },
            r == if an == bn { lo } else { t - lo }, y == c * t + r;
    lemma_mul_strictly_positive(y, p);
    assert(abs_int(s) == y * p);
    lemma_pow0(10);
    lemma_scaled_base(abs_int(s), f, c, e + g, f);
    lemma_scaled_base(c + 1, e + g, abs_int(s), f, f);
    assert(abs_int(s) * pow(10, 0) == abs_int(s));
    assert(!scaled_le(abs_int(s), f, c, e + g)) by (nonlinear_arith)
        requires
            scaled_le(abs_int(s), f, c, e + g) == (abs_int(s) <= c * pow(10, (e + g - f) as nat)),
            abs_int(s) == (c * t + r) * p, pow(10, (e + g - f) as nat) == t * p, 0 < r, p > 0;
    assert(!scaled_le(c + 1, e + g, abs_int(s), f)) by (nonlinear_arith)
        requires
            scaled_le(c + 1, e + g, abs_int(s), f) == ((c + 1) * pow(10, (e + g - f) as nat) <= abs_int(s)),
            abs_int(s) == (c * t + r) * p, pow(10, (e + g - f) as nat) == t * p, r < t, p > 0;
    lemma_fit_between(c, true, e + g, 0, c, 0);
    lemma_same_side_gap(c, e + g, 10 * c + 5, e + g - 1, abs_int(s), f, lay);
}

pub(crate) proof fn lemma_add_fit_transfer<W: crate::layout::Width>()
    requires
        fit_layout(W::LAYOUT),
    ensures
        forall|ret: Result<crate::layout::InRange<W>, crate::Error>, neg: bool, m1: int, f1: int, m2: int, f2: int, pref: int, dir: Option<crate::Rounding>|
            #![trigger same_side(m1, f1, m2, f2, W::LAYOUT), rounded(ret, neg, m1, f1, pref, dir)]
            same_side(m1, f1, m2, f2, W::LAYOUT) && rounded(ret, neg, m1, f1, pref, dir) ==> rounded(ret, neg, m2, f2, pref, dir),
{
    assert forall|ret: Result<crate::layout::InRange<W>, crate::Error>, neg: bool, m1: int, f1: int, m2: int, f2: int, pref: int, dir: Option<crate::Rounding>|
        #![trigger same_side(m1, f1, m2, f2, W::LAYOUT), rounded(ret, neg, m1, f1, pref, dir)]
        same_side(m1, f1, m2, f2, W::LAYOUT) && rounded(ret, neg, m1, f1, pref, dir) implies rounded(ret, neg, m2, f2, pref, dir) by {
        lemma_rounded_transfer::<W>(ret, neg, m1, f1, m2, f2, pref, dir);
    }
}

pub(crate) proof fn lemma_scaled_base(c1: int, e1: int, c2: int, e2: int, b: int)
    requires
        b <= e1,
        b <= e2,
    ensures
        scaled_le(c1, e1, c2, e2) == (c1 * pow(10, (e1 - b) as nat) <= c2 * pow(10, (e2 - b) as nat)),
{
    if e1 >= e2 {
        let d = (e1 - e2) as nat;
        let t = (e2 - b) as nat;
        lemma_pow_adds(10, d, t);
        assert((e1 - b) as nat == d + t);
        lemma_pow_positive(10, t);
        lemma_mul_is_associative(c1, pow(10, d), pow(10, t));
        if c1 * pow(10, d) <= c2 {
            lemma_mul_inequality(c1 * pow(10, d), c2, pow(10, t));
        } else {
            lemma_mul_strict_inequality(c2, c1 * pow(10, d), pow(10, t));
        }
    } else {
        let d = (e2 - e1) as nat;
        let t = (e1 - b) as nat;
        lemma_pow_adds(10, d, t);
        assert((e2 - b) as nat == d + t);
        lemma_pow_positive(10, t);
        lemma_mul_is_associative(c2, pow(10, d), pow(10, t));
        if c1 <= c2 * pow(10, d) {
            lemma_mul_inequality(c1, c2 * pow(10, d), pow(10, t));
        } else {
            lemma_mul_strict_inequality(c2 * pow(10, d), c1, pow(10, t));
        }
    }
}

pub(crate) proof fn lemma_scaled_same(c1: int, c2: int, e: int)
    ensures
        scaled_le(c1, e, c2, e) == (c1 <= c2),
{
    lemma_pow0(10);
    assert((e - e) as nat == 0);
    assert(c1 * pow(10, 0) == c1);
}

pub(crate) proof fn lemma_scaled_trans(c1: int, e1: int, c2: int, e2: int, c3: int, e3: int)
    requires
        scaled_le(c1, e1, c2, e2),
        scaled_le(c2, e2, c3, e3),
    ensures
        scaled_le(c1, e1, c3, e3),
{
    let b = if e1 <= e2 && e1 <= e3 {
        e1
    } else if e2 <= e3 {
        e2
    } else {
        e3
    };
    lemma_scaled_base(c1, e1, c2, e2, b);
    lemma_scaled_base(c2, e2, c3, e3, b);
    lemma_scaled_base(c1, e1, c3, e3, b);
}

pub(crate) proof fn lemma_scaled_total(c1: int, e1: int, c2: int, e2: int)
    ensures
        scaled_le(c1, e1, c2, e2) || scaled_le(c2, e2, c1, e1),
{
    let b = if e1 <= e2 {
        e1
    } else {
        e2
    };
    lemma_scaled_base(c1, e1, c2, e2, b);
    lemma_scaled_base(c2, e2, c1, e1, b);
}

pub(crate) proof fn lemma_same_side_eq(m1: int, f1: int, m2: int, f2: int, lay: crate::layout::Layout)
    requires
        scaled_eq(m1, f1, m2, f2),
    ensures
        same_side(m1, f1, m2, f2, lay),
{
    assert forall|c: int, e: int| #![trigger fits_above(c, e, lay)] fits_above(c, e, lay) implies (scaled_le(c, e, m1, f1) == scaled_le(c, e, m2, f2)) && (scaled_le(m1, f1, c, e) == scaled_le(m2, f2, c, e)) by {
        if scaled_le(c, e, m1, f1) {
            lemma_scaled_trans(c, e, m1, f1, m2, f2);
        }
        if scaled_le(c, e, m2, f2) {
            lemma_scaled_trans(c, e, m2, f2, m1, f1);
        }
        if scaled_le(m1, f1, c, e) {
            lemma_scaled_trans(m2, f2, m1, f1, c, e);
        }
        if scaled_le(m2, f2, c, e) {
            lemma_scaled_trans(m1, f1, m2, f2, c, e);
        }
    }
}

pub(crate) proof fn lemma_same_side_gap(c: int, e: int, m1: int, f1: int, m2: int, f2: int, lay: crate::layout::Layout)
    requires
        fit_layout(lay),
        0 <= c,
        e <= lay.qmin || pow(10, (lay.digits - 1) as nat) <= c,
        !scaled_le(m1, f1, c, e),
        !scaled_le(c + 1, e, m1, f1),
        !scaled_le(m2, f2, c, e),
        !scaled_le(c + 1, e, m2, f2),
    ensures
        same_side(m1, f1, m2, f2, lay),
{
    lemma_scaled_total(c, e, m1, f1);
    lemma_scaled_total(c, e, m2, f2);
    lemma_scaled_total(m1, f1, c + 1, e);
    lemma_scaled_total(m2, f2, c + 1, e);
    assert forall|c2: int, e2: int| #![trigger fits_above(c2, e2, lay)] fits_above(c2, e2, lay) implies (scaled_le(c2, e2, m1, f1) == scaled_le(c2, e2, m2, f2)) && (scaled_le(m1, f1, c2, e2) == scaled_le(m2, f2, c2, e2)) by {
        lemma_gap(c, e, lay, c2, e2);
        if scaled_le(c2, e2, c, e) {
            lemma_scaled_trans(c2, e2, c, e, m1, f1);
            lemma_scaled_trans(c2, e2, c, e, m2, f2);
            if scaled_le(m1, f1, c2, e2) {
                lemma_scaled_trans(m1, f1, c2, e2, c, e);
            }
            if scaled_le(m2, f2, c2, e2) {
                lemma_scaled_trans(m2, f2, c2, e2, c, e);
            }
        } else {
            lemma_scaled_trans(m1, f1, c + 1, e, c2, e2);
            lemma_scaled_trans(m2, f2, c + 1, e, c2, e2);
            if scaled_le(c2, e2, m1, f1) {
                lemma_scaled_trans(c + 1, e, c2, e2, m1, f1);
            }
            if scaled_le(c2, e2, m2, f2) {
                lemma_scaled_trans(c + 1, e, c2, e2, m2, f2);
            }
        }
    }
}

pub(crate) proof fn lemma_rounded_transfer<W: crate::layout::Width>(
    ret: Result<crate::layout::InRange<W>, crate::Error>,
    neg: bool,
    m1: int,
    f1: int,
    m2: int,
    f2: int,
    pref: int,
    dir: Option<crate::Rounding>,
)
    requires
        fit_layout(W::LAYOUT),
        rounded(ret, neg, m1, f1, pref, dir),
        same_side(m1, f1, m2, f2, W::LAYOUT),
    ensures
        rounded(ret, neg, m2, f2, pref, dir),
{
    let lay = W::LAYOUT;
    let p10 = pow(10, lay.digits as nat);
    let p1 = (lay.digits - 1) as nat;
    let low = pow(10, p1);
    let qmax = lay.qmax as int;
    lemma_pow10_step(p1);
    assert(p1 + 1 == lay.digits as nat);
    lemma_pow_positive(10, p1);
    match ret {
        Ok(r) => {
            let c = r.view().coeff as int;
            let e = r.view().exp as int;
            assert(fits_above(c, e, lay));
            assert forall|c2: int, e2: int| #![trigger fits_above(c2, e2, lay)] fits_above(c2, e2, lay) && scaled_le(m2, f2, c2, e2) implies scaled_le(m1, f1, c2, e2) by {}
            assert forall|c2: int, e2: int| #![trigger fits_above(c2, e2, lay)] fits_above(c2, e2, lay) && scaled_le(c2, e2, m2, f2) implies scaled_le(c2, e2, m1, f1) by {}
        },
        Err(crate::Error::Overflow) => {
            lemma_pow0(10);
            lemma_pow1(10);
            lemma_scaled_base(p10, qmax, low, qmax + 1, qmax);
            lemma_scaled_base(low, qmax + 1, p10, qmax, qmax);
            assert((qmax - qmax) as nat == 0);
            assert((qmax + 1 - qmax) as nat == 1);
            assert(p10 * pow(10, 0) == p10);
            lemma_mul_is_commutative(low, 10);
            assert(fits_above(low, qmax + 1, lay));
            assert(fits_above(p10 - 1, qmax, lay));
            if scaled_le(p10, qmax, m1, f1) {
                lemma_scaled_trans(low, qmax + 1, p10, qmax, m1, f1);
                lemma_scaled_trans(p10, qmax, low, qmax + 1, m2, f2);
            }
        },
        Err(crate::Error::Inexact) => {
            assert(fits_above(p10 - 1, qmax, lay));
            assert forall|c2: int, e2: int| #![trigger fits(c2, e2, lay)] fits(c2, e2, lay) implies !scaled_eq(c2, e2, m2, f2) by {
                assert(fits_above(c2, e2, lay));
            }
        },
        Err(_) => {},
    }
}

pub(crate) proof fn lemma_exact_bounds(c: int, e: int, m: int, f: int, lay: crate::layout::Layout)
    requires
        fits_above(c, e, lay),
        scaled_eq(c, e, m, f),
    ensures
        is_floor(c, e, m, f, lay),
        is_ceil(c, e, m, f, lay),
{
    assert forall|c2: int, e2: int| #![trigger fits_above(c2, e2, lay)] fits_above(c2, e2, lay) && scaled_le(c2, e2, m, f) implies scaled_le(c2, e2, c, e) by {
        lemma_scaled_trans(c2, e2, m, f, c, e);
    }
    assert forall|c2: int, e2: int| #![trigger fits_above(c2, e2, lay)] fits_above(c2, e2, lay) && scaled_le(m, f, c2, e2) implies scaled_le(c, e, c2, e2) by {
        lemma_scaled_trans(c, e, m, f, c2, e2);
    }
}

pub(crate) proof fn lemma_gap(q: int, x: int, lay: crate::layout::Layout, c2: int, e2: int)
    requires
        fit_layout(lay),
        0 <= q,
        x <= lay.qmin || pow(10, (lay.digits - 1) as nat) <= q,
        fits_above(c2, e2, lay),
    ensures
        scaled_le(c2, e2, q, x) || scaled_le(q + 1, x, c2, e2),
{
    if e2 >= x {
        lemma_scaled_base(c2, e2, q, x, x);
        lemma_scaled_base(q + 1, x, c2, e2, x);
        lemma_pow0(10);
        assert((x - x) as nat == 0);
        assert(q * pow(10, 0) == q);
        assert((q + 1) * pow(10, 0) == q + 1);
    } else {
        let p1 = (lay.digits - 1) as nat;
        let g = (x - e2) as nat;
        lemma_pow10_step(p1);
        assert(p1 + 1 == lay.digits as nat);
        lemma_pow1(10);
        lemma_pow_increases(10, 1, g);
        lemma_mul_inequality(10, pow(10, g), q);
        lemma_mul_is_commutative(10, q);
        lemma_mul_is_commutative(pow(10, g), q);
    }
}

pub(crate) proof fn lemma_overflow_low(c: int, e: int, m: int, f: int, lay: crate::layout::Layout)
    requires
        fit_layout(lay),
        scaled_le(c, e, m, f),
        pow(10, (lay.digits - 1) as nat) <= c,
        e > lay.qmax,
    ensures
        scaled_le(pow(10, lay.digits as nat), lay.qmax as int, m, f),
        !scaled_le(m, f, pow(10, lay.digits as nat) - 1, lay.qmax as int),
{
    let p = lay.digits as nat;
    let p1 = (lay.digits - 1) as nat;
    let g = (e - lay.qmax) as nat;
    lemma_pow10_step(p1);
    assert(p1 + 1 == p);
    lemma_pow1(10);
    lemma_pow_increases(10, 1, g);
    lemma_pow_positive(10, p1);
    lemma_mul_inequality(pow(10, p1), c, pow(10, g));
    lemma_mul_inequality(10, pow(10, g), pow(10, p1));
    lemma_mul_is_commutative(pow(10, g), pow(10, p1));
    lemma_mul_is_commutative(10, pow(10, p1));
    assert(scaled_le(pow(10, p), lay.qmax as int, c, e));
    lemma_scaled_trans(pow(10, p), lay.qmax as int, c, e, m, f);
    if scaled_le(m, f, pow(10, p) - 1, lay.qmax as int) {
        lemma_scaled_trans(pow(10, p), lay.qmax as int, m, f, pow(10, p) - 1, lay.qmax as int);
        lemma_scaled_same(pow(10, p), pow(10, p) - 1, lay.qmax as int);
    }
}

pub(crate) proof fn lemma_overflow_max(q: int, x: int, m: int, f: int, lay: crate::layout::Layout)
    requires
        fit_layout(lay),
        !scaled_le(m, f, q, x),
        pow(10, lay.digits as nat) - 1 <= q,
        x >= lay.qmax,
    ensures
        !scaled_le(m, f, pow(10, lay.digits as nat) - 1, lay.qmax as int),
{
    let mx = pow(10, lay.digits as nat) - 1;
    let g = (x - lay.qmax) as nat;
    lemma_pow_positive(10, lay.digits as nat);
    lemma_pow_positive(10, g);
    lemma_scaled_base(mx, lay.qmax as int, q, x, lay.qmax as int);
    lemma_pow0(10);
    assert((lay.qmax - lay.qmax) as nat == 0);
    assert(mx * pow(10, 0) == mx);
    lemma_mul_inequality(1, pow(10, g), q);
    lemma_mul_is_commutative(pow(10, g), q);
    if scaled_le(m, f, mx, lay.qmax as int) {
        lemma_scaled_trans(m, f, mx, lay.qmax as int, q, x);
    }
}

pub(crate) proof fn lemma_below_max(q: int, x: int, m: int, f: int, lay: crate::layout::Layout)
    requires
        fit_layout(lay),
        scaled_le(m, f, q + 1, x),
        0 <= q,
        q + 1 <= pow(10, lay.digits as nat),
        x <= lay.qmax,
        x == lay.qmax ==> q + 1 <= pow(10, lay.digits as nat) - 1,
    ensures
        scaled_le(m, f, pow(10, lay.digits as nat) - 1, lay.qmax as int),
{
    let p10 = pow(10, lay.digits as nat);
    lemma_scaled_base(q + 1, x, p10 - 1, lay.qmax as int, x);
    lemma_pow0(10);
    assert((x - x) as nat == 0);
    assert((q + 1) * pow(10, 0) == q + 1);
    if x < lay.qmax {
        let g = (lay.qmax - x) as nat;
        lemma_pow1(10);
        lemma_pow_increases(10, 1, g);
        lemma_pow_increases(10, 1, lay.digits as nat);
        lemma_mul_inequality(10, pow(10, g), p10 - 1);
        lemma_mul_is_commutative(pow(10, g), p10 - 1);
    } else {
        assert((p10 - 1) * pow(10, 0) == p10 - 1);
    }
    lemma_scaled_trans(m, f, q + 1, x, p10 - 1, lay.qmax as int);
}

pub(crate) proof fn lemma_decimal_digits_le(x: nat, p: nat)
    requires
        x < pow(10, p),
    ensures
        decimal_digits(x) <= p,
    decreases p,
{
    if x > 0 {
        lemma_pow0(10);
        assert(p > 0);
        lemma_pow10_step((p - 1) as nat);
        assert((p - 1) as nat + 1 == p);
        lemma_multiply_divide_lt(x as int, 10, pow(10, (p - 1) as nat));
        lemma_decimal_digits_le(x / 10, (p - 1) as nat);
    }
}

pub(crate) proof fn lemma_fit_split(c: int, s: bool, e: int, lay: crate::layout::Layout, d: int, k: int, kk: int, q: int, rem: int)
    requires
        fit_layout(lay),
        0 <= c < 0x8000_0000_0000_0000_0000_0000_0000_0000int,
        s ==> pow(10, (lay.digits - 1) as nat) <= c,
        0 <= d,
        c < pow(10, d as nat),
        d > 0 ==> pow(10, (d - 1) as nat) <= c,
        k >= (if d > lay.digits { d - lay.digits } else { 0 }),
        k >= lay.qmin - e,
        k == (if d > lay.digits { d - lay.digits } else { 0 }) || k == lay.qmin - e,
        kk == (if k < 39 { k } else { 39 }),
        c == q * pow(10, kk as nat) + rem,
        0 <= q,
        0 <= rem < pow(10, kk as nat),
    ensures
        fit_split(c, s, e, lay, k, q, rem),
{
    let p = lay.digits as nat;
    let p1 = (lay.digits - 1) as nat;
    lemma_pow10_38();
    lemma_pow10_step(38);
    lemma_pow0(10);
    if k > 39 {
        if q > 0 {
            lemma_mul_inequality(1, q, pow(10, 39));
        }
        assert(q == 0);
        lemma_pow_increases(10, 39, k as nat);
    }
    assert(c == q * pow(10, k as nat) + rem);
    assert(rem < pow(10, k as nat));
    lemma_pow_positive(10, k as nat);
    lemma_mul_inequality(1, pow(10, k as nat), q);
    lemma_mul_is_commutative(pow(10, k as nat), q);
    if q >= pow(10, p) {
        lemma_mul_inequality(pow(10, p), q, pow(10, k as nat));
        lemma_pow_adds(10, p, k as nat);
        lemma_pow_increases(10, d as nat, p + k as nat);
    }
    if e + k != lay.qmin && (rem != 0 || s) {
        if d > lay.digits {
            if q < pow(10, p1) {
                lemma_mul_inequality(q + 1, pow(10, p1), pow(10, k as nat));
                lemma_pow_adds(10, p1, k as nat);
                assert(p1 + k as nat == (d - 1) as nat);
                assert((q + 1) * pow(10, k as nat) == q * pow(10, k as nat) + pow(10, k as nat)) by (nonlinear_arith);
            }
        } else {
            assert(k == 0);
            assert(q * pow(10, 0) == q);
        }
    }
}

pub(crate) proof fn lemma_narrow_fit_split(c: int, s: bool, e: int, lay: crate::layout::Layout, d: int, k: int, q: int, rem: int)
    requires
        fit_layout(lay),
        0 <= c < 0x8000_0000_0000_0000int,
        s ==> pow(10, (lay.digits - 1) as nat) <= c,
        0 <= d,
        c < pow(10, d as nat),
        d > 0 ==> pow(10, (d - 1) as nat) <= c,
        k >= (if d > lay.digits { d - lay.digits } else { 0 }),
        k >= lay.qmin - e,
        k == (if d > lay.digits { d - lay.digits } else { 0 }) || k == lay.qmin - e,
        c == q * pow(10, k as nat) + rem,
        0 <= q,
        0 <= rem < pow(10, k as nat),
    ensures
        fit_split(c, s, e, lay, k, q, rem),
{
    if k > 39 {
        lemma_pow10_38();
        lemma_pow10_step(38);
        lemma_pow_increases(10, 39, k as nat);
        assert(q == 0) by (nonlinear_arith)
            requires c == q * pow(10, k as nat) + rem, q >= 0, rem >= 0,
                c < 0x8000_0000_0000_0000int,
                pow(10, k as nat) >= pow(10, 39), pow(10, 39) > 0x8000_0000_0000_0000int;
    }
    lemma_fit_split(c, s, e, lay, d, k, if k < 39 { k } else { 39 }, q, rem);
}

pub(crate) proof fn lemma_fit_between(c: int, s: bool, e: int, k: int, q: int, rem: int)
    requires
        k >= 0,
        c == q * pow(10, k as nat) + rem,
        0 <= rem < pow(10, k as nat),
    ensures ({
        let m = 10 * c + if s { 5int } else { 0int };
        &&& (rem == 0 && !s) ==> scaled_eq(q, e + k, m, e - 1)
        &&& (rem != 0 || s) ==> {
            &&& scaled_le(q, e + k, m, e - 1)
            &&& !scaled_le(m, e - 1, q, e + k)
            &&& scaled_le(m, e - 1, q + 1, e + k)
            &&& !scaled_le(q + 1, e + k, m, e - 1)
        }
    }),
{
    let t = pow(10, k as nat);
    let m = 10 * c + if s { 5int } else { 0int };
    lemma_pow10_step(k as nat);
    assert((e + k - (e - 1)) as nat == k as nat + 1);
    assert((e - 1 - (e - 1)) as nat == 0);
    lemma_scaled_base(q, e + k, m, e - 1, e - 1);
    lemma_scaled_base(m, e - 1, q, e + k, e - 1);
    lemma_scaled_base(m, e - 1, q + 1, e + k, e - 1);
    lemma_scaled_base(q + 1, e + k, m, e - 1, e - 1);
    assert(q * (10 * t) == 10 * (q * t)) by (nonlinear_arith);
    assert((q + 1) * (10 * t) == 10 * (q * t) + 10 * t) by (nonlinear_arith);
    assert(m * pow(10, 0) == m);
}

pub(crate) proof fn lemma_fit_zero(c: int, s: bool, e: int, lay: crate::layout::Layout, k: int, q: int, rem: int, pref: int)
    requires
        fit_layout(lay),
        fit_split(c, s, e, lay, k, q, rem),
        rem == 0,
        !s,
        q == 0,
        lay.qmin <= pref <= lay.qmax,
    ensures
        exact_result(0, pref, 10 * c + if s { 5int } else { 0int }, e - 1, pref, lay),
{
    lemma_pow_positive(10, lay.digits as nat);
    assert(c == 0);
    lemma_scaled_base(0, pref, 0, e - 1, if pref <= e - 1 { pref } else { e - 1 });
    lemma_scaled_base(0, e - 1, 0, pref, if pref <= e - 1 { pref } else { e - 1 });
    lemma_exact_bounds(0, pref, 0, e - 1, lay);
}

pub(crate) proof fn lemma_fit_scale(q: int, dq: int, lay: crate::layout::Layout, sh: int)
    requires
        fit_layout(lay),
        0 <= q < pow(10, dq as nat),
        0 <= dq,
        0 <= sh,
        dq + sh <= lay.digits,
    ensures
        q * pow(10, sh as nat) < pow(10, lay.digits as nat),
        pow(10, lay.digits as nat) <= 10_000_000_000_000_000_000_000_000_000_000_000_000int,
{
    lemma_pow_adds(10, dq as nat, sh as nat);
    lemma_pow_positive(10, sh as nat);
    lemma_mul_strict_inequality(q, pow(10, dq as nat), pow(10, sh as nat));
    lemma_pow_increases(10, dq as nat + sh as nat, lay.digits as nat);
    lemma_pow10_bounds(lay.digits as nat);
}

pub(crate) proof fn lemma_fit_exact(c: int, s: bool, e: int, lay: crate::layout::Layout, k: int, q: int, rem: int, pref: int, sh: int, dq: int, cc: int, ee: int)
    requires
        fit_layout(lay),
        fit_split(c, s, e, lay, k, q, rem),
        rem == 0,
        !s,
        lay.qmin <= pref <= lay.qmax,
        pref <= e + k,
        e + k <= pref ==> sh == 0,
        e + k > pref ==> {
            &&& 0 < q
            &&& 0 <= dq <= lay.digits
            &&& q < pow(10, dq as nat)
            &&& dq > 0 ==> pow(10, (dq - 1) as nat) <= q
            &&& sh == if e + k - pref <= lay.digits - dq { e + k - pref } else { lay.digits - dq }
        },
        cc == q * pow(10, sh as nat) || (sh == 0 && cc == q),
        ee == e + k - sh,
    ensures
        exact_result(cc, ee, 10 * c + if s { 5int } else { 0int }, e - 1, pref, lay),
{
    let m = 10 * c + if s { 5int } else { 0int };
    let x = e + k;
    let f = e - 1;
    let p = lay.digits as nat;
    let p1 = (lay.digits - 1) as nat;
    lemma_fit_between(c, s, e, k, q, rem);
    lemma_pow0(10);
    assert(q * pow(10, 0) == q);
    if sh == 0 {
        assert(sh as nat == 0);
    }
    assert(cc == q * pow(10, sh as nat));
    lemma_pow10_step(p1);
    assert(p1 + 1 == p);
    lemma_pow_positive(10, sh as nat);
    if x > pref {
        assert(dq > 0);
        lemma_fit_scale(q, dq, lay, sh);
        if sh == lay.digits - dq {
            lemma_mul_inequality(pow(10, (dq - 1) as nat), q, pow(10, sh as nat));
            lemma_pow_adds(10, (dq - 1) as nat, sh as nat);
            assert((dq - 1) as nat + sh as nat == p1);
        }
    }
    lemma_scaled_base(cc, ee, q, x, ee);
    lemma_scaled_base(q, x, cc, ee, ee);
    assert((ee - ee) as nat == 0);
    assert((x - ee) as nat == sh as nat);
    assert(cc * pow(10, 0) == cc);
    lemma_scaled_trans(cc, ee, q, x, m, f);
    lemma_scaled_trans(m, f, q, x, cc, ee);
    lemma_exact_bounds(cc, ee, m, f, lay);
    if ee > lay.qmax {
        lemma_overflow_low(cc, ee, m, f, lay);
    }
}

pub(crate) proof fn lemma_fit_none(c: int, s: bool, e: int, lay: crate::layout::Layout, k: int, q: int, rem: int)
    requires
        fit_layout(lay),
        fit_split(c, s, e, lay, k, q, rem),
        rem != 0 || s,
    ensures ({
        let m = 10 * c + if s { 5int } else { 0int };
        let p10 = pow(10, lay.digits as nat);
        if e + k > lay.qmax || (e + k == lay.qmax && q >= p10 - 1) {
            !scaled_le(m, e - 1, p10 - 1, lay.qmax as int)
        } else {
            &&& scaled_le(m, e - 1, p10 - 1, lay.qmax as int)
            &&& forall|c2: int, e2: int| #![trigger fits(c2, e2, lay)] fits(c2, e2, lay) ==> !scaled_eq(c2, e2, m, e - 1)
        }
    }),
{
    let m = 10 * c + if s { 5int } else { 0int };
    let f = e - 1;
    let x = e + k;
    let p10 = pow(10, lay.digits as nat);
    lemma_fit_between(c, s, e, k, q, rem);
    if x > lay.qmax {
        lemma_overflow_low(q, x, m, f, lay);
    } else if x == lay.qmax && q >= p10 - 1 {
        lemma_overflow_max(q, x, m, f, lay);
    } else {
        lemma_below_max(q, x, m, f, lay);
        assert forall|c2: int, e2: int| #![trigger fits(c2, e2, lay)] fits(c2, e2, lay) implies !scaled_eq(c2, e2, m, f) by {
            lemma_gap(q, x, lay, c2, e2);
            if scaled_le(c2, e2, q, x) && scaled_le(m, f, c2, e2) {
                lemma_scaled_trans(m, f, c2, e2, q, x);
            }
            if scaled_le(q + 1, x, c2, e2) && scaled_le(c2, e2, m, f) {
                lemma_scaled_trans(q + 1, x, c2, e2, m, f);
            }
        }
    }
}

pub(crate) proof fn lemma_fit_down(c: int, s: bool, e: int, lay: crate::layout::Layout, k: int, q: int, rem: int)
    requires
        fit_layout(lay),
        fit_split(c, s, e, lay, k, q, rem),
        rem != 0 || s,
    ensures
        floor_result(q, e + k, 10 * c + if s { 5int } else { 0int }, e - 1, lay),
{
    let m = 10 * c + if s { 5int } else { 0int };
    let f = e - 1;
    let x = e + k;
    lemma_fit_between(c, s, e, k, q, rem);
    assert forall|c2: int, e2: int| #![trigger fits_above(c2, e2, lay)] fits_above(c2, e2, lay) && scaled_le(c2, e2, m, f) implies scaled_le(c2, e2, q, x) by {
        lemma_gap(q, x, lay, c2, e2);
        if scaled_le(q + 1, x, c2, e2) {
            lemma_scaled_trans(q + 1, x, c2, e2, m, f);
        }
    }
    lemma_pow10_step((lay.digits - 1) as nat);
    assert((lay.digits - 1) as nat + 1 == lay.digits as nat);
    if x > lay.qmax {
        lemma_overflow_low(q, x, m, f, lay);
    }
}

pub(crate) proof fn lemma_fit_up(c: int, s: bool, e: int, lay: crate::layout::Layout, k: int, q: int, rem: int)
    requires
        fit_layout(lay),
        fit_split(c, s, e, lay, k, q, rem),
        rem != 0 || s,
    ensures ({
        let m = 10 * c + if s { 5int } else { 0int };
        let p10 = pow(10, lay.digits as nat);
        &&& q + 1 <= p10
        &&& q + 1 < p10 ==> ceil_result(q + 1, e + k, m, e - 1, lay)
        &&& q + 1 == p10 ==> ceil_result(pow(10, (lay.digits - 1) as nat), e + k + 1, m, e - 1, lay)
    }),
{
    let m = 10 * c + if s { 5int } else { 0int };
    let f = e - 1;
    let x = e + k;
    let p10 = pow(10, lay.digits as nat);
    let p1 = (lay.digits - 1) as nat;
    let low = pow(10, p1);
    lemma_fit_between(c, s, e, k, q, rem);
    lemma_pow10_step(p1);
    assert(p1 + 1 == lay.digits as nat);
    lemma_pow_positive(10, p1);
    if q + 1 < p10 {
        assert forall|c2: int, e2: int| #![trigger fits_above(c2, e2, lay)] fits_above(c2, e2, lay) && scaled_le(m, f, c2, e2) implies scaled_le(q + 1, x, c2, e2) by {
            lemma_gap(q, x, lay, c2, e2);
            if scaled_le(c2, e2, q, x) {
                lemma_scaled_trans(m, f, c2, e2, q, x);
            }
        }
        if x > lay.qmax {
            lemma_overflow_low(q, x, m, f, lay);
        }
    } else {
        lemma_pow1(10);
        lemma_pow0(10);
        lemma_scaled_base(low, x + 1, q + 1, x, x);
        lemma_scaled_base(q + 1, x, low, x + 1, x);
        assert((x + 1 - x) as nat == 1);
        assert((x - x) as nat == 0);
        assert((q + 1) * pow(10, 0) == q + 1);
        lemma_mul_is_commutative(low, 10);
        lemma_scaled_trans(m, f, q + 1, x, low, x + 1);
        assert forall|c2: int, e2: int| #![trigger fits_above(c2, e2, lay)] fits_above(c2, e2, lay) && scaled_le(m, f, c2, e2) implies scaled_le(low, x + 1, c2, e2) by {
            lemma_gap(q, x, lay, c2, e2);
            if scaled_le(c2, e2, q, x) {
                lemma_scaled_trans(m, f, c2, e2, q, x);
            }
            lemma_scaled_trans(low, x + 1, q + 1, x, c2, e2);
        }
        if scaled_le(low, x + 1, m, f) {
            lemma_scaled_trans(q + 1, x, low, x + 1, m, f);
        }
        if x + 1 > lay.qmax {
            lemma_overflow_max(q, x, m, f, lay);
        }
    }
}

pub proof fn lemma_reciprocal_exact(x: int, d: int, m: int, p: int, e: int)
    requires
        0 <= x,
        0 < d,
        0 < p,
        0 <= e < d,
        m * d == p + e,
        x * e < p,
    ensures
        (x * m) / p == x / d,
{
    let q = x / d;
    let r = x % d;
    lemma_fundamental_div_mod(x, d);
    assert(0 <= r < d);
    assert(q >= 0) by { lemma_div_pos_is_pos(x, d); }
    assert(q * d * p <= x * p) by {
        lemma_mul_inequality(q * d, x, p);
    }
    assert((x * m) * d == x * p + x * e) by (nonlinear_arith)
        requires m * d == p + e;
    assert(x * e >= 0) by (nonlinear_arith) requires x >= 0, e >= 0;
    assert(q * p * d <= (x * m) * d) by (nonlinear_arith)
        requires q * d * p <= x * p, (x * m) * d == x * p + x * e, x * e >= 0;
    assert(q * p <= x * m) by (nonlinear_arith)
        requires q * p * d <= (x * m) * d, d > 0;
    assert(r * p + x * e < d * p) by (nonlinear_arith)
        requires r + 1 <= d, x * e < p, p > 0, r >= 0;
    assert((x * m) * d < (q + 1) * p * d) by (nonlinear_arith)
        requires (x * m) * d == x * p + x * e, x == d * q + r, r * p + x * e < d * p;
    assert(x * m < (q + 1) * p) by (nonlinear_arith)
        requires (x * m) * d < (q + 1) * p * d, d > 0;
    assert(m > 0) by (nonlinear_arith) requires m * d == p + e, d > 0, p > 0, e >= 0;
    assert(x * m >= 0) by (nonlinear_arith) requires x >= 0, m > 0;
    assert((x * m) / p == q) by {
        lemma_div_pos_is_pos(x * m, p);
        lemma_fundamental_div_mod(x * m, p);
        let t = (x * m) / p;
        assert(t * p <= x * m) by (nonlinear_arith)
            requires x * m == p * t + (x * m) % p, (x * m) % p >= 0;
        assert(x * m < (t + 1) * p) by (nonlinear_arith)
            requires x * m == p * t + (x * m) % p, (x * m) % p < p;
        assert(t == q) by (nonlinear_arith)
            requires t * p <= x * m, x * m < (t + 1) * p, q * p <= x * m, x * m < (q + 1) * p, p > 0;
    }
}

pub proof fn lemma_div_rem_pow10_64_edges(x: u64, k: u32)
    ensures
        k == 0 ==> pow(10, k as nat) == 1,
        k >= 20 ==> (x as int) < pow(10, k as nat),
{
    lemma_pow0(10);
    if k >= 20 {
        reveal_with_fuel(pow, 21);
        assert(pow(10, 20) == 100_000_000_000_000_000_000int);
        lemma_pow_increases(10, 20, k as nat);
    }
}

pub proof fn lemma_div_rem_pow10_64_main(x: u64, k: u32, m: u64, post: u32, q: u64)
    requires
        x < 0x8000_0000_0000_0000u64,
        1 <= k <= 19,
        post < 64,
        pow2(post as nat) <= pow(10, k as nat),
        pow(10, k as nat) < pow2(post as nat + 1),
        m as int * pow(10, k as nat) >= pow2(64 + post as nat),
        m as int * pow(10, k as nat) - pow2(64 + post as nat) < pow(10, k as nat),
        q == (#[verifier::truncate] (((x as int * m as int + 0) / 0x1_0000_0000_0000_0000int) as u64)) >> post,
    ensures
        q as int == x as int / pow(10, k as nat),
        q as int * pow(10, k as nat) <= x as int,
        x as int - q as int * pow(10, k as nat) < pow(10, k as nat),
{
    let d = pow(10, k as nat);
    let p = pow2(64 + post as nat) as int;
    let e = m as int * d - p;
    let xi = x as int;
    let mi = m as int;
    lemma2_to64();
    lemma2_to64_rest();
    lemma_pow2_pos(post as nat);
    lemma_pow2_adds(63, post as nat + 1);
    lemma_pow2_adds(64, post as nat);
    assert(63 + (post as nat + 1) == 64 + post as nat);
    assert(pow2(63) == 0x8000_0000_0000_0000int);
    assert(xi * e < p) by (nonlinear_arith)
        requires 0 <= xi < pow2(63), 0 <= e < d, d < pow2(post as nat + 1),
            p == pow2(63) * pow2(post as nat + 1);
    lemma_reciprocal_exact(xi, d, mi, p, e);
    let two64 = 0x1_0000_0000_0000_0000int;
    let h = (xi * mi) / two64;
    assert(xi * mi >= 0) by (nonlinear_arith) requires xi >= 0, mi >= 0;
    assert(xi * mi < two64 * two64) by (nonlinear_arith)
        requires 0 <= xi < two64, 0 <= mi < two64;
    assert(0 <= h < two64) by {
        lemma_div_pos_is_pos(xi * mi, two64);
        lemma_multiply_divide_lt(xi * mi, two64, two64);
    }
    lemma_div_denominator(xi * mi, two64, pow2(post as nat) as int);
    let hu = #[verifier::truncate] (h as u64);
    assert(hu as int == h);
    let pu = post as u64;
    assert(hu >> post == hu >> pu) by (bit_vector)
        requires pu == post as u64, post < 64;
    vstd::bits::lemma_u64_shr_is_div(hu, pu);
    assert(q as int == xi / d);
    lemma_fundamental_div_mod(xi, d);
}

pub proof fn lemma_div_rem_pow10_edges(x: u128, k: u32)
    requires
        x < 0x8000_0000_0000_0000_0000_0000_0000_0000u128,
    ensures
        k == 0 ==> pow(10, k as nat) == 1,
        k >= 39 ==> (x as int) < pow(10, k as nat),
{
    lemma_pow0(10);
    if k >= 39 {
        reveal_with_fuel(pow, 40);
        assert(pow(10, 39) == 1_000_000_000_000_000_000_000_000_000_000_000_000_000int);
        lemma_pow_increases(10, 39, k as nat);
    }
}

pub proof fn lemma_div_rem_pow10_main(x: u128, k: u32, m: u128, post: u32, q: u128)
    requires
        x < 0x8000_0000_0000_0000_0000_0000_0000_0000u128,
        recip_spec(k as int, m as int, post as int),
        q == (#[verifier::truncate] (((x as int * m as int + 0) / 0x1_0000_0000_0000_0000_0000_0000_0000_0000int) as u128)) >> post,
    ensures
        q as int == x as int / pow(10, k as nat),
        q as int * pow(10, k as nat) <= x as int,
        x as int - q as int * pow(10, k as nat) < pow(10, k as nat),
{
    let d = pow(10, k as nat);
    let p = pow2(128 + post as nat) as int;
    let e = m as int * d - p;
    let xi = x as int;
    let mi = m as int;
    lemma_pow2_128();
    lemma_pow2_pos(post as nat);
    lemma_pow2_adds(127, post as nat + 1);
    assert(127 + (post as nat + 1) == 128 + post as nat);
    assert(xi * e < p) by (nonlinear_arith)
        requires 0 <= xi < pow2(127), 0 <= e < d, d < pow2(post as nat + 1),
            p == pow2(127) * pow2(post as nat + 1);
    lemma_reciprocal_exact(xi, d, mi, p, e);
    let w = 0x1_0000_0000_0000_0000_0000_0000_0000_0000int;
    let h = (xi * mi) / w;
    assert(xi * mi >= 0) by (nonlinear_arith) requires xi >= 0, mi >= 0;
    lemma_mul_below(xi, mi, w);
    assert(0 <= h < w) by {
        lemma_div_pos_is_pos(xi * mi, w);
        lemma_multiply_divide_lt(xi * mi, w, w);
    }
    lemma_pow2_adds(128, post as nat);
    lemma_div_denominator(xi * mi, w, pow2(post as nat) as int);
    let hu = #[verifier::truncate] (h as u128);
    assert(hu as int == h);
    let pu = post as u128;
    assert(hu >> post == hu >> pu) by (bit_vector)
        requires pu == post as u128, post < 128;
    vstd::bits::lemma_u128_shr_is_div(hu, pu);
    assert(q as int == xi / d);
    lemma_fundamental_div_mod(xi, d);
}

pub open spec fn decimal_digits(x: nat) -> nat
    decreases x,
{
    if x == 0 {
        0
    } else {
        1 + decimal_digits(x / 10)
    }
}

pub proof fn lemma_decimal_digits(x: nat, n: nat)
    requires
        x < pow(10, n),
        n > 0 ==> pow(10, (n - 1) as nat) <= x,
    ensures
        decimal_digits(x) == n,
    decreases n,
{
    if n == 0 {
        lemma_pow0(10);
    } else {
        let m = (n - 1) as nat;
        lemma_pow_positive(10, m);
        lemma_pow1(10);
        lemma_pow_adds(10, 1, m);
        assert(1 + m == n);
        assert(pow(10, n) == 10 * pow(10, m));
        if m == 0 {
            lemma_pow0(10);
            assert(x / 10 == 0);
            assert(decimal_digits(0) == 0);
        } else {
            let j = (m - 1) as nat;
            lemma_pow_adds(10, 1, j);
            assert(1 + j == m);
            assert(pow(10, m) == 10 * pow(10, j));
            lemma_multiply_divide_lt(x as int, 10, pow(10, m));
            lemma_div_is_ordered(10 * pow(10, j), x as int, 10);
            lemma_div_multiples_vanish(pow(10, j), 10);
            lemma_decimal_digits(x / 10, m);
        }
    }
}

pub proof fn lemma_digits_result(x: int, t: nat)
    requires
        0 <= x < pow(10, t + 1),
        t > 0 ==> pow(10, (t - 1) as nat) <= x,
    ensures
        x >= pow(10, t) ==> decimal_digits(x as nat) == t + 1,
        x < pow(10, t) ==> decimal_digits(x as nat) == t,
{
    if x >= pow(10, t) {
        lemma_decimal_digits(x as nat, t + 1);
    } else {
        lemma_decimal_digits(x as nat, t);
    }
}

pub open spec fn digits_estimate_ok(b: nat) -> bool {
    let t = (b * 1233) / 4096;
    &&& (t > 0 ==> pow(10, (t - 1) as nat) <= pow(2, (b - 1) as nat))
    &&& pow(2, b) <= pow(10, (t + 1) as nat)
}

pub open spec fn digits_estimates_ok(n: nat) -> bool
    decreases n,
{
    if n == 0 {
        true
    } else {
        digits_estimate_ok(n) && digits_estimates_ok((n - 1) as nat)
    }
}

pub proof fn lemma_digits_estimates_below(n: nat, b: nat)
    requires
        digits_estimates_ok(n),
        1 <= b <= n,
    ensures
        digits_estimate_ok(b),
    decreases n,
{
    if b < n {
        lemma_digits_estimates_below((n - 1) as nat, b);
    }
}

pub proof fn lemma_digits_estimate(x: int, b: nat, t: int)
    requires
        b <= 128,
        b == 0 ==> x == 0,
        b >= 1 ==> pow2((b - 1) as nat) <= x < pow2(b),
        t == (b * 1233) / 4096,
    ensures
        0 <= t <= 38,
        b <= 64 ==> t <= 19,
        x < pow(10, (t + 1) as nat),
        t > 0 ==> pow(10, (t - 1) as nat) <= x,
{
    if b == 0 {
        lemma_pow1(10);
    } else {
        assert(digits_estimates_ok(128)) by (compute_only);
        lemma_digits_estimates_below(128, b);
        lemma_pow_positive(2, b);
        lemma_pow_positive(2, (b - 1) as nat);
        assert(pow2(b) == pow(2, b));
        assert(pow2((b - 1) as nat) == pow(2, (b - 1) as nat));
    }
}

pub proof fn lemma_pow10_38()
    ensures
        pow(10, 38) == 100_000_000_000_000_000_000_000_000_000_000_000_000int,
{
    reveal_with_fuel(pow, 39);
}

pub proof fn lemma_scaled_rem_direct(cx: int, d: nat, n: nat)
    requires
        0 <= cx < pow(10, n),
        n + d <= 38,
    ensures
        0 <= cx * pow(10, d) < 0x1_0000_0000_0000_0000_0000_0000_0000_0000int,
{
    let p = pow(10, d);
    lemma_pow_positive(10, d);
    lemma_pow_adds(10, n, d);
    lemma_mul_strict_inequality(cx, pow(10, n), p);
    lemma_mul_nonnegative(cx, p);
    if n + d < 38 {
        lemma_pow_increases(10, n + d, 38);
    }
    lemma_pow10_38();
}

pub proof fn lemma_scaled_rem_step(cx: int, cs: int, done: nat, k: nat, r: int)
    requires
        0 <= cx,
        0 < cs < 10_000_000_000_000_000_000_000_000_000_000_000int,
        1 <= k <= 4,
        0 <= r < cs,
        r == (cx * pow(10, done)) % cs,
    ensures
        0 <= r * pow(10, k) < 0x1_0000_0000_0000_0000_0000_0000_0000_0000int,
        (r * pow(10, k)) % cs == (cx * pow(10, done + k)) % cs,
{
    lemma_pow_positive(10, k);
    lemma_pow_increases(10, k, 4);
    reveal_with_fuel(pow, 5);
    assert(0 <= r * pow(10, k) <= r * 10_000) by (nonlinear_arith)
        requires 0 <= r, 0 < pow(10, k) <= 10_000;
    lemma_mul_mod_noop_left(cx * pow(10, done), pow(10, k), cs);
    lemma_pow_adds(10, done, k);
    lemma_mul_is_associative(cx, pow(10, done), pow(10, k));
}

pub proof fn lemma_scaled_rem_start(cx: int, cs: int)
    requires
        0 <= cx,
        0 < cs,
    ensures
        cx % cs == (cx * pow(10, 0)) % cs,
        0 <= cx % cs < cs,
{
    lemma_pow0(10);
    lemma_mod_bound(cx, cs);
}

pub proof fn lemma_div_scaled_start(a: int, b: int, n: nat)
    requires
        0 <= a,
        0 < b < 10_000_000_000_000_000_000_000_000_000_000_000int,
        n == decimal_digits(b as nat),
    ensures
        1 <= n <= 34,
        (a / b) * b + a % b == a * pow(10, 0),
        0 <= a % b < b,
        0 <= a / b,
{
    reveal_with_fuel(pow, 35);
    assert(pow(10, 34) == 10_000_000_000_000_000_000_000_000_000_000_000int);
    lemma_decimal_digits_le(b as nat, 34);
    assert(decimal_digits(b as nat) >= 1);
    lemma_pow0(10);
    lemma_fundamental_div_mod(a, b);
    lemma_mod_bound(a, b);
    lemma_div_pos_is_pos(a, b);
    lemma_mul_is_commutative(a / b, b);
}

pub proof fn lemma_div_scaled_step(a: int, b: int, k: nat, done: nat, c: nat, step: nat, q: int, r: int)
    requires
        0 <= a,
        0 < b < pow(10, (38 - step) as nat),
        1 <= c <= step <= 37,
        done + c <= k,
        0 <= q,
        0 <= r < b,
        q * b + r == a * pow(10, done),
        a * pow(10, k) < b * pow(10, 35),
    ensures
        0 <= r * pow(10, c) < 0x1_0000_0000_0000_0000_0000_0000_0000_0000int,
        0 <= q * pow(10, c) + (r * pow(10, c)) / b < 0x1_0000_0000_0000_0000_0000_0000_0000_0000int,
        0 <= (r * pow(10, c)) % b < b,
        (q * pow(10, c) + (r * pow(10, c)) / b) * b + (r * pow(10, c)) % b == a * pow(10, done + c),
{
    let pc = pow(10, c);
    let rest = (k - done) as nat;
    let t = r * pc;
    lemma_pow_positive(10, c);
    lemma_pow_positive(10, rest);
    lemma_pow_positive(10, done);
    lemma_pow_increases(10, c, step);
    lemma_pow_increases(10, c, rest);
    lemma_pow_adds(10, (38 - step) as nat, step);
    lemma_pow_adds(10, done, rest);
    lemma_pow_adds(10, done, c);
    lemma_pow10_38();
    lemma_pow10_bounds(c);
    reveal_with_fuel(pow, 36);
    assert(pow(10, 35) == 100_000_000_000_000_000_000_000_000_000_000_000int);
    assert((38 - step) as nat + step == 38);
    assert(done + rest == k);
    assert(0 <= t < b * pc) by (nonlinear_arith)
        requires
            0 <= r < b,
            0 < pc,
            t == r * pc,
    ;
    assert(b * pc <= pow(10, (38 - step) as nat) * pow(10, step)) by (nonlinear_arith)
        requires
            0 < b < pow(10, (38 - step) as nat),
            0 < pc <= pow(10, step),
    ;
    lemma_fundamental_div_mod(t, b);
    lemma_mod_bound(t, b);
    lemma_div_pos_is_pos(t, b);
    let d = t / b;
    assert(d < pc) by (nonlinear_arith)
        requires
            t == b * d + t % b,
            0 <= t % b,
            t < b * pc,
            0 < b,
    ;
    assert(q * pow(10, rest) < pow(10, 35)) by (nonlinear_arith)
        requires
            a * pow(10, k) < b * pow(10, 35),
            pow(10, k) == pow(10, done) * pow(10, rest),
            q * b + r == a * pow(10, done),
            0 <= r,
            0 <= q,
            0 < b,
            0 < pow(10, rest),
    ;
    assert(0 <= q * pc <= q * pow(10, rest)) by (nonlinear_arith)
        requires
            0 <= q,
            0 < pc <= pow(10, rest),
    ;
    assert((q * pc + d) * b + t % b == a * pow(10, done + c)) by (nonlinear_arith)
        requires
            t == b * d + t % b,
            t == r * pc,
            q * b + r == a * pow(10, done),
            pow(10, done + c) == pow(10, done) * pc,
    ;
}

pub proof fn lemma_div_scaled_end(a: int, b: int, k: nat, q: int, r: int)
    requires
        0 < b,
        0 <= r < b,
        q * b + r == a * pow(10, k),
    ensures
        q == (a * pow(10, k)) / b,
        r == (a * pow(10, k)) % b,
{
    lemma_fundamental_div_mod_converse(a * pow(10, k), b, q, r);
}

pub open spec fn strip_scale(k: int) -> int {
    if k == 0 { 10 } else { pow(10, (2 * k) as nat) }
}

pub open spec fn strip_step_ok(k: int) -> bool {
    k == 32 || k == 16 || k == 8 || k == 4 || k == 2 || k == 1 || k == 0
}

pub proof fn lemma_strip_start(c: int)
    requires
        0 < c < 0x8000_0000_0000_0000_0000_0000_0000_0000int,
    ensures
        c % strip_scale(32) != 0,
        c == c * pow(10, 0),
{
    lemma_pow0(10);
    reveal_with_fuel(pow, 40);
    assert(pow(10, 39) == 1_000_000_000_000_000_000_000_000_000_000_000_000_000int);
    lemma_pow_increases(10, 39, 64);
    lemma_small_mod(c as nat, pow(10, 64) as nat);
}

pub proof fn lemma_strip_start_64(c: int)
    requires
        0 < c < 10_000_000_000_000_000int,
    ensures
        c % strip_scale(8) != 0,
        c == c * pow(10, 0),
{
    lemma_pow0(10);
    reveal_with_fuel(pow, 17);
    lemma_small_mod(c as nat, pow(10, 16) as nat);
}

pub proof fn lemma_strip_end_64(orig: int, c: int, zeros: nat)
    requires
        0 < orig < 10_000_000_000_000_000int,
        c > 0,
        orig == c * pow(10, zeros),
    ensures
        zeros <= 15,
{
    lemma_pow_positive(10, zeros);
    assert(pow(10, zeros) <= orig) by (nonlinear_arith)
        requires orig == c * pow(10, zeros), c >= 1, pow(10, zeros) > 0;
    if zeros > 15 {
        reveal_with_fuel(pow, 17);
        lemma_pow_increases(10, 16, zeros);
        assert(false);
    }
}

pub proof fn lemma_strip_step(orig: int, c: int, zeros: nat, k: nat, q: int, r: int)
    requires
        k >= 1,
        strip_step_ok(k as int),
        c > 0,
        orig == c * pow(10, zeros),
        c == q * pow(10, k) + r,
        0 <= r < pow(10, k),
        c % strip_scale(k as int) != 0,
    ensures
        r == 0 ==> 0 < q <= c,
        r == 0 ==> orig == q * pow(10, zeros + k),
        r == 0 ==> q % strip_scale(k as int / 2) != 0,
        r != 0 ==> c % strip_scale(k as int / 2) != 0,
{
    let d = pow(10, k);
    lemma_pow_positive(10, k);
    lemma_pow_adds(10, k, k);
    assert(k + k == 2 * k);
    if k == 1 {
        lemma_pow1(10);
    } else {
        assert(2 * (k / 2) == k);
    }
    assert(strip_scale(k as int / 2) == d);
    if r == 0 {
        assert(q > 0) by (nonlinear_arith) requires c == q * d, c > 0, d > 0;
        assert(q <= c) by (nonlinear_arith) requires c == q * d, q > 0, d >= 1;
        lemma_pow_adds(10, zeros, k);
        assert(orig == q * pow(10, zeros + k)) by (nonlinear_arith)
            requires orig == c * pow(10, zeros), c == q * d, pow(10, zeros + k) == pow(10, zeros) * d;
        if q % d == 0 {
            lemma_fundamental_div_mod(q, d);
            let t = q / d;
            assert(c == t * (d * d)) by (nonlinear_arith) requires c == q * d, q == d * t + 0;
            assert(d * d > 0) by (nonlinear_arith) requires d > 0;
            lemma_mod_multiples_basic(t, d * d);
            assert(false);
        }
    } else {
        lemma_fundamental_div_mod_converse(c, d, q, r);
    }
}

pub proof fn lemma_strip_end(orig: int, c: int, zeros: nat)
    requires
        0 < orig < 0x8000_0000_0000_0000_0000_0000_0000_0000int,
        c > 0,
        orig == c * pow(10, zeros),
    ensures
        zeros <= 38,
{
    lemma_pow_positive(10, zeros);
    assert(pow(10, zeros) <= orig) by (nonlinear_arith)
        requires orig == c * pow(10, zeros), c >= 1, pow(10, zeros) > 0;
    if zeros > 38 {
        reveal_with_fuel(pow, 40);
        lemma_pow_increases(10, 39, zeros);
        assert(false);
    }
}

pub open spec fn recip64_spec(k: int, m: int, post: int) -> bool {
    &&& 1 <= k <= 19
    &&& 0 <= m < 0x1_0000_0000_0000_0000int
    &&& 0 <= post < 64
    &&& pow2(post as nat) <= pow(10, k as nat)
    &&& pow(10, k as nat) < pow2(post as nat + 1)
    &&& m * pow(10, k as nat) >= pow2(64 + post as nat)
    &&& m * pow(10, k as nat) - pow2(64 + post as nat) < pow(10, k as nat)
}

pub proof fn lemma_pow10_step(i: nat)
    ensures
        pow(10, 0) == 1,
        pow(10, i + 1) == 10 * pow(10, i),
{
    lemma_pow0(10);
    reveal(pow);
}

pub proof fn lemma_pow10_bounds(i: nat)
    ensures
        i <= 18 ==> pow(10, i) <= 1_000_000_000_000_000_000int,
        i <= 37 ==> pow(10, i) <= 10_000_000_000_000_000_000_000_000_000_000_000_000int,
{
    reveal_with_fuel(pow, 38);
    assert(pow(10, 18) == 1_000_000_000_000_000_000int);
    assert(pow(10, 37) == 10_000_000_000_000_000_000_000_000_000_000_000_000int);
    if i <= 18 {
        lemma_pow_increases(10, i, 18);
    }
    if i <= 37 {
        lemma_pow_increases(10, i, 37);
    }
}

pub proof fn lemma_pow10_facts(k: nat)
    requires
        1 <= k <= 19,
    ensures
        10 <= pow(10, k),
        pow(10, k) < 0x1_0000_0000_0000_0000int,
        pow(10, k) % 5 == 0,
{
    reveal_with_fuel(pow, 20);
    assert(pow(10, 19) == 10_000_000_000_000_000_000int);
    lemma_pow_increases(10, k, 19);
    lemma_pow_increases(10, 1, k);
    lemma_pow10_step((k - 1) as nat);
    assert(pow(10, k) == 10 * pow(10, (k - 1) as nat));
    assert((10 * pow(10, (k - 1) as nat)) % 5 == 0) by {
        lemma_mul_is_associative(5, 2, pow(10, (k - 1) as nat));
        lemma_mod_multiples_basic(2 * pow(10, (k - 1) as nat), 5);
    }
}

pub proof fn lemma_pow2_not_multiple_of_5(e: nat)
    ensures
        pow2(e) % 5 != 0,
    decreases e,
{
    if e == 0 {
        lemma2_to64();
    } else {
        lemma_pow2_not_multiple_of_5((e - 1) as nat);
        lemma_pow2_unfold(e);
        let a = pow2((e - 1) as nat) as int;
        lemma_pow2_pos((e - 1) as nat);
        lemma_fundamental_div_mod(a, 5);
        let q = a / 5;
        let r = a % 5;
        assert(2 * a == 5 * (2 * q) + 2 * r) by (nonlinear_arith)
            requires a == 5 * q + r;
        lemma_mod_multiples_vanish(2 * q, 2 * r, 5);
        assert((2 * r) % 5 != 0) by {
            assert(1 <= r < 5);
            if r == 1 {} else if r == 2 {} else if r == 3 {} else {}
        }
    }
}

pub proof fn lemma_double_power(power: u64, post: u32, d: u64)
    requires
        power as int == pow2(post as nat),
        1 <= power,
        power <= d / 2,
        post <= 63,
    ensures
        2 * power <= d,
        2 * power == pow2(post as nat + 1),
        post + 1 <= 63,
{
    lemma_pow2_unfold(post as nat + 1);
    lemma2_to64();
    if post + 1 > 63 {
        lemma_pow2_strictly_increases(63, post as nat + 1);
        lemma_pow2_unfold(64);
        assert(pow2(64) == 0x1_0000_0000_0000_0000int);
        assert(post == 63);
    }
}

pub proof fn lemma_reciprocal_64_entry(k: u32, power: u64, post: u32, numerator: u128, d: u128)
    requires
        1 <= k <= 19,
        d as int == pow(10, k as nat),
        power as int == pow2(post as nat),
        1 <= power <= d,
        d < 2 * power,
        post <= 63,
        numerator == (power as u128) << 64,
    ensures
        ({
            let m = numerator as int / d as int + if numerator as int % d as int != 0 { 1int } else { 0int };
            &&& m < 0x1_0000_0000_0000_0000int
            &&& recip64_spec(k as int, m, post as int)
        }),
{
    let two64 = 0x1_0000_0000_0000_0000int;
    let pw = power as u128;
    assert(pw < 0x1_0000_0000_0000_0000u128 ==> pw << 64 == pw * 0x1_0000_0000_0000_0000u128) by (bit_vector);
    let n = numerator as int;
    let di = d as int;
    let p = power as int;
    assert(n == p * two64);
    lemma2_to64();
    lemma_pow2_adds(64, post as nat);
    lemma_pow2_unfold(64);
    assert(pow2(64) == two64);
    assert(pow2(64 + post as nat) == n);
    lemma_pow2_unfold(post as nat + 1);
    lemma_pow10_facts(k as nat);
    lemma_pow2_not_multiple_of_5(post as nat);
    assert(p < di);
    let q = n / di;
    let r = n % di;
    lemma_fundamental_div_mod(n, di);
    let mi = q + if r != 0 { 1int } else { 0int };
    if r == 0 {
        assert(mi * di == n) by (nonlinear_arith) requires mi == q, n == di * q + r, r == 0;
    } else {
        assert(mi * di == n + (di - r)) by (nonlinear_arith) requires mi == q + 1, n == di * q + r;
    }
    assert(mi * di >= n);
    assert(mi * di - n < di);
    assert(two64 * (di - p) >= two64) by (nonlinear_arith) requires di - p >= 1, two64 > 0;
    assert(n <= di * (two64 - 1)) by (nonlinear_arith) requires n == p * two64, di <= two64 * (di - p);
    assert(mi < two64) by (nonlinear_arith)
        requires mi * di - n < di, n <= di * (two64 - 1), di > 0, mi * di >= n;
}

pub open spec fn recip_spec(k: int, m: int, post: int) -> bool {
    &&& 1 <= k <= 38
    &&& 0 <= m < 0x1_0000_0000_0000_0000_0000_0000_0000_0000int
    &&& 0 <= post < 128
    &&& pow2(post as nat) <= pow(10, k as nat)
    &&& pow(10, k as nat) < pow2(post as nat + 1)
    &&& m * pow(10, k as nat) >= pow2(128 + post as nat)
    &&& m * pow(10, k as nat) - pow2(128 + post as nat) < pow(10, k as nat)
}

pub proof fn lemma_pow2_128()
    ensures
        pow2(0) == 1,
        pow2(63) == 0x8000_0000_0000_0000int,
        pow2(64) == 0x1_0000_0000_0000_0000int,
        pow2(127) == 0x8000_0000_0000_0000_0000_0000_0000_0000int,
        pow2(128) == 0x1_0000_0000_0000_0000_0000_0000_0000_0000int,
{
    lemma2_to64();
    lemma2_to64_rest();
    lemma_pow2_adds(63, 64);
    lemma_pow2_adds(64, 64);
}

pub proof fn lemma_pow10_facts_128(k: nat)
    requires
        1 <= k <= 38,
    ensures
        10 <= pow(10, k),
        pow(10, k) < 0x8000_0000_0000_0000_0000_0000_0000_0000int,
        pow(10, k) % 5 == 0,
{
    reveal_with_fuel(pow, 39);
    assert(pow(10, 38) == 100_000_000_000_000_000_000_000_000_000_000_000_000int);
    lemma_pow_increases(10, k, 38);
    lemma_pow_increases(10, 1, k);
    lemma_pow10_step((k - 1) as nat);
    assert((10 * pow(10, (k - 1) as nat)) % 5 == 0) by {
        lemma_mul_is_associative(5, 2, pow(10, (k - 1) as nat));
        lemma_mod_multiples_basic(2 * pow(10, (k - 1) as nat), 5);
    }
}

pub proof fn lemma_long_divide_step(d: int, steps: nat, q: int, r: int)
    requires
        1 < d < 0x8000_0000_0000_0000_0000_0000_0000_0000int,
        0 <= r < d,
        0 <= q,
        steps >= 1,
        (q * d + r) * pow2(steps) < d * pow2(128),
    ensures
        2 * r < 0x1_0000_0000_0000_0000_0000_0000_0000_0000int,
        2 * r >= d ==> 2 * q + 1 < 0x1_0000_0000_0000_0000_0000_0000_0000_0000int,
        2 * r >= d ==> ((2 * q + 1) * d + (2 * r - d)) * pow2((steps - 1) as nat) == (q * d + r) * pow2(steps),
        2 * r < d ==> 2 * q < 0x1_0000_0000_0000_0000_0000_0000_0000_0000int,
        2 * r < d ==> ((2 * q) * d + 2 * r) * pow2((steps - 1) as nat) == (q * d + r) * pow2(steps),
{
    lemma_pow2_128();
    let w = pow2(128) as int;
    let t = pow2((steps - 1) as nat) as int;
    lemma_pow2_unfold(steps);
    lemma_pow2_pos((steps - 1) as nat);
    assert(pow2(steps) == 2 * t);
    let n = q * d + r;
    assert(n * (2 * t) == (2 * n) * t) by (nonlinear_arith);
    if 2 * r >= d {
        let q2 = 2 * q + 1;
        let r2 = 2 * r - d;
        assert(q2 * d + r2 == 2 * n) by (nonlinear_arith) requires q2 == 2 * q + 1, r2 == 2 * r - d, n == q * d + r;
        assert(q2 * d <= (2 * n) * t) by (nonlinear_arith) requires q2 * d + r2 == 2 * n, r2 >= 0, t >= 1, q2 >= 0, d > 0;
        assert(q2 < w) by (nonlinear_arith) requires q2 * d <= (2 * n) * t, (2 * n) * t < d * w, d > 0;
    } else {
        let q2 = 2 * q;
        assert(q2 * d + 2 * r == 2 * n) by (nonlinear_arith) requires q2 == 2 * q, n == q * d + r;
        assert(q2 * d <= (2 * n) * t) by (nonlinear_arith) requires q2 * d + 2 * r == 2 * n, r >= 0, t >= 1, q2 >= 0, d > 0;
        assert(q2 < w) by (nonlinear_arith) requires q2 * d <= (2 * n) * t, (2 * n) * t < d * w, d > 0;
    }
}

pub proof fn lemma_reciprocal_start(k: nat, d: int, width: nat)
    requires
        1 <= k <= 38,
        d == pow(10, k),
        width >= 1,
        pow2((width - 1) as nat) <= d,
        d < pow2(width),
    ensures
        1 < d < 0x8000_0000_0000_0000_0000_0000_0000_0000int,
        width <= 127,
        pow2((width - 1) as nat) < d,
        pow2(127 + width) < d * pow2(128),
{
    lemma_pow2_128();
    lemma_pow10_facts_128(k);
    if width > 127 {
        lemma_pow2_strictly_increases(127, (width - 1) as nat);
    }
    lemma_pow2_not_multiple_of_5((width - 1) as nat);
    lemma_pow2_adds(128, (width - 1) as nat);
    assert(128 + (width - 1) as nat == 127 + width);
    lemma_pow2_pos(128);
    let p = pow2((width - 1) as nat) as int;
    let w = pow2(128) as int;
    assert(pow2(127 + width) == w * p);
    assert(w * p < d * w) by (nonlinear_arith) requires p < d, w > 0;
}

pub proof fn lemma_ceil_fits(n: int, d: int, p: int, w: int, q: int, r: int)
    requires
        n == p * w,
        1 <= p < d <= w,
        q * d + r == n,
        0 <= r < d,
        0 <= q,
    ensures
        ({
            let m = q + if r != 0 { 1int } else { 0int };
            &&& m * d >= n
            &&& m * d - n < d
            &&& m <= w - 1
        }),
{
    let m = q + if r != 0 { 1int } else { 0int };
    if r == 0 {
        assert(m * d == n) by (nonlinear_arith) requires m == q, q * d + r == n, r == 0;
    } else {
        assert(m * d == n + (d - r)) by (nonlinear_arith) requires m == q + 1, q * d + r == n;
    }
    assert(w * (d - p) >= d) by (nonlinear_arith) requires d - p >= 1, w >= d, d >= 1;
    assert(n <= d * (w - 1)) by (nonlinear_arith) requires n == p * w, d <= w * (d - p);
    assert(m <= w - 1) by (nonlinear_arith) requires m * d - n < d, n <= d * (w - 1), d > 0, m * d >= n;
}

pub proof fn lemma_reciprocal_entry(k: nat, d: int, width: nat, q: int, r: int)
    requires
        1 <= k <= 38,
        d == pow(10, k),
        1 <= width <= 127,
        pow2((width - 1) as nat) < d,
        d < pow2(width),
        q * d + r == pow2(127 + width),
        0 <= r < d,
        0 <= q,
    ensures
        r != 0,
        q < 0xffff_ffff_ffff_ffff_ffff_ffff_ffff_ffffint,
        recip_spec(k as int, q + 1, width - 1),
{
    lemma_pow2_128();
    lemma_pow10_facts_128(k);
    let post = (width - 1) as nat;
    let p = pow2(post) as int;
    let w = pow2(128) as int;
    lemma_pow2_adds(128, post);
    assert(128 + post == 127 + width);
    assert(pow2(127 + width) == w * p);
    lemma_mul_is_commutative(w, p);
    if width == 127 {
    } else {
        lemma_pow2_strictly_increases(width, 128);
    }
    assert(d <= w);
    lemma_pow2_pos(post);
    lemma_ceil_fits(p * w, d, p, w, q, r);
    if r == 0 {
        lemma_pow2_not_multiple_of_5(127 + width);
        assert(d == 5 * (d / 5)) by { lemma_fundamental_div_mod(d, 5); }
        assert(q * d == 5 * (q * (d / 5))) by (nonlinear_arith) requires d == 5 * (d / 5);
        lemma_mod_multiples_basic(q * (d / 5), 5);
        lemma_mul_is_commutative(5, q * (d / 5));
        assert(false);
    }
    assert(width - 1 + 1 == width);
}

pub proof fn lemma_long_divide_halves(steps: nat, half: nat, n: int)
    requires
        steps >= 2,
        half == steps / 2,
        n >= 0,
    ensures
        1 <= half < steps,
        steps - half < steps,
        n * pow2(half) <= n * pow2(steps),
        n * pow2(half) * pow2((steps - half) as nat) == n * pow2(steps),
{
    lemma_pow2_adds(half, (steps - half) as nat);
    lemma_pow2_pos((steps - half) as nat);
    lemma_pow2_pos(half);
    let a = pow2(half) as int;
    let b = pow2((steps - half) as nat) as int;
    assert(n * a * b == n * (a * b)) by (nonlinear_arith);
    assert(n * a <= n * (a * b)) by (nonlinear_arith) requires n >= 0, a >= 1, b >= 1;
}

pub proof fn lemma_u128_shl_one(s: u32)
    requires
        s < 128,
    ensures
        (1u128 << s) as int == pow2(s as nat),
    decreases s,
{
    if s == 0 {
        assert(1u128 << 0u32 == 1u128) by (bit_vector);
        lemma2_to64();
    } else {
        lemma_u128_shl_one((s - 1) as u32);
        let t = (s - 1) as u32;
        assert((1u128 << s) == (1u128 << t) + (1u128 << t)) by (bit_vector)
            requires 1 <= s < 128, t == s - 1;
        lemma_pow2_unfold(s as nat);
        lemma_pow2_strictly_increases(t as nat, 127);
        lemma_pow2_128();
    }
}

pub proof fn lemma_bit_width_is_post(x: int, width: nat, post: nat)
    requires
        x != 0 ==> width >= 1 && pow2((width - 1) as nat) <= x && x < pow2(width),
        pow2(post) <= x,
        x < pow2(post + 1),
    ensures
        width >= 1,
        width - 1 == post,
{
    lemma_pow2_pos(post);
    if width - 1 < post {
        if width < post {
            lemma_pow2_strictly_increases(width, post);
        }
    } else if width - 1 > post {
        if post + 1 < width - 1 {
            lemma_pow2_strictly_increases(post + 1, (width - 1) as nat);
        }
    }
}

pub proof fn lemma_u128_limbs(a: u128)
    ensures
        a & 0xffff_ffff_ffff_ffffu128 == a % 0x1_0000_0000_0000_0000u128,
        a >> 64u32 == a / 0x1_0000_0000_0000_0000u128,
        a >> 64i32 == a / 0x1_0000_0000_0000_0000u128,
        (a >> 64u32) < 0x1_0000_0000_0000_0000u128,
{
    assert(a & 0xffff_ffff_ffff_ffffu128 == a % 0x1_0000_0000_0000_0000u128) by (bit_vector);
    assert(a >> 64u32 == a / 0x1_0000_0000_0000_0000u128) by (bit_vector);
    assert(a >> 64i32 == a / 0x1_0000_0000_0000_0000u128) by (bit_vector);
    assert((a >> 64u32) < 0x1_0000_0000_0000_0000u128) by (bit_vector);
}

pub proof fn lemma_u128_join(x: u128, y: u128)
    ensures
        ((x & 0xffff_ffff_ffff_ffffu128) | ((y & 0xffff_ffff_ffff_ffffu128) << 64i32)) as int
            == (x % 0x1_0000_0000_0000_0000u128) as int + (y % 0x1_0000_0000_0000_0000u128) as int * 0x1_0000_0000_0000_0000int,
{
    let w = 0x1_0000_0000_0000_0000u128;
    assert(((x & 0xffff_ffff_ffff_ffffu128) | ((y & 0xffff_ffff_ffff_ffffu128) << 64i32))
        == (x % 0x1_0000_0000_0000_0000u128) + (y % 0x1_0000_0000_0000_0000u128) * 0x1_0000_0000_0000_0000u128) by (bit_vector);
    assert((y % w) as int * (w as int) < 0x1_0000_0000_0000_0000_0000_0000_0000_0000int) by (nonlinear_arith)
        requires (y % w) < w, w == 0x1_0000_0000_0000_0000u128;
}

pub proof fn lemma_mul_below(x: int, y: int, w: int)
    requires
        0 <= x < w,
        0 <= y < w,
    ensures
        0 <= x * y < w * w,
{
    lemma_mul_nonnegative(x, y);
    lemma_mul_inequality(y, w, x);
    lemma_mul_strict_inequality(x, w, w);
    lemma_mul_is_commutative(x, y);
    lemma_mul_is_commutative(x, w);
}

pub proof fn lemma_mul_limbs(a: int, b: int)
    requires
        0 <= a < 0x1_0000_0000_0000_0000_0000_0000_0000_0000int,
        0 <= b < 0x1_0000_0000_0000_0000_0000_0000_0000_0000int,
    ensures
        ({
            let w = 0x1_0000_0000_0000_0000int;
            let al = a % w;
            let ah = a / w;
            let bl = b % w;
            let bh = b / w;
            let ll = al * bl;
            let hl = ah * bl;
            let lh = al * bh;
            let hh = ah * bh;
            let mid = ll / w + hl % w + lh % w;
            &&& 0 <= al < w && 0 <= ah < w && 0 <= bl < w && 0 <= bh < w
            &&& 0 <= ll < w * w && 0 <= hl < w * w && 0 <= lh < w * w && 0 <= hh < w * w
            &&& 0 <= mid < w * w
            &&& 0 <= hh + hl / w + lh / w + mid / w < w * w
            &&& (ll % w + (mid % w) * w) + (hh + hl / w + lh / w + mid / w) * (w * w) == a * b
            &&& pow2(128) == w * w
        }),
{
    lemma_pow2_128();
    let w = 0x1_0000_0000_0000_0000int;
    let al = a % w;
    let ah = a / w;
    let bl = b % w;
    let bh = b / w;
    lemma_fundamental_div_mod(a, w);
    lemma_fundamental_div_mod(b, w);
    lemma_div_pos_is_pos(a, w);
    lemma_div_pos_is_pos(b, w);
    lemma_multiply_divide_lt(a, w, w);
    lemma_multiply_divide_lt(b, w, w);
    let ll = al * bl;
    let hl = ah * bl;
    let lh = al * bh;
    let hh = ah * bh;
    lemma_mul_below(al, bl, w);
    lemma_mul_below(ah, bl, w);
    lemma_mul_below(al, bh, w);
    lemma_mul_below(ah, bh, w);
    lemma_fundamental_div_mod(ll, w);
    lemma_fundamental_div_mod(hl, w);
    lemma_fundamental_div_mod(lh, w);
    lemma_div_pos_is_pos(ll, w);
    lemma_div_pos_is_pos(hl, w);
    lemma_div_pos_is_pos(lh, w);
    lemma_multiply_divide_lt(ll, w, w);
    let mid = ll / w + hl % w + lh % w;
    assert(0 <= mid < 3 * w);
    lemma_fundamental_div_mod(mid, w);
    lemma_div_pos_is_pos(mid, w);
    let high = hh + hl / w + lh / w + mid / w;
    let low = ll % w + (mid % w) * w;
    let w2 = w * w;
    assert(ah * b == w * hh + hl) by (nonlinear_arith) requires b == w * bh + bl, hh == ah * bh, hl == ah * bl;
    assert(al * b == w * lh + ll) by (nonlinear_arith) requires b == w * bh + bl, lh == al * bh, ll == al * bl;
    assert(a * b == w * (ah * b) + al * b) by (nonlinear_arith) requires a == w * ah + al;
    let hlh = hl / w;
    let hll = hl % w;
    let lhh = lh / w;
    let lhl = lh % w;
    let llh = ll / w;
    let lll = ll % w;
    let midh = mid / w;
    let midl = mid % w;
    assert(w * (w * hh + hl) == w2 * hh + w2 * hlh + w * hll) by (nonlinear_arith)
        requires hl == w * hlh + hll, w2 == w * w;
    assert(w * lh + ll == w2 * lhh + w * lhl + w * llh + lll) by (nonlinear_arith)
        requires lh == w * lhh + lhl, ll == w * llh + lll, w2 == w * w;
    assert(a * b == w2 * (hh + hlh + lhh) + w * mid + lll) by (nonlinear_arith)
        requires
            a * b == w * (ah * b) + al * b,
            ah * b == w * hh + hl,
            al * b == w * lh + ll,
            w * (w * hh + hl) == w2 * hh + w2 * hlh + w * hll,
            w * lh + ll == w2 * lhh + w * lhl + w * llh + lll,
            mid == llh + hll + lhl;
    assert(w * mid == w2 * midh + w * midl) by (nonlinear_arith) requires mid == w * midh + midl, w2 == w * w;
    assert(low + high * w2 == a * b) by (nonlinear_arith)
        requires
            a * b == w2 * (hh + hlh + lhh) + w * mid + lll,
            w * mid == w2 * midh + w * midl,
            high == hh + hlh + lhh + midh,
            low == lll + midl * w;
    assert(low >= 0) by (nonlinear_arith) requires ll % w >= 0, mid % w >= 0, w > 0, low == ll % w + (mid % w) * w;
    lemma_mul_below(a, b, w2);
    assert(high < w2) by (nonlinear_arith)
        requires low + high * w2 == a * b, low >= 0, a * b < w2 * w2, w2 > 0;
    assert(high >= 0);
}

pub proof fn lemma_verify_reciprocal(k: nat, m: int, post: nat, low: int, high: int)
    requires
        recip_spec(k as int, m, post as int),
        0 <= low < pow2(128),
        0 <= high,
        low + high * pow2(128) == m * pow(10, k),
    ensures
        high == pow2(post),
        low < pow(10, k),
{
    lemma_pow2_128();
    lemma_pow10_facts_128(k);
    let w2 = pow2(128) as int;
    let d = pow(10, k);
    let e = m * d - pow2(128 + post);
    lemma_pow2_adds(128, post);
    let p = pow2(post) as int;
    assert(m * d == p * w2 + e);
    assert(low + high * w2 == e + p * w2);
    lemma_fundamental_div_mod_converse(m * d, w2, high, low);
    lemma_fundamental_div_mod_converse(m * d, w2, p, e);
}

pub proof fn lemma_verify_reciprocal_64(k: nat, m: int, post: nat)
    requires
        recip64_spec(k as int, m, post as int),
    ensures
        64 + post < 128,
        m * pow(10, k) < 0x1_0000_0000_0000_0000_0000_0000_0000_0000int,
{
    lemma_pow2_128();
    lemma_pow10_facts(k);
    lemma_mul_below(m, pow(10, k), 0x1_0000_0000_0000_0000int);
}

pub proof fn lemma_pow2_above_coefficients(a: nat)
    requires
        pow2(a) < 10_000_000_000_000_000_000_000_000_000_000_000int,
    ensures
        a < 113,
{
    if a >= 113 {
        lemma2_to64();
        lemma2_to64_rest();
        lemma_pow2_adds(64, 49);
        lemma_pow2_strictly_increases(113, a);
        if a == 113 {
        }
    }
}

pub proof fn lemma_pow2_below_pow5(f: nat)
    ensures
        pow2(f) <= pow(5, f),
    decreases f,
{
    if f == 0 {
        lemma2_to64();
        lemma_pow0(5);
    } else {
        lemma_pow2_below_pow5((f - 1) as nat);
        lemma_pow2_unfold(f);
        reveal(pow);
        assert(pow(5, f) == 5 * pow(5, (f - 1) as nat));
        lemma_pow2_pos((f - 1) as nat);
    }
}

pub open spec fn odd_shift(cs: u128, a: u32, odd: u128) -> bool {
    &&& a < 128
    &&& odd == cs >> a
    &&& (cs as int) % (pow2(a as nat) as int) == 0
    &&& ((cs as int) / (pow2(a as nat) as int)) % 2 == 1
}

pub proof fn lemma_odd_part_start(cs: u128, a: u32, odd: u128)
    requires
        0 < cs,
        odd_shift(cs, a, odd),
    ensures
        0 < odd,
        odd % 2 == 1,
        cs as int == pow2(a as nat) * pow(5, 0) * odd as int,
{
    vstd::bits::lemma_u128_shr_is_div(cs, a as u128);
    lemma_pow2_pos(a as nat);
    lemma_pow0(5);
    lemma_fundamental_div_mod(cs as int, pow2(a as nat) as int);
}

pub proof fn lemma_odd_part_step(cs: int, a: nat, f: nat, odd: int)
    requires
        0 < odd,
        odd % 2 == 1,
        odd % 5 == 0,
        cs == pow2(a) * pow(5, f) * odd,
    ensures
        0 < odd / 5 < odd,
        (odd / 5) % 2 == 1,
        cs == pow2(a) * pow(5, f + 1) * (odd / 5),
{
    lemma_fundamental_div_mod(odd, 5);
    let q = odd / 5;
    assert(odd == 5 * q);
    assert(q % 2 == 1) by {
        lemma_fundamental_div_mod(q, 2);
        let h = q / 2;
        let b = q % 2;
        assert(odd == 2 * (5 * h + 2 * b) + b) by (nonlinear_arith)
            requires odd == 5 * q, q == 2 * h + b;
        lemma_mod_bound(q, 2);
        lemma_mod_multiples_vanish(5 * h + 2 * b, b, 2);
        lemma_small_mod(b as nat, 2);
    }
    reveal(pow);
    assert(pow(5, f + 1) == 5 * pow(5, f));
    assert(pow2(a) * pow(5, f) * odd == pow2(a) * (5 * pow(5, f)) * q) by (nonlinear_arith)
        requires odd == 5 * q;
}

pub proof fn lemma_cancel_ten(o: int, m: int)
    requires
        0 < o,
        0 <= m,
        o % 2 == 1,
        o % 5 != 0,
        (m * 10) % o == 0,
    ensures
        m % o == 0,
{
    lemma_fundamental_div_mod(m * 10, o);
    let k = (m * 10) / o;
    assert(m * 10 == o * k);
    lemma_mul_mod_noop(o, k, 2);
    lemma_mod_multiples_basic(m * 5, 2);
    assert((m * 10) % 2 == 0) by {
        assert(m * 10 == (m * 5) * 2) by (nonlinear_arith);
    }
    assert(k % 2 == 0) by {
        lemma_mod_bound(k, 2);
        if k % 2 == 1 {
            assert((1 * 1) % 2 == 1);
        }
    }
    lemma_fundamental_div_mod(k, 2);
    let k1 = k / 2;
    assert(m * 5 == o * k1) by (nonlinear_arith)
        requires m * 10 == o * k, k == 2 * k1;
    lemma_mul_mod_noop(o, k1, 5);
    lemma_mod_multiples_basic(m, 5);
    assert((m * 5) % 5 == 0);
    assert(k1 % 5 == 0) by {
        lemma_mod_bound(o, 5);
        lemma_mod_bound(k1, 5);
        let a = o % 5;
        let b = k1 % 5;
        assert((a * b) % 5 == 0);
        assert(b == 0) by {
            assert(1 <= a < 5 && 0 <= b < 5);
            assert(a == 1 || a == 2 || a == 3 || a == 4);
            assert(b == 0 || b == 1 || b == 2 || b == 3 || b == 4);
            if b != 0 {
                if a == 1 { assert((1 * b) % 5 != 0); }
                else if a == 2 { assert((2 * b) % 5 != 0); }
                else if a == 3 { assert((3 * b) % 5 != 0); }
                else { assert((4 * b) % 5 != 0); }
            }
        }
    }
    lemma_fundamental_div_mod(k1, 5);
    let k2 = k1 / 5;
    assert(m == o * k2) by (nonlinear_arith)
        requires m * 5 == o * k1, k1 == 5 * k2;
    lemma_mod_multiples_basic(k2, o);
    assert(o * k2 == k2 * o) by (nonlinear_arith);
}

pub proof fn lemma_cancel_pow10(o: int, n: int, d: nat)
    requires
        0 < o,
        0 <= n,
        o % 2 == 1,
        o % 5 != 0,
        (n * pow(10, d)) % o == 0,
    ensures
        n % o == 0,
    decreases d,
{
    if d == 0 {
        lemma_pow0(10);
    } else {
        reveal(pow);
        let p = pow(10, (d - 1) as nat);
        lemma_pow_positive(10, (d - 1) as nat);
        assert(n * pow(10, d) == (n * p) * 10) by (nonlinear_arith)
            requires pow(10, d) == 10 * p;
        lemma_mul_nonnegative(n, p);
        lemma_cancel_ten(o, n * p);
        lemma_cancel_pow10(o, n, (d - 1) as nat);
    }
}

pub proof fn lemma_scaled_is_multiple(cx: int, cs: int, a: nat, f: nat, o: int, d: nat)
    requires
        0 <= cx,
        0 < o,
        o % 2 == 1,
        o % 5 != 0,
        cs == pow2(a) * pow(5, f) * o,
        a <= d,
        f <= d,
    ensures
        (cx * pow(10, d)) % cs == 0 <==> cx % o == 0,
{
    lemma_pow2_pos(a);
    lemma_pow_positive(5, f);
    lemma_pow_positive(10, d);
    lemma_pow2(d);
    lemma_pow2(a);
    lemma_pow_distributes(2, 5, d);
    lemma_pow_sub_add_cancel(2, d, a);
    lemma_pow_sub_add_cancel(5, d, f);
    lemma_pow_adds(2, a, (d - a) as nat);
    lemma_pow_adds(5, f, (d - f) as nat);
    let u = pow(2, (d - a) as nat) * pow(5, (d - f) as nat);
    assert(pow(10, d) == pow2(a) * pow(5, f) * u) by (nonlinear_arith)
        requires
            pow(10, d) == pow(2, d) * pow(5, d),
            pow(2, d) == pow(2, a) * pow(2, (d - a) as nat),
            pow(5, d) == pow(5, f) * pow(5, (d - f) as nat),
            pow2(a) == pow(2, a),
            u == pow(2, (d - a) as nat) * pow(5, (d - f) as nat);
    lemma_mul_strictly_positive(pow2(a) as int, pow(5, f));
    let s = pow2(a) * pow(5, f);
    lemma_mul_strictly_positive(s, o);
    if cx % o == 0 {
        lemma_fundamental_div_mod(cx, o);
        let j = cx / o;
        assert(cx * pow(10, d) == (j * u) * cs) by (nonlinear_arith)
            requires cx == o * j, pow(10, d) == s * u, cs == s * o;
        lemma_mod_multiples_basic(j * u, cs);
    }
    if (cx * pow(10, d)) % cs == 0 {
        lemma_fundamental_div_mod(cx * pow(10, d), cs);
        let k = (cx * pow(10, d)) / cs;
        assert(cx * pow(10, d) == (k * s) * o) by (nonlinear_arith)
            requires cx * pow(10, d) == cs * k, cs == s * o;
        lemma_mod_multiples_basic(k * s, o);
        lemma_cancel_pow10(o, cx, d);
    }
}

pub proof fn lemma_scaled_is_multiple_bounds(cs: int, a: nat, f: nat, o: int)
    requires
        0 < o,
        0 < cs < 10_000_000_000_000_000_000_000_000_000_000_000int,
        cs == pow2(a) * pow(5, f) * o,
    ensures
        a < 113,
        f < 113,
{
    lemma_pow2_pos(a);
    lemma_pow_positive(5, f);
    lemma_pow2_below_pow5(f);
    lemma_pow2_pos(f);
    assert(pow2(a) <= cs) by (nonlinear_arith)
        requires cs == pow2(a) * pow(5, f) * o, 0 < pow(5, f), 0 < o, 0 < pow2(a);
    assert(pow2(f) <= cs) by (nonlinear_arith)
        requires cs == pow2(a) * pow(5, f) * o, pow2(f) <= pow(5, f), 0 < pow2(f), 0 < o, 0 < pow2(a);
    lemma_pow2_above_coefficients(a);
    lemma_pow2_above_coefficients(f);
}

pub(crate) open spec fn same_order(x: int, y: int, exact_x: int, exact_y: int) -> bool {
    (x < y <==> exact_x < exact_y) && (x == y <==> exact_x == exact_y)
}

pub(crate) open spec fn comparison_value(a: crate::layout::Unpacked, b: crate::layout::Unpacked) -> int {
    signed_at(a.neg, a.coeff as int, a.exp as int, min_exp(a, b))
}

pub(crate) open spec fn comparison_result(r: core::cmp::Ordering, a: crate::layout::Unpacked, b: crate::layout::Unpacked) -> bool {
    let x = comparison_value(a, b);
    let y = comparison_value(b, a);
    match r {
        core::cmp::Ordering::Less => x < y,
        core::cmp::Ordering::Equal => x == y,
        core::cmp::Ordering::Greater => x > y,
    }
}

pub(crate) proof fn lemma_comparison_scale_64(c: int, k: nat)
    requires 0 <= c < 10000000000000000int, k <= 16,
    ensures
        1 <= pow(10, k) <= 10000000000000000int,
        0 <= c * pow(10, k) < 100000000000000000000000000000000int,
        c * pow(10, k) == 0 <==> c == 0,
{
    lemma_pow_positive(10, k);
    lemma_pow_increases(10, k, 16);
    assert(c * pow(10, k) == 0 <==> c == 0) by (nonlinear_arith)
        requires 0 <= c, pow(10, k) > 0;
    assert(pow(10, 16) == 10000000000000000int) by (compute_only);
    assert(0 <= c * pow(10, k) < 100000000000000000000000000000000int) by (nonlinear_arith)
        requires 0 <= c < 10000000000000000int, 1 <= pow(10, k) <= 10000000000000000int;
}

pub(crate) proof fn lemma_comparison_dominance_64(c: int, other: int, k: nat)
    requires 0 <= c < 10000000000000000int, 0 <= other < 10000000000000000int, k >= 16,
    ensures
        same_order(c * pow(10, 16), other, c * pow(10, k), other),
        c * pow(10, 16) == 0 <==> c == 0,
        c * pow(10, k) == 0 <==> c == 0,
{
    lemma_pow_increases(10, 16, k);
    assert(pow(10, 16) == 10000000000000000int) by (compute_only);
    if c > 0 {
        assert(c * pow(10, k) >= 10000000000000000int) by (nonlinear_arith)
            requires c > 0, pow(10, k) >= 10000000000000000int;
    }
}

pub(crate) proof fn lemma_comparison_clamp_64(a: crate::layout::Unpacked, b: crate::layout::Unpacked, g: int)
    requires
        a.coeff < 10000000000000000int, b.coeff < 10000000000000000int,
        g == if a.exp - b.exp < -16 { -16 } else if a.exp - b.exp > 16 { 16 } else { a.exp - b.exp },
    ensures
        same_order(
            a.coeff * pow(10, (if g > 0 { g } else { 0 }) as nat),
            b.coeff * pow(10, (if g < 0 { -g } else { 0 }) as nat),
            a.coeff * pow(10, (a.exp - min_exp(a, b)) as nat),
            b.coeff * pow(10, (b.exp - min_exp(a, b)) as nat)),
{
    lemma_pow0(10);
    let d = a.exp - b.exp;
    if d >= 16 {
        lemma_comparison_dominance_64(a.coeff as int, b.coeff as int, d as nat);
    } else if d <= -16 {
        lemma_comparison_dominance_64(b.coeff as int, a.coeff as int, (-d) as nat);
    }
}

pub(crate) proof fn lemma_comparison_signs(a: crate::layout::Unpacked, b: crate::layout::Unpacked, x: int, y: int)
    requires
        0 <= x, 0 <= y,
        same_order(x, y, a.coeff * pow(10, (a.exp - min_exp(a, b)) as nat), b.coeff * pow(10, (b.exp - min_exp(a, b)) as nat)),
        x == 0 <==> a.coeff == 0,
        y == 0 <==> b.coeff == 0,
    ensures
        same_order(if a.neg { -x } else { x }, if b.neg { -y } else { y }, comparison_value(a, b), comparison_value(b, a)),
        ((x == y) && (a.neg == b.neg || x == 0)) <==> comparison_value(a, b) == comparison_value(b, a),
{
    let ex = a.coeff * pow(10, (a.exp - min_exp(a, b)) as nat);
    let ey = b.coeff * pow(10, (b.exp - min_exp(a, b)) as nat);
    lemma_pow_positive(10, (a.exp - min_exp(a, b)) as nat);
    lemma_pow_positive(10, (b.exp - min_exp(a, b)) as nat);
    assert(ex >= 0 && (ex == 0 <==> a.coeff == 0)) by (nonlinear_arith)
        requires ex == a.coeff * pow(10, (a.exp - min_exp(a, b)) as nat), a.coeff >= 0, pow(10, (a.exp - min_exp(a, b)) as nat) > 0;
    assert(ey >= 0 && (ey == 0 <==> b.coeff == 0)) by (nonlinear_arith)
        requires ey == b.coeff * pow(10, (b.exp - min_exp(a, b)) as nat), b.coeff >= 0, pow(10, (b.exp - min_exp(a, b)) as nat) > 0;
}

pub(crate) proof fn lemma_comparison_adjusted(a: crate::layout::Unpacked, b: crate::layout::Unpacked, da: nat, db: nat)
    requires
        1 <= da <= 34, 1 <= db <= 34,
        pow(10, (da - 1) as nat) <= a.coeff < pow(10, da),
        pow(10, (db - 1) as nat) <= b.coeff < pow(10, db),
    ensures
        a.exp + da < b.exp + db ==> a.coeff * pow(10, (a.exp - min_exp(a, b)) as nat) < b.coeff * pow(10, (b.exp - min_exp(a, b)) as nat),
        a.exp + da > b.exp + db ==> a.coeff * pow(10, (a.exp - min_exp(a, b)) as nat) > b.coeff * pow(10, (b.exp - min_exp(a, b)) as nat),
        a.exp + da == b.exp + db ==> -33 <= a.exp - b.exp <= 33,
{
    let ka = (a.exp - min_exp(a, b)) as nat;
    let kb = (b.exp - min_exp(a, b)) as nat;
    lemma_add_digit_scale(a.coeff as int, da, ka);
    lemma_add_digit_scale(b.coeff as int, db, kb);
    if a.exp + da < b.exp + db {
        lemma_pow_increases(10, da + ka, (db + kb - 1) as nat);
    } else if a.exp + da > b.exp + db {
        lemma_pow_increases(10, db + kb, (da + ka - 1) as nat);
    }
}

pub(crate) proof fn lemma_comparison_align_128(a: crate::layout::Unpacked, b: crate::layout::Unpacked, da: nat, db: nat)
    requires
        1 <= da <= 34, 1 <= db <= 34,
        pow(10, (da - 1) as nat) <= a.coeff < pow(10, da),
        pow(10, (db - 1) as nat) <= b.coeff < pow(10, db),
        a.exp + da == b.exp + db,
    ensures
        -33 <= a.exp - b.exp <= 33,
        0 < a.coeff * pow(10, (a.exp - min_exp(a, b)) as nat) < 10000000000000000000000000000000000int,
        0 < b.coeff * pow(10, (b.exp - min_exp(a, b)) as nat) < 10000000000000000000000000000000000int,
{
    let ka = (a.exp - min_exp(a, b)) as nat;
    let kb = (b.exp - min_exp(a, b)) as nat;
    lemma_add_digit_scale(a.coeff as int, da, ka);
    lemma_add_digit_scale(b.coeff as int, db, kb);
    assert(da + ka <= 34 && db + kb <= 34);
    lemma_pow_increases(10, da + ka, 34);
    lemma_pow_increases(10, db + kb, 34);
    assert(pow(10, 34) == 10000000000000000000000000000000000int) by (compute_only);
}

}
