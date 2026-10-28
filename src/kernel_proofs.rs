use core::cmp::Ordering;

use super::{
    cmp, cmp_general, digits, digits_64, div_rem_pow10, div_rem_pow10_64, eq,
    strip_trailing_zeros_64, zero_sign,
};
use crate::Rounding;
use crate::layout::{Unpacked, W32, W64, W128, Width};
use crate::tables::{pow10, pow10_64};

fn any_rounding() -> Rounding {
    if kani::any() {
        Rounding::TowardNegative
    } else {
        Rounding::TowardPositive
    }
}

fn any_unpacked<W: Width>() -> Unpacked {
    let neg: bool = kani::any();
    let coeff: u128 = kani::any();
    let exp: i32 = kani::any();
    kani::assume(coeff < pow10(W::LAYOUT.digits));
    kani::assume(exp >= W::LAYOUT.qmin && exp <= W::LAYOUT.qmax);
    Unpacked { neg, coeff, exp }
}

#[kani::proof]
#[kani::unwind(40)]
fn power_tables_are_powers_of_ten() {
    let mut expected = 1_u128;
    let mut k = 0;
    while k <= 38 {
        assert_eq!(pow10(k), expected);
        if k <= 19 {
            assert_eq!(u128::from(pow10_64(k)), expected);
        }
        if k < 38 {
            expected *= 10;
        }
        k += 1;
    }
}

#[kani::proof]
fn digits_is_the_decimal_length() {
    let x: u128 = kani::any();
    let n = digits(x);
    assert!(n <= 39);
    if x == 0 {
        assert_eq!(n, 0);
    } else {
        assert!(n >= 1);
        assert!(x >= pow10(n - 1));
        if n <= 38 {
            assert!(x < pow10(n));
        }
    }
}

#[kani::proof]
fn digits_64_is_the_decimal_length() {
    let x: u64 = kani::any();
    let n = digits_64(x);
    assert!(n <= 20);
    if x == 0 {
        assert_eq!(n, 0);
    } else {
        assert!(n >= 1);
        assert!(x >= pow10_64(n - 1));
        if n <= 19 {
            assert!(x < pow10_64(n));
        }
    }
}

#[kani::proof]
fn div_rem_pow10_outside_table() {
    let x: u128 = kani::any();
    let k: u32 = kani::any();
    kani::assume(k == 0 || k >= 39);
    let expected = if k == 0 { (x, 0) } else { (0, x) };
    assert_eq!(div_rem_pow10(x, k), expected);
}

#[kani::proof]
fn div_rem_pow10_64_outside_table() {
    let x: u64 = kani::any();
    let k: u32 = kani::any();
    kani::assume(k == 0 || k >= 20);
    let expected = if k == 0 { (x, 0) } else { (0, x) };
    assert_eq!(div_rem_pow10_64(x, k), expected);
}

macro_rules! division_proofs {
    ($($name:ident = $k:literal),* $(,)?) => {
        $(
            #[kani::proof]
            fn $name() {
                let x: u128 = kani::any();
                kani::assume(x < 1_u128 << 127);
                let (q, r) = div_rem_pow10(x, $k);
                let d = pow10($k);
                assert!(r < d);
                assert_eq!(q.checked_mul(d).and_then(|p| p.checked_add(r)), Some(x));
            }
        )*
    };
}

macro_rules! division_64_proofs {
    ($($name:ident = $k:literal),* $(,)?) => {
        $(
            #[kani::proof]
            fn $name() {
                let x: u64 = kani::any();
                kani::assume(x < 1_u64 << 63);
                let (q, r) = div_rem_pow10_64(x, $k);
                let d = pow10_64($k);
                assert!(r < d);
                assert_eq!(q.checked_mul(d).and_then(|p| p.checked_add(r)), Some(x));
            }
        )*
    };
}

division_proofs!(
    div_rem_pow10_k36 = 36,
    div_rem_pow10_k37 = 37,
    div_rem_pow10_k38 = 38,
);

division_64_proofs!(
    div_rem_pow10_64_k15 = 15,
    div_rem_pow10_64_k16 = 16,
    div_rem_pow10_64_k17 = 17,
    div_rem_pow10_64_k18 = 18,
    div_rem_pow10_64_k19 = 19,
);

macro_rules! strip_64_proofs {
    ($($name:ident = $zeros:literal),* $(,)?) => {
        $(
            #[kani::proof]
            #[kani::unwind(5)]
            fn $name() {
                let unit: u64 = kani::any();
                kani::assume(unit % 10 != 0 && unit < pow10_64(16 - $zeros));
                let x = unit * pow10_64($zeros);
                assert_eq!(strip_trailing_zeros_64(x), (unit, $zeros));
            }
        )*
    };
}

#[kani::proof]
fn strip_trailing_zeros_64_zero() {
    assert_eq!(strip_trailing_zeros_64(0), (0, 0));
}

strip_64_proofs!(strip_trailing_zeros_64_z15 = 15);

#[kani::proof]
fn zero_sign_follows_exact_zero_rule() {
    let a_neg: bool = kani::any();
    let b_neg: bool = kani::any();
    let dir = if kani::any() {
        None
    } else {
        Some(any_rounding())
    };
    let expected = if a_neg == b_neg {
        a_neg
    } else {
        matches!(dir, Some(Rounding::TowardNegative))
    };
    assert_eq!(zero_sign(a_neg, b_neg, dir), expected);
}

macro_rules! comparison_proofs {
    ($width:ty, $eq_name:ident, $cmp_name:ident) => {
        #[kani::proof]
        #[kani::unwind(3)]
        fn $eq_name() {
            let a = any_unpacked::<$width>();
            let b = any_unpacked::<$width>();
            let limit = (38 - <$width as Width>::LAYOUT.digits) as i32;
            kani::assume((a.exp - b.exp).abs() <= limit);
            assert_eq!(eq::<$width>(a, b), cmp_general(a, b) == Ordering::Equal);
        }

        #[kani::proof]
        #[kani::unwind(3)]
        fn $cmp_name() {
            let a = any_unpacked::<$width>();
            let b = any_unpacked::<$width>();
            let limit = (38 - <$width as Width>::LAYOUT.digits) as i32;
            kani::assume((a.exp - b.exp).abs() <= limit);
            assert_eq!(cmp::<$width>(a, b), cmp_general(a, b));
        }
    };
}

comparison_proofs!(
    W32,
    eq_bounded_32_matches_general,
    cmp_bounded_32_matches_general
);

macro_rules! comparison_gap_proofs {
    ($width:ty, $($name:ident = $gap:literal),* $(,)?) => {
        $(
            #[kani::proof]
            #[kani::unwind(3)]
            fn $name() {
                let a = any_unpacked::<$width>();
                let b = any_unpacked::<$width>();
                kani::assume(a.exp - b.exp == $gap);
                let general = cmp_general(a, b);
                assert_eq!(cmp::<$width>(a, b), general);
                assert_eq!(eq::<$width>(a, b), general == Ordering::Equal);
            }
        )*
    };
}

comparison_gap_proofs!(
    W64,
    bounded_64_gap_m22_matches_general = -22,
    bounded_64_gap_p00_matches_general = 0,
    bounded_64_gap_p22_matches_general = 22,
);

comparison_gap_proofs!(W128, bounded_128_gap_p00_matches_general = 0);
