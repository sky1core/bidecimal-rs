#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use core::marker::PhantomData;

use crate::Error;
use crate::tables::pow10;

#[cfg(verus_keep_ghost)]
use vstd::arithmetic::{power::pow, power2::pow2};
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg(not(verus_keep_ghost))]
macro_rules! plain {
    ($($item:tt)*) => { $($item)* };
}

macro_rules! verified_items {
    ($($item:tt)*) => {
        #[cfg(verus_keep_ghost)]
        verus! { $($item)* }
        #[cfg(not(verus_keep_ghost))]
        plain! { $($item)* }
    };
}

verified_items! {
    pub(crate) struct W32;
    pub(crate) struct W64;
    pub(crate) struct W128;

    pub(crate) struct Layout {
        pub(crate) digits: u32,
        pub(crate) qmin: i32,
        pub(crate) qmax: i32,
        pub(crate) bias: i32,
        pub(crate) exp_bits: u32,
        pub(crate) trail_bits: u32,
    }

    mod sealed {
        pub trait Sealed {}
    }

    pub(crate) trait Width: sealed::Sealed {
        type Bits;
        const LAYOUT: Layout;
    }

    impl sealed::Sealed for W32 {}
    impl sealed::Sealed for W64 {}
    impl sealed::Sealed for W128 {}

    impl Width for W32 {
        type Bits = u32;
        const LAYOUT: Layout = Layout {
            digits: 7,
            qmin: -101,
            qmax: 90,
            bias: 101,
            exp_bits: 8,
            trail_bits: 20,
        };
    }

    impl Width for W64 {
        type Bits = u64;
        const LAYOUT: Layout = Layout {
            digits: 16,
            qmin: -398,
            qmax: 369,
            bias: 398,
            exp_bits: 10,
            trail_bits: 50,
        };
    }

    impl Width for W128 {
        type Bits = u128;
        const LAYOUT: Layout = Layout {
            digits: 34,
            qmin: -6176,
            qmax: 6111,
            bias: 6176,
            exp_bits: 14,
            trail_bits: 110,
        };
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy)]
pub(crate) struct Unpacked {
    pub(crate) neg: bool,
    pub(crate) coeff: u128,
    pub(crate) exp: i32,
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(crate) struct InRange<W: Width> {
    value: Unpacked,
    width: PhantomData<W>,
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
impl<W: Width> InRange<W> {
    #[cfg_attr(verus_keep_ghost, verus_spec(ret =>
        requires bid_width(W::LAYOUT),
        ensures
            ret.is_some() == (coeff < pow(10, W::LAYOUT.digits as nat)
                && W::LAYOUT.qmin <= exp <= W::LAYOUT.qmax),
            match ret {
                Some(r) => r.view() == (Unpacked { neg, coeff, exp: exp as i32 }),
                None => true,
            },
    ))]
    pub(crate) const fn checked(neg: bool, coeff: u128, exp: i128) -> Option<Self> {
        if coeff < pow10(W::LAYOUT.digits)
            && exp >= W::LAYOUT.qmin as i128
            && exp <= W::LAYOUT.qmax as i128
        {
            Some(Self::unchecked(Unpacked {
                neg,
                coeff,
                exp: exp as i32,
            }))
        } else {
            None
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(ret =>
        requires bid_width(W::LAYOUT), in_range(value, W::LAYOUT),
        ensures ret.view() == value,
    ))]
    pub(crate) const fn unchecked(value: Unpacked) -> Self {
        Self {
            value,
            width: PhantomData,
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(ret =>
        ensures ret == self.view(), bid_width(W::LAYOUT), in_range(ret, W::LAYOUT),
    ))]
    pub(crate) const fn unpacked(self) -> Unpacked {
        #[cfg(verus_keep_ghost)]
        proof! { use_type_invariant(&self); }
        self.value
    }
}

#[cfg(verus_keep_ghost)]
verus! {

pub(crate) open spec fn bid_width(lay: Layout) -> bool {
    lay == W32::LAYOUT || lay == W64::LAYOUT || lay == W128::LAYOUT
}

pub(crate) proof fn lemma_layout_values()
    ensures
        W32::LAYOUT.digits == 7 && W32::LAYOUT.qmin == -101 && W32::LAYOUT.qmax == 90
            && W32::LAYOUT.bias == 101 && W32::LAYOUT.exp_bits == 8 && W32::LAYOUT.trail_bits == 20,
        W64::LAYOUT.digits == 16 && W64::LAYOUT.qmin == -398 && W64::LAYOUT.qmax == 369
            && W64::LAYOUT.bias == 398 && W64::LAYOUT.exp_bits == 10 && W64::LAYOUT.trail_bits == 50,
        W128::LAYOUT.digits == 34 && W128::LAYOUT.qmin == -6176 && W128::LAYOUT.qmax == 6111
            && W128::LAYOUT.bias == 6176 && W128::LAYOUT.exp_bits == 14 && W128::LAYOUT.trail_bits == 110,
{
}

pub(crate) open spec fn in_range(value: Unpacked, lay: Layout) -> bool {
    value.coeff < pow(10, lay.digits as nat) && lay.qmin <= value.exp <= lay.qmax
}

impl<W: Width> InRange<W> {
    #[verifier::type_invariant]
    spec fn inv(self) -> bool {
        bid_width(W::LAYOUT) && in_range(self.value, W::LAYOUT)
    }

    pub(crate) closed spec fn view(self) -> Unpacked {
        self.value
    }
}

pub(crate) open spec fn field(bits: int, low: nat, len: nat) -> int {
    (bits / (pow2(low) as int)) % (pow2(len) as int)
}

pub(crate) open spec fn bid_sign(bits: int, lay: Layout) -> bool {
    field(bits, (lay.exp_bits + lay.trail_bits + 3) as nat, 1) == 1
}

pub(crate) open spec fn bid_finite(bits: int, lay: Layout) -> bool {
    field(bits, (lay.exp_bits + lay.trail_bits - 2) as nat, 5) < 30
}

pub(crate) open spec fn bid_large(bits: int, lay: Layout) -> bool {
    field(bits, (lay.exp_bits + lay.trail_bits + 1) as nat, 2) == 3
}

pub(crate) open spec fn bid_coeff(bits: int, lay: Layout) -> int {
    if bid_large(bits, lay) {
        pow2((lay.trail_bits + 3) as nat) + field(bits, 0, (lay.trail_bits + 1) as nat)
    } else {
        field(bits, 0, (lay.trail_bits + 3) as nat)
    }
}

pub(crate) open spec fn bid_exp(bits: int, lay: Layout) -> int {
    let low = if bid_large(bits, lay) { lay.trail_bits + 1 } else { lay.trail_bits + 3 };
    field(bits, low as nat, lay.exp_bits as nat) - lay.bias
}

pub(crate) open spec fn bid_fields(bits: int, lay: Layout, value: Unpacked) -> bool {
    value.neg == bid_sign(bits, lay) && value.coeff == bid_coeff(bits, lay)
        && value.exp == bid_exp(bits, lay)
}

pub(crate) open spec fn bid_canonical(bits: int, lay: Layout) -> bool {
    bid_finite(bits, lay) && bid_coeff(bits, lay) < pow(10, lay.digits as nat)
}

}

#[cfg(verus_keep_ghost)]
macro_rules! bid_proofs {
    (
        $width:ident, $bits:ty, sign: $s:literal, form: $g:literal, special: $c:literal,
        small_coeff: $sm:literal, large_coeff: $lg:literal, exp_bits: $e:literal
    ) => {
        verus! {
        impl $width {
            pub(crate) proof fn lemma_shifts()
                ensures
                    1 <= (1 as $bits) << ($e as u32) <= 16384,
                    1 <= (1 as $bits) << ($lg as u32),
                    1 <= (1 as $bits) << ($sm as u32),
            {
                assert(1 <= (1 as $bits) << ($e as u32) && (1 as $bits) << ($e as u32) <= 16384)
                    by (bit_vector);
                assert(1 <= (1 as $bits) << ($lg as u32)) by (bit_vector);
                assert(1 <= (1 as $bits) << ($sm as u32)) by (bit_vector);
            }

            pub(crate) proof fn lemma_unpack(
                bits: $bits,
                sign_shift: u32,
                small: u32,
                large: u32,
                exp_mask: $bits,
            )
                requires
                    sign_shift == $s,
                    small == $sm,
                    large == $lg,
                    exp_mask as int == ((1 as $bits) << ($e as u32)) - 1,
                ensures
                    bid_sign(bits as int, $width::LAYOUT) == (bits >> sign_shift != 0),
                    bid_finite(bits as int, $width::LAYOUT)
                        == ((bits >> (sign_shift - 5)) & 0b11111 < 0b11110),
                    bid_large(bits as int, $width::LAYOUT)
                        == ((bits >> (sign_shift - 2)) & 0b11 == 0b11),
                    bid_large(bits as int, $width::LAYOUT) ==> {
                        &&& bid_coeff(bits as int, $width::LAYOUT) == (((1 as $bits) << small)
                            | (bits & ((((1 as $bits) << large) - 1) as $bits))) as int
                        &&& bid_exp(bits as int, $width::LAYOUT)
                            == ((bits >> large) & exp_mask) as int - $width::LAYOUT.bias
                    },
                    !bid_large(bits as int, $width::LAYOUT) ==> {
                        &&& bid_coeff(bits as int, $width::LAYOUT)
                            == (bits & ((((1 as $bits) << small) - 1) as $bits)) as int
                        &&& bid_exp(bits as int, $width::LAYOUT)
                            == ((bits >> small) & exp_mask) as int - $width::LAYOUT.bias
                    },
                    bid_finite(bits as int, $width::LAYOUT) ==> $width::LAYOUT.qmin
                        <= bid_exp(bits as int, $width::LAYOUT) <= $width::LAYOUT.qmax,
                    (bits >> small) & exp_mask <= exp_mask,
                    (bits >> large) & exp_mask <= exp_mask,
            {
                let mask = ((((1 as $bits) << ($e as u32)) - 1) as $bits);
                let small_bit = (1 as $bits) << ($sm as u32);
                let large_bit = (1 as $bits) << ($lg as u32);
                assert((bits >> $sm) & exp_mask <= exp_mask) by (bit_vector);
                assert((bits >> $lg) & exp_mask <= exp_mask) by (bit_vector);
                reveal(pow);
                assert(pow2(0) == 1 && pow2(1) == 2 && pow2(2) == 4 && pow2(5) == 32)
                    by (compute_only);
                assert(pow2($s) == ((1 as $bits) << ($s as u32)) as nat) by (compute_only);
                assert(pow2($g) == ((1 as $bits) << ($g as u32)) as nat) by (compute_only);
                assert(pow2($c) == ((1 as $bits) << ($c as u32)) as nat) by (compute_only);
                assert(pow2($sm) == ((1 as $bits) << ($sm as u32)) as nat) by (compute_only);
                assert(pow2($lg) == ((1 as $bits) << ($lg as u32)) as nat) by (compute_only);
                assert(pow2($e) == ((1 as $bits) << ($e as u32)) as nat) by (compute_only);
                assert(((1 as $bits) << ($e as u32)) - 1 == ((((1 as $bits) << ($e as u32)) - 1) as $bits))
                    by (compute_only);
                assert(3 * (((1 as $bits) << (($e - 2) as u32)) as int)
                    == $width::LAYOUT.qmax + $width::LAYOUT.bias + 1) by (compute_only);
                assert((bits >> $s != 0) == ((bits / ((1 as $bits) << ($s as u32))) % 2 == 1))
                    by (bit_vector);
                assert((bits >> $c) & 0b11111 == (bits / ((1 as $bits) << ($c as u32))) % 32)
                    by (bit_vector);
                assert((bits >> $g) & 0b11 == (bits / ((1 as $bits) << ($g as u32))) % 4)
                    by (bit_vector);
                assert(small_bit | (bits & ((large_bit - 1) as $bits)) == small_bit + bits % large_bit)
                    by (bit_vector)
                    requires
                        small_bit == (1 as $bits) << ($sm as u32),
                        large_bit == (1 as $bits) << ($lg as u32);
                assert(bits & ((small_bit - 1) as $bits) == bits % small_bit) by (bit_vector)
                    requires small_bit == (1 as $bits) << ($sm as u32);
                assert((bits >> $lg) & mask == (bits / large_bit) % ((mask + 1) as $bits)) by (bit_vector)
                    requires
                        large_bit == (1 as $bits) << ($lg as u32),
                        mask == (((1 as $bits) << ($e as u32)) - 1) as $bits;
                assert((bits >> $sm) & mask == (bits / small_bit) % ((mask + 1) as $bits)) by (bit_vector)
                    requires
                        small_bit == (1 as $bits) << ($sm as u32),
                        mask == (((1 as $bits) << ($e as u32)) - 1) as $bits;
                assert((bits >> $c) & 0b11111 < 0b11110 ==> {
                    &&& ((bits >> $g) & 0b11 == 0b11
                        ==> (bits >> $lg) & mask < 3 * ((1 as $bits) << (($e - 2) as u32)))
                    &&& ((bits >> $g) & 0b11 != 0b11
                        ==> (bits >> $sm) & mask < 3 * ((1 as $bits) << (($e - 2) as u32)))
                }) by (bit_vector)
                    requires mask == (((1 as $bits) << ($e as u32)) - 1) as $bits;
            }

            pub(crate) proof fn lemma_encode(
                value: Unpacked,
                sign_shift: u32,
                small: u32,
                large: u32,
                top: u32,
            )
                requires
                    sign_shift == $s,
                    small == $sm,
                    large == $lg,
                    top == $g,
                    in_range(value, $width::LAYOUT),
                ensures
                    ({
                        let exp = (value.exp + $width::LAYOUT.bias) as $bits;
                        let coeff = value.coeff as $bits;
                        let sign = (value.neg as $bits) << sign_shift;
                        let r = sign | (exp << small) | coeff;
                        let q = sign | ((3 as $bits) << top) | (exp << large)
                            | (coeff & ((((1 as $bits) << large) - 1) as $bits));
                        &&& coeff < (1 as $bits) << small ==> bid_canonical(r as int, $width::LAYOUT)
                            && bid_fields(r as int, $width::LAYOUT, value)
                        &&& coeff >= (1 as $bits) << small ==> bid_canonical(q as int, $width::LAYOUT)
                            && bid_fields(q as int, $width::LAYOUT, value)
                    }),
            {
                let n = value.neg as $bits;
                let exp = (value.exp + $width::LAYOUT.bias) as $bits;
                let coeff = value.coeff as $bits;
                let mask = ((((1 as $bits) << ($e as u32)) - 1) as $bits);
                let small_bit = (1 as $bits) << ($sm as u32);
                let large_bit = (1 as $bits) << ($lg as u32);
                let exp_limit = (3 * ((1 as $bits) << (($e - 2) as u32))) as $bits;
                reveal(pow);
                assert(pow(10, $width::LAYOUT.digits as nat)
                    <= ((1 as $bits) << ($sm as u32)) as int + ((1 as $bits) << ($lg as u32)) as int)
                    by (compute_only);
                assert(((1 as $bits) << ($sm as u32)) as int + ((1 as $bits) << ($lg as u32)) as int
                    <= $bits::MAX as int) by (compute_only);
                assert((3 * ((1 as $bits) << (($e - 2) as u32))) as int == $width::LAYOUT.qmax + $width::LAYOUT.bias + 1)
                    by (compute_only);
                assert(((1 as $bits) << ($e as u32)) - 1 == ((((1 as $bits) << ($e as u32)) - 1) as $bits)) by (compute_only);
                assert(n <= 1);
                assert(coeff as int == value.coeff);
                assert(exp as int == value.exp + $width::LAYOUT.bias);
                let r = (n << ($s as u32)) | (exp << ($sm as u32)) | coeff;
                assert(n <= 1 && exp < exp_limit && coeff < small_bit ==> {
                    &&& (r >> $s != 0) == (n == 1)
                    &&& (r >> $c) & 0b11111 < 0b11110
                    &&& (r >> $g) & 0b11 != 0b11
                    &&& r & ((small_bit - 1) as $bits) == coeff
                    &&& (r >> $sm) & mask == exp
                }) by (bit_vector)
                    requires
                        r == (n << ($s as u32)) | (exp << ($sm as u32)) | coeff,
                        small_bit == (1 as $bits) << ($sm as u32),
                        mask == (((1 as $bits) << ($e as u32)) - 1) as $bits,
                        exp_limit == (3 * ((1 as $bits) << (($e - 2) as u32))) as $bits;
                Self::lemma_unpack(r, $s, $sm, $lg, mask);
                let q = (n << ($s as u32)) | ((3 as $bits) << ($g as u32)) | (exp << ($lg as u32))
                    | (coeff & ((large_bit - 1) as $bits));
                assert(n <= 1 && exp < exp_limit && small_bit <= coeff < small_bit + large_bit ==> {
                    &&& (q >> $s != 0) == (n == 1)
                    &&& (q >> $c) & 0b11111 < 0b11110
                    &&& (q >> $g) & 0b11 == 0b11
                    &&& small_bit | (q & ((large_bit - 1) as $bits)) == coeff
                    &&& (q >> $lg) & mask == exp
                }) by (bit_vector)
                    requires
                        q == (n << ($s as u32)) | ((3 as $bits) << ($g as u32))
                            | (exp << ($lg as u32)) | (coeff & ((large_bit - 1) as $bits)),
                        small_bit == (1 as $bits) << ($sm as u32),
                        large_bit == (1 as $bits) << ($lg as u32),
                        mask == (((1 as $bits) << ($e as u32)) - 1) as $bits,
                        exp_limit == (3 * ((1 as $bits) << (($e - 2) as u32))) as $bits;
                Self::lemma_unpack(q, $s, $sm, $lg, mask);
            }
        }
        }
    };
}

#[cfg(verus_keep_ghost)]
bid_proofs!(W32, u32, sign: 31, form: 29, special: 26, small_coeff: 23, large_coeff: 21, exp_bits: 8);
#[cfg(verus_keep_ghost)]
bid_proofs!(W64, u64, sign: 63, form: 61, special: 58, small_coeff: 53, large_coeff: 51, exp_bits: 10);
#[cfg(verus_keep_ghost)]
bid_proofs!(W128, u128, sign: 127, form: 125, special: 122, small_coeff: 113, large_coeff: 111, exp_bits: 14);

macro_rules! bid_layout {
    ($width:ty, $decode:ident, $unpack:ident, $encode:ident) => {
        #[expect(
            clippy::arithmetic_side_effects,
            reason = "width is 32/64/128, so sign_shift - 5 is 26/58/122; P is 7/16/34, so 10^P - 1 fits u128"
        )]
        #[cfg_attr(verus_keep_ghost, verus_spec(ret =>
            ensures
                match ret {
                    Ok(v) => bid_canonical(bits as int, <$width as Width>::LAYOUT)
                        && bid_fields(bits as int, <$width as Width>::LAYOUT, v)
                        && in_range(v, <$width as Width>::LAYOUT),
                    Err(e) => !bid_canonical(bits as int, <$width as Width>::LAYOUT)
                        && e == Error::InvalidInput,
                },
        ))]
        pub(crate) fn $decode(bits: <$width as Width>::Bits) -> Result<Unpacked, Error> {
            let sign_shift = <<$width as Width>::Bits>::BITS - 1;
            #[cfg(verus_keep_ghost)]
            proof! {
                let lay = <$width as Width>::LAYOUT;
                <$width>::lemma_shifts();
                <$width>::lemma_unpack(
                    bits,
                    sign_shift,
                    (lay.trail_bits + 3) as u32,
                    (lay.trail_bits + 1) as u32,
                    (((1 as <$width as Width>::Bits) << lay.exp_bits) - 1) as <$width as Width>::Bits,
                );
            }
            if (bits >> (sign_shift - 5)) & 0b11111 >= 0b11110 {
                return Err(Error::InvalidInput);
            }
            let value = $unpack(bits);
            #[cfg(verus_keep_ghost)]
            proof! { vstd::arithmetic::power::lemma_pow_positive(10, <$width as Width>::LAYOUT.digits as nat); }
            if value.coeff > pow10(<$width as Width>::LAYOUT.digits) - 1 {
                return Err(Error::InvalidInput);
            }
            Ok(value)
        }

        #[expect(
            clippy::arithmetic_side_effects,
            reason = "width is 32/64/128; trail_bits is 20/50/110, exp_bits is 8/10/14; exponent field <= 16383, bias <= 6176, so subtraction fits i32"
        )]
        #[cfg_attr(verus_keep_ghost, verus_spec(ret =>
            ensures bid_fields(bits as int, <$width as Width>::LAYOUT, ret),
        ))]
        pub(crate) fn $unpack(bits: <$width as Width>::Bits) -> Unpacked {
            #[cfg(verus_keep_ghost)]
            proof! { <$width>::lemma_shifts(); }
            let sign_shift = <<$width as Width>::Bits>::BITS - 1;
            let small_coeff_bits = <$width as Width>::LAYOUT.trail_bits + 3;
            let large_coeff_bits = <$width as Width>::LAYOUT.trail_bits + 1;
            let exp_mask: <$width as Width>::Bits = (1 << <$width as Width>::LAYOUT.exp_bits) - 1;
            #[cfg(verus_keep_ghost)]
            proof! { <$width>::lemma_unpack(bits, sign_shift, small_coeff_bits, large_coeff_bits, exp_mask); }
            let (coeff, biased_exp) = if (bits >> (sign_shift - 2)) & 0b11 == 0b11 {
                (
                    (1 << small_coeff_bits) | (bits & ((1 << large_coeff_bits) - 1)),
                    (bits >> large_coeff_bits) & exp_mask,
                )
            } else {
                (
                    bits & ((1 << small_coeff_bits) - 1),
                    (bits >> small_coeff_bits) & exp_mask,
                )
            };
            Unpacked {
                neg: bits >> sign_shift != 0,
                coeff: coeff as u128,
                exp: biased_exp as i32 - <$width as Width>::LAYOUT.bias,
            }
        }

        #[expect(
            clippy::arithmetic_side_effects,
            reason = "coeff < 10^7/10^16/10^34 fits the destination width; exp in qmin..=qmax gives exp+bias in 0..=191/767/12287; all shifts are below the destination width"
        )]
        #[cfg_attr(verus_keep_ghost, verus_spec(ret =>
            ensures
                bid_canonical(ret as int, <$width as Width>::LAYOUT),
                bid_fields(ret as int, <$width as Width>::LAYOUT, value.view()),
        ))]
        pub(crate) const fn $encode(value: InRange<$width>) -> <$width as Width>::Bits {
            #[cfg(verus_keep_ghost)]
            proof! { use_type_invariant(&value); <$width>::lemma_shifts(); }
            let value = value.value;
            let sign = (value.neg as <$width as Width>::Bits) << (<<$width as Width>::Bits>::BITS - 1);
            let exp = (value.exp + <$width as Width>::LAYOUT.bias) as <$width as Width>::Bits;
            let coeff = value.coeff as <$width as Width>::Bits;
            let small_coeff_bits = <$width as Width>::LAYOUT.trail_bits + 3;
            #[cfg(verus_keep_ghost)]
            proof! {
                <$width>::lemma_encode(
                    value,
                    (<<$width as Width>::Bits>::BITS - 1) as u32,
                    small_coeff_bits,
                    (<$width as Width>::LAYOUT.trail_bits + 1) as u32,
                    (<<$width as Width>::Bits>::BITS - 3) as u32,
                );
            }
            if coeff < 1 << small_coeff_bits {
                sign | (exp << small_coeff_bits) | coeff
            } else {
                let large_coeff_bits = <$width as Width>::LAYOUT.trail_bits + 1;
                sign | (0b11 << (<<$width as Width>::Bits>::BITS - 3))
                    | (exp << large_coeff_bits)
                    | (coeff & ((1 << large_coeff_bits) - 1))
            }
        }
    };
}

bid_layout!(W32, decode_32, unpack_32, encode_32);
bid_layout!(W64, decode_64, unpack_64, encode_64);
bid_layout!(W128, decode_128, unpack_128, encode_128);

const _: () = assert!(10_u128.pow(34) - 1 < (1_u128 << 113));

#[inline]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "only validated canonical Decimal128 storage is passed: coeff < 10^34 < 2^113 excludes the large-coefficient form; the 14-bit exponent field fits i32 and subtracting the bias 6176 cannot overflow"
)]
#[cfg_attr(verus_keep_ghost, verus_spec(ret =>
    requires bid_canonical(bits as int, W128::LAYOUT),
    ensures bid_fields(bits as int, W128::LAYOUT, ret),
))]
pub(crate) fn unpack_canonical_128(bits: u128) -> Unpacked {
    #[cfg(verus_keep_ghost)]
    proof! {
        W128::lemma_shifts();
        reveal(pow);
        assert(pow(10, 34) < pow2(113)) by (compute_only);
        vstd::arithmetic::power2::lemma_pow2_pos(111);
        assert(!bid_large(bits as int, W128::LAYOUT));
    }
    let coeff_bits = W128::LAYOUT.trail_bits + 3;
    let exp_mask = (1_u128 << W128::LAYOUT.exp_bits) - 1;
    #[cfg(verus_keep_ghost)]
    proof! {
        W128::lemma_unpack(
            bits,
            (u128::BITS - 1) as u32,
            coeff_bits,
            (W128::LAYOUT.trail_bits + 1) as u32,
            exp_mask,
        );
    }
    Unpacked {
        neg: bits >> (u128::BITS - 1) != 0,
        coeff: bits & ((1_u128 << coeff_bits) - 1),
        exp: ((bits >> coeff_bits) & exp_mask) as i32 - W128::LAYOUT.bias,
    }
}
