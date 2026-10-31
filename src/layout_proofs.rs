use crate::layout::{
    InRange, Unpacked, W32, W64, W128, Width, decode_32, decode_64, decode_128, encode_32,
    encode_64, encode_128, unpack_canonical_128,
};
use crate::operand::sealed::{Class, Sealed};
use crate::{Decimal32, Decimal64, Decimal128, Error, Rounding};

struct Format {
    bits: u32,
    exp_bits: u32,
    trail_bits: u32,
    bias: i32,
    qmax: i32,
    digits: u32,
}

const F32: Format = Format {
    bits: 32,
    exp_bits: 8,
    trail_bits: 20,
    bias: 101,
    qmax: 90,
    digits: 7,
};

const F64: Format = Format {
    bits: 64,
    exp_bits: 10,
    trail_bits: 50,
    bias: 398,
    qmax: 369,
    digits: 16,
};

const F128: Format = Format {
    bits: 128,
    exp_bits: 14,
    trail_bits: 110,
    bias: 6176,
    qmax: 6111,
    digits: 34,
};

fn mask(width: u32) -> u128 {
    if width == 128 {
        u128::MAX
    } else {
        (1_u128 << width) - 1
    }
}

fn interchange(f: &Format, bits: u128) -> Option<Unpacked> {
    let neg = (bits >> (f.bits - 1)) & 1 == 1;
    let combination_head = (bits >> (f.bits - 5)) & 0b1111;
    if combination_head == 0b1111 {
        return None;
    }
    let exp_mask = mask(f.exp_bits);
    let (coeff, biased) = if (bits >> (f.bits - 3)) & 0b11 == 0b11 {
        (
            (1_u128 << (f.trail_bits + 3)) | (bits & mask(f.trail_bits + 1)),
            (bits >> (f.trail_bits + 1)) & exp_mask,
        )
    } else {
        (
            bits & mask(f.trail_bits + 3),
            (bits >> (f.trail_bits + 3)) & exp_mask,
        )
    };
    if coeff >= 10_u128.pow(f.digits) {
        return None;
    }
    Some(Unpacked {
        neg,
        coeff,
        exp: biased as i32 - f.bias,
    })
}

/// A canonical infinity or NaN encoding: the combination field starts with
/// 11110 or 11111, and a NaN has only its sign, signaling bit, and a payload
/// below `10^(P-1)` set besides those five bits.
fn canonical_special(f: &Format, bits: u128) -> bool {
    let rest = bits & mask(f.bits - 6);
    match (bits >> (f.bits - 6)) & 0b11111 {
        0b11110 => rest == 0,
        0b11111 => {
            let payload = rest & mask(f.trail_bits);
            rest & !(payload | (1 << (f.bits - 7))) == 0 && payload < 10_u128.pow(f.digits - 1)
        }
        _ => false,
    }
}

fn finite_fields<T: Sealed>(x: T) -> Unpacked {
    match x.class() {
        Class::Finite(v) => v,
        _ => panic!("not a finite value"),
    }
}

fn same(a: Unpacked, b: Unpacked) -> bool {
    a.neg == b.neg && a.coeff == b.coeff && a.exp == b.exp
}

fn any_in_range<W: Width>() -> Unpacked {
    let neg: bool = kani::any();
    let coeff: u128 = kani::any();
    let exp: i32 = kani::any();
    kani::assume(coeff < 10_u128.pow(W::LAYOUT.digits));
    kani::assume(exp >= W::LAYOUT.qmin && exp <= W::LAYOUT.qmax);
    Unpacked { neg, coeff, exp }
}

macro_rules! layout_proofs {
    ($bits:ty, $width:ty, $format:expr, $decimal:ty, $decode:ident, $encode:ident,
     $decode_proof:ident, $encode_proof:ident, $checked_proof:ident, $public_proof:ident, $zero_proof:ident) => {
        #[kani::proof]
        fn $decode_proof() {
            let bits: $bits = kani::any();
            let expected = interchange(&$format, bits as u128);
            match ($decode(bits), expected) {
                (Ok(v), Some(e)) => {
                    assert!(same(v, e));
                    assert_eq!($encode(InRange::unchecked(v)), bits);
                }
                (Err(error), None) => assert_eq!(error, Error::InvalidInput),
                _ => panic!("decode disagrees with the interchange format"),
            }
        }

        #[kani::proof]
        fn $encode_proof() {
            let v = any_in_range::<$width>();
            let Some(value) = InRange::<$width>::checked(v.neg, v.coeff, i128::from(v.exp)) else {
                panic!("in-range fields rejected");
            };
            let bits = $encode(value);
            let Ok(decoded) = $decode(bits) else {
                panic!("encoded fields do not decode");
            };
            assert!(same(decoded, v));
        }

        #[kani::proof]
        fn $checked_proof() {
            let neg: bool = kani::any();
            let coeff: u128 = kani::any();
            let exp: i128 = kani::any();
            let inside = coeff < 10_u128.pow($format.digits)
                && exp >= i128::from(<$width as Width>::LAYOUT.qmin)
                && exp <= i128::from(<$width as Width>::LAYOUT.qmax)
                && <$width as Width>::LAYOUT.qmin == -$format.bias
                && <$width as Width>::LAYOUT.qmax == $format.qmax
                && <$width as Width>::LAYOUT.digits == $format.digits;
            match InRange::<$width>::checked(neg, coeff, exp) {
                Some(value) => {
                    assert!(inside);
                    let v = value.unpacked();
                    assert!(v.neg == neg && v.coeff == coeff && i128::from(v.exp) == exp);
                }
                None => assert!(!inside),
            }
        }

        #[kani::proof]
        fn $zero_proof() {
            let bits: $bits = kani::any();
            if let Ok(value) = <$decimal>::from_bits(bits) {
                assert_eq!(
                    value.is_zero(),
                    matches!(value.class(), Class::Finite(v) if v.coeff == 0)
                );
            }
        }

        #[kani::proof]
        fn $public_proof() {
            let bits: $bits = kani::any();
            match (<$decimal>::from_bits(bits), $decode(bits)) {
                (Ok(x), Ok(v)) => {
                    assert_eq!(x.to_bits(), bits);
                    assert!(same(finite_fields(x), v));
                    assert_eq!(x.is_zero(), v.coeff == 0);
                    assert_eq!(x.is_positive(), !v.neg && v.coeff != 0);
                    assert_eq!(x.is_negative(), v.neg && v.coeff != 0);
                }
                (Ok(x), Err(_)) => {
                    assert!(canonical_special(&$format, bits as u128));
                    assert_eq!(x.to_bits(), bits);
                    assert!(!matches!(x.class(), Class::Finite(_)));
                    assert!(!x.is_zero() && !x.is_finite());
                    assert_eq!(x.is_nan(), (bits >> ($format.bits - 6)) & 1 == 1);
                }
                (Err(a), Err(b)) => {
                    assert!(!canonical_special(&$format, bits as u128));
                    assert_eq!(a, b);
                }
                (Err(_), Ok(_)) => panic!("public storage rejects a decodable value"),
            }
        }
    };
}

layout_proofs!(
    u32,
    W32,
    F32,
    Decimal32,
    decode_32,
    encode_32,
    decode_32_matches_interchange_format,
    encode_32_round_trips_every_in_range_field,
    in_range_32_accepts_exactly_the_layout_bounds,
    public_storage_32_matches_decode,
    is_zero_32_matches_class
);
layout_proofs!(
    u64,
    W64,
    F64,
    Decimal64,
    decode_64,
    encode_64,
    decode_64_matches_interchange_format,
    encode_64_round_trips_every_in_range_field,
    in_range_64_accepts_exactly_the_layout_bounds,
    public_storage_64_matches_decode,
    is_zero_64_matches_class
);
layout_proofs!(
    u128,
    W128,
    F128,
    Decimal128,
    decode_128,
    encode_128,
    decode_128_matches_interchange_format,
    encode_128_round_trips_every_in_range_field,
    in_range_128_accepts_exactly_the_layout_bounds,
    public_storage_128_matches_decode,
    is_zero_128_matches_class
);

#[kani::proof]
fn unpack_canonical_128_matches_decode() {
    let bits: u128 = kani::any();
    if let Ok(v) = decode_128(bits) {
        assert!(same(unpack_canonical_128(bits), v));
        let Ok(value) = Decimal128::from_bits(bits) else {
            panic!("canonical finite storage rejected");
        };
        assert!(same(Sealed::unpack(value), v));
    }
}

#[kani::proof]
fn constants_encode_their_fields() {
    let d32 = [Decimal32::ZERO, Decimal32::MAX, Decimal32::MIN].map(finite_fields);
    let d64 = [Decimal64::ZERO, Decimal64::MAX, Decimal64::MIN].map(finite_fields);
    let d128 = [Decimal128::ZERO, Decimal128::MAX, Decimal128::MIN].map(finite_fields);
    for (nan, inf, neg_inf, f) in [
        (
            Decimal32::NAN.to_bits() as u128,
            Decimal32::INFINITY.to_bits() as u128,
            Decimal32::NEG_INFINITY.to_bits() as u128,
            &F32,
        ),
        (
            Decimal64::NAN.to_bits() as u128,
            Decimal64::INFINITY.to_bits() as u128,
            Decimal64::NEG_INFINITY.to_bits() as u128,
            &F64,
        ),
        (
            Decimal128::NAN.to_bits(),
            Decimal128::INFINITY.to_bits(),
            Decimal128::NEG_INFINITY.to_bits(),
            &F128,
        ),
    ] {
        assert_eq!(nan, 0b11111 << (f.bits - 6));
        assert_eq!(inf, 0b11110 << (f.bits - 6));
        assert_eq!(neg_inf, inf | (1 << (f.bits - 1)));
        assert!(
            canonical_special(f, nan) && canonical_special(f, inf) && canonical_special(f, neg_inf)
        );
    }
    for (values, f) in [(d32, &F32), (d64, &F64), (d128, &F128)] {
        let max = 10_u128.pow(f.digits) - 1;
        assert!(same(
            values[0],
            Unpacked {
                neg: false,
                coeff: 0,
                exp: 0
            }
        ));
        assert!(same(
            values[1],
            Unpacked {
                neg: false,
                coeff: max,
                exp: f.qmax
            }
        ));
        assert!(same(
            values[2],
            Unpacked {
                neg: true,
                coeff: max,
                exp: f.qmax
            }
        ));
    }
}

macro_rules! widening_proof {
    ($name:ident, $small:ty, $large:ty, $small_bits:ty, $small_f:expr, $large_f:expr,
     $decode_large:ident, $rounded:ident) => {
        #[kani::proof]
        fn $name() {
            let bits: $small_bits = kani::any();
            let Ok(x) = <$small>::from_bits(bits) else {
                return;
            };
            let wide = <$large>::from(x);
            let dir = if kani::any() {
                Rounding::TowardNegative
            } else {
                Rounding::TowardPositive
            };
            let Class::Finite(u) = x.class() else {
                let quiet = bits & !(1 << (<$small_bits>::BITS - 7));
                let (small, large) = (&$small_f, &$large_f);
                let narrow = u128::from(bits);
                let sign = (narrow >> (small.bits - 1)) << (large.bits - 1);
                let widened = if (narrow >> (small.bits - 6)) & 0b11111 == 0b11111 {
                    sign | (0b11111 << (large.bits - 6)) | (narrow & mask(small.trail_bits))
                } else {
                    sign | (0b11110 << (large.bits - 6))
                };
                assert_eq!(u128::from(wide.to_bits()), widened);
                assert_eq!(wide.is_nan(), x.is_nan());
                assert_eq!(wide.is_infinite(), x.is_infinite());
                assert_eq!(wide.is_sign_negative(), x.is_sign_negative());
                assert_eq!(<$small>::try_from(wide).map(|y| y.to_bits()), Ok(quiet));
                assert_eq!(
                    <$small>::$rounded(wide, dir).map(|y| y.to_bits()),
                    Ok(quiet)
                );
                return;
            };
            let Ok(v) = $decode_large(wide.to_bits()) else {
                panic!("widened storage is not canonical");
            };
            assert!(same(v, u));
            assert_eq!(<$small>::try_from(wide).map(|y| y.to_bits()), Ok(bits));
            assert_eq!(<$small>::$rounded(wide, dir).map(|y| y.to_bits()), Ok(bits));
        }
    };
}

widening_proof!(
    widen_32_to_64_and_narrow_back,
    Decimal32,
    Decimal64,
    u32,
    F32,
    F64,
    decode_64,
    from_d64_rounded
);
widening_proof!(
    widen_32_to_128_and_narrow_back,
    Decimal32,
    Decimal128,
    u32,
    F32,
    F128,
    decode_128,
    from_d128_rounded
);
widening_proof!(
    widen_64_to_128_and_narrow_back,
    Decimal64,
    Decimal128,
    u64,
    F64,
    F128,
    decode_128,
    from_d128_rounded
);
