//! Ordinary regression tests and test-only fault injection for binary input.
//! Synthetic endpoint failures are never natural reachability evidence.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "Test fixture arithmetic is independently bounded and assertions report failures"
)]
use super::*;
use core::cell::Cell;
use std::vec::Vec;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Fault {
    #[default]
    None,
    DigitBoundary,
    RoundBoundary,
    Drop10Boundary,
    Shift128Boundary,
    NonpositiveShift,
}
#[derive(Clone, Copy, Debug, Default)]
struct State {
    fault: Fault,
    entered: u64,
    fallback: u64,
    shift128: u64,
    zero: u64,
    drop10: u64,
}
std::thread_local! { static STATE: Cell<State> = const { Cell::new(State { fault: Fault::None, entered: 0, fallback: 0, shift128: 0, zero: 0, drop10: 0 }) }; }

pub(super) fn observe_interval(
    low: u128,
    high: u128,
    shift: i32,
    precision: u32,
) -> (u128, u128, i32) {
    STATE.with(|cell| {
        let mut state = cell.get();
        state.entered += 1;
        state.shift128 += u64::from(shift == 128);
        state.zero += u64::from(shift > 128);
        state.drop10 +=
            u64::from(shift > 0 && shift < 128 && (low >> shift) >= 10_u128.pow(precision));
        cell.set(state);
        match state.fault {
            Fault::None => (low, high, shift),
            Fault::DigitBoundary => {
                let boundary = 10_u128.pow(precision) << 16;
                (boundary - 1, boundary + 1, 16)
            }
            Fault::RoundBoundary => {
                let boundary = (2 << 16) + (1 << 15);
                (boundary, boundary + 2, 16)
            }
            Fault::Drop10Boundary => {
                let boundary = (10_u128.pow(precision) + 5) << 16;
                (boundary, boundary + 2, 16)
            }
            Fault::Shift128Boundary => (1 << 127, (1 << 127) + 2, 128),
            Fault::NonpositiveShift => (low, high, 0),
        }
    })
}
pub(super) fn observe_exact_fallback() {
    STATE.with(|cell| {
        let mut s = cell.get();
        s.fallback += 1;
        cell.set(s);
    });
}
fn reset(fault: Fault) {
    STATE.with(|cell| {
        cell.set(State {
            fault,
            ..State::default()
        })
    });
}
fn state() -> State {
    STATE.with(Cell::get)
}
struct ResetOnDrop;
impl Drop for ResetOnDrop {
    fn drop(&mut self) {
        reset(Fault::None);
    }
}
fn inject(fault: Fault) -> ResetOnDrop {
    reset(fault);
    ResetOnDrop
}

fn convert(single: bool, width: u32, bits: u64) -> u128 {
    macro_rules! convert {
        ($ty:ty) => {
            if single {
                <$ty>::from_f32(f32::from_bits(bits as u32)).to_bits() as u128
            } else {
                <$ty>::from_f64(f64::from_bits(bits)).to_bits() as u128
            }
        };
    }
    match width {
        32 => convert!(crate::Decimal32),
        64 => convert!(crate::Decimal64),
        128 => convert!(crate::Decimal128),
        _ => unreachable!(),
    }
}

#[test]
fn synthetic_endpoint_rounding_edges() {
    // shift=128 ties to even zero; a point above the midpoint rounds to one.
    assert_eq!(interval_endpoint((1 << 127) - 1, 128, false), 0);
    assert_eq!(interval_endpoint(1 << 127, 128, false), 0);
    assert_eq!(interval_endpoint((1 << 127) + 1, 128, false), 1);
    for shift in [1_u32, 2, 16, 64, 100, 120] {
        let unit = 1_u128 << shift;
        let half = unit >> 1;
        assert_eq!(interval_endpoint(2 * unit + half - 1, shift, false), 2);
        assert_eq!(interval_endpoint(2 * unit + half, shift, false), 2);
        assert_eq!(interval_endpoint(2 * unit + half + 1, shift, false), 3);
        assert_eq!(interval_endpoint(3 * unit + half, shift, false), 4);
        assert_eq!(interval_endpoint(25 * unit, shift, true), 2);
        assert_eq!(interval_endpoint(25 * unit + 1, shift, true), 3);
        assert_eq!(interval_endpoint(35 * unit, shift, true), 4);
        assert_eq!(interval_endpoint(99 * unit + half, shift, true), 10);
    }
}

#[test]
fn injected_actual_dispatch_falls_back_to_exact_fraction_fixtures() {
    let mut checks = 0;
    for line in include_str!("interval_fixtures.tsv")
        .lines()
        .filter(|x| !x.starts_with('#'))
    {
        let f: Vec<_> = line.split('\t').collect();
        assert_eq!(f.len(), 6, "fixture must have six fields: {line}");
        assert!(matches!(f[0], "0" | "1"), "invalid source kind: {line}");
        let single = f[0] == "1";
        let width = f[1].parse().unwrap();
        let bits = u64::from_str_radix(f[2], 16).unwrap();
        assert!(
            !single || u32::try_from(bits).is_ok(),
            "f32 bits out of range: {line}"
        );
        let expected = u128::from_str_radix(f[3], 16).unwrap();
        let baseline = u128::from_str_radix(f[4], 16).unwrap();
        assert_eq!(
            expected, baseline,
            "independent Fraction encoding disagrees with baseline: {line}"
        );
        reset(Fault::None);
        assert_eq!(
            convert(single, width, bits),
            expected,
            "natural conversion: {line}"
        );
        assert_eq!(
            state().fallback,
            0,
            "fixture unexpectedly reaches natural fallback: {line}"
        );
        assert_eq!(
            state().entered,
            1,
            "fixture must reach actual slow dispatch: {line}"
        );
        for fault in [
            Fault::DigitBoundary,
            Fault::RoundBoundary,
            Fault::Drop10Boundary,
            Fault::Shift128Boundary,
            Fault::NonpositiveShift,
        ] {
            let _reset = inject(fault);
            assert_eq!(
                convert(single, width, bits),
                expected,
                "injected={fault:?}: {line}"
            );
            let trace = state();
            assert_eq!(
                (trace.entered, trace.fallback),
                (1, 1),
                "actual dispatch did not reject and execute exact fallback; {fault:?}: {line}"
            );
            checks += 1;
        }
    }
    assert_eq!(
        checks, 2_010,
        "all 402 fixtures must exercise all five rejection modes"
    );
}

#[test]
fn injection_does_not_bypass_fast_or_special_dispatch() {
    for (single, cases) in [
        (
            false,
            &[
                0,
                1_u64 << 63,
                0x3ff0000000000000,
                0x4008000000000000,
                0x7ff0000000000000,
                0xfff8000000000012,
            ][..],
        ),
        (
            true,
            &[
                0,
                1_u64 << 31,
                0x3f800000,
                0x40400000,
                0x7f7fffff,
                0x7f800000,
                0xffc00012,
            ][..],
        ),
    ] {
        for &bits in cases {
            for width in [32, 64, 128] {
                reset(Fault::None);
                let expected = convert(single, width, bits);
                let _reset = inject(Fault::RoundBoundary);
                assert_eq!(convert(single, width, bits), expected);
                assert_eq!((state().entered, state().fallback), (0, 0));
            }
        }
    }
}

// Check fixed-point constants with exact integer inequalities, independently
// of floating-point logarithms and the production cached-power arithmetic.
fn assert_power_interval(n: &num_bigint::BigUint, d: &num_bigint::BigUint, base: u32, k: i32) {
    let one = num_bigint::BigUint::from(1_u32);
    let power = num_bigint::BigUint::from(base).pow(k.unsigned_abs());
    let (p, q) = if k >= 0 { (power, one) } else { (one, power) };
    let value = n * q;
    let lower = d * p;
    assert!(value >= lower);
    assert!(value < lower * base);
}

#[test]
fn fixed_point_logarithms_and_all_cached_powers_are_exact() {
    use num_bigint::BigUint;
    let one = BigUint::from(1_u32);
    for e in -1074_i32..=1023 {
        let power = &one << e.unsigned_abs() as usize;
        let (n, d) = if e >= 0 {
            (power, one.clone())
        } else {
            (one.clone(), power)
        };
        assert_power_interval(&n, &d, 10, (e * 78913) >> 18);
    }
    for s in -342_i32..=338 {
        let power = BigUint::from(10_u32).pow(s.unsigned_abs());
        let (n, d) = if s >= 0 {
            (power, one.clone())
        } else {
            (one.clone(), power)
        };
        let log2_10 = (s * 217706) >> 16;
        assert_power_interval(&n, &d, 2, log2_10);

        // floor(log2(5^s)) = floor(log2(10^s)) - s. Reconstruct
        // floor(5^s * 2^(127-floor(log2(5^s)))) as an exact ratio.
        let power = BigUint::from(5_u32).pow(s.unsigned_abs());
        let (mut n, mut d) = if s >= 0 {
            (power, one.clone())
        } else {
            (one.clone(), power)
        };
        let shift = 127 - (log2_10 - s);
        if shift >= 0 {
            n <<= shift as usize;
        } else {
            d <<= shift.unsigned_abs() as usize;
        }
        let cached = if s <= 308 {
            BINARY_POW5_CACHE[(s + 342) as usize]
        } else {
            BINARY_POW5_CACHE_EXT[(s - 309) as usize]
        };
        assert!(cached >= 1_u128 << 127);
        assert_eq!(BigUint::from(cached), n / d, "cache exponent {s}");
    }
}

#[test]
fn finish_handles_decimal_carry_qmin_and_overflow() {
    use crate::layout::W32;
    let finite = |coefficient, exponent, neg| match finish::<W32>(coefficient, exponent, neg, 7) {
        Converted::Finite(value) => {
            let value = value.unpacked();
            (value.coeff, value.exp, value.neg)
        }
        _ => panic!("expected finite finish"),
    };
    assert_eq!(finite(10_000_000, -101, false), (1_000_000, -100, false));
    assert_eq!(finite(1, -101, true), (1, -101, true));
    assert_eq!(finite(0, -101, true), (0, 0, true));
    assert_eq!(finite(999_999, 91, false), (9_999_990, 90, false));
    assert_eq!(finite(9_999_999, 90, false), (9_999_999, 90, false));
    for neg in [false, true] {
        assert!(
            matches!(finish::<W32>(10_000_000, 90, neg, 7), Converted::Infinite(sign) if sign == neg)
        );
    }
}

#[test]
fn natural_qmin_clamp_covers_shift_128_and_zero_certificate() {
    let _reset = inject(Fault::None);
    assert_eq!(interval_from_binary(1, -337, 7, -101), Some((0, -101)));
    assert_eq!(state().shift128, 1);
    assert_eq!(interval_from_binary(1, -338, 7, -101), Some((0, -101)));
    assert_eq!(state().zero, 1);
    assert_eq!(interval_from_binary(1, -336, 7, -101), Some((1, -101)));
}
