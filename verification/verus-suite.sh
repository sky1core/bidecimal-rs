#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

VERUS_VERSION=0.2026.10.18.7d82994
# Selected-operand ordering adds six proof items; one-sided equality adds two.
# Removing the widened add window uses two explicit nonlinear identity proofs.
# Removing its two unused bit-bound lemmas removes six verification items.
EXPECTED_VERIFIED=386

functions=(
    bidecimal::error::Error::clone
    bidecimal::kernel::add_sub
    bidecimal::kernel::add_sub_general
    bidecimal::kernel::canonical
    bidecimal::kernel::clamped_magnitudes_64
    bidecimal::kernel::cmp_64
    bidecimal::kernel::cmp_128
    bidecimal::kernel::cmp_wide_gap_128
    bidecimal::kernel::digits
    bidecimal::kernel::digits_64
    bidecimal::kernel::div_rem_pow10
    bidecimal::kernel::div_rem_pow10_64
    bidecimal::kernel::div_rem_scaled
    bidecimal::kernel::eq_64
    bidecimal::kernel::exceeds_max
    bidecimal::kernel::fit
    bidecimal::kernel::is_largest_finite
    bidecimal::kernel::scaled_is_multiple
    bidecimal::kernel::scaled_rem
    bidecimal::kernel::strip_trailing_zeros
    bidecimal::kernel::strip_trailing_zeros_64
    bidecimal::kernel::zero_sign
    bidecimal::layout::InRange::checked
    bidecimal::layout::InRange::inv
    bidecimal::layout::InRange::unchecked
    bidecimal::layout::InRange::unpacked
    bidecimal::layout::InRange::view
    bidecimal::layout::Unpacked::clone
    bidecimal::layout::W128::lemma_encode
    bidecimal::layout::W128::lemma_shifts
    bidecimal::layout::W128::lemma_unpack
    bidecimal::layout::W32::lemma_encode
    bidecimal::layout::W32::lemma_shifts
    bidecimal::layout::W32::lemma_unpack
    bidecimal::layout::W64::lemma_encode
    bidecimal::layout::W64::lemma_shifts
    bidecimal::layout::W64::lemma_unpack
    bidecimal::layout::Width::LAYOUT
    bidecimal::layout::bid_canonical
    bidecimal::layout::bid_coeff
    bidecimal::layout::bid_exp
    bidecimal::layout::bid_fields
    bidecimal::layout::bid_finite
    bidecimal::layout::bid_large
    bidecimal::layout::bid_sign
    bidecimal::layout::bid_width
    bidecimal::layout::decode_128
    bidecimal::layout::decode_32
    bidecimal::layout::decode_64
    bidecimal::layout::encode_128
    bidecimal::layout::encode_32
    bidecimal::layout::encode_64
    bidecimal::layout::field
    'bidecimal::layout::impl&%3::LAYOUT'
    'bidecimal::layout::impl&%4::LAYOUT'
    'bidecimal::layout::impl&%5::LAYOUT'
    bidecimal::layout::in_range
    bidecimal::layout::lemma_layout_values
    bidecimal::layout::unpack_128
    bidecimal::layout::unpack_32
    bidecimal::layout::unpack_64
    bidecimal::layout::unpack_canonical_128
    bidecimal::narrow::Finite64::clone
    bidecimal::narrow::Finite64::view
    bidecimal::narrow::add_sub_general
    bidecimal::narrow::aligned
    bidecimal::narrow::compressed
    bidecimal::narrow::fit
    bidecimal::rounding::Rounding::clone
    bidecimal::tables::POW10
    bidecimal::tables::POW10_64
    bidecimal::tables::RECIP
    bidecimal::tables::RECIP_64
    bidecimal::tables::Recip64::clone
    bidecimal::tables::Recip::clone
    bidecimal::tables::build_pow10
    bidecimal::tables::build_pow10_64
    bidecimal::tables::build_reciprocal
    bidecimal::tables::build_reciprocal_64
    bidecimal::tables::build_reciprocals
    bidecimal::tables::build_reciprocals_64
    bidecimal::tables::const_mul_128_to_256
    bidecimal::tables::fill_pow10
    bidecimal::tables::fill_pow10_64
    bidecimal::tables::fill_reciprocals
    bidecimal::tables::fill_reciprocals_64
    bidecimal::tables::floor_log2_64
    bidecimal::tables::long_divide
    bidecimal::tables::pow10
    bidecimal::tables::pow10_64
    bidecimal::tables::recip
    bidecimal::tables::recip_64
    bidecimal::tables::verify_reciprocals
    bidecimal::tables::verify_reciprocals_64
    bidecimal::tables::verify_reciprocals_64_from
    bidecimal::tables::verify_reciprocals_from
    bidecimal::verus_model::abs_int
    bidecimal::verus_model::ceil_result
    bidecimal::verus_model::clamp_exp
    bidecimal::verus_model::closest_exp
    bidecimal::verus_model::comparison_result
    bidecimal::verus_model::comparison_value
    bidecimal::verus_model::decimal_digits
    bidecimal::verus_model::digits_estimate_ok
    bidecimal::verus_model::digits_estimates_ok
    bidecimal::verus_model::exact_result
    bidecimal::verus_model::exact_sum
    bidecimal::verus_model::fit_layout
    bidecimal::verus_model::fit_split
    bidecimal::verus_model::fits
    bidecimal::verus_model::fits_above
    bidecimal::verus_model::floor_result
    bidecimal::verus_model::is_ceil
    bidecimal::verus_model::is_floor
    bidecimal::verus_model::least_exp
    bidecimal::verus_model::lemma_add_compress_bounds
    bidecimal::verus_model::lemma_add_compress_model
    bidecimal::verus_model::lemma_add_compress_scale
    bidecimal::verus_model::lemma_add_digit_scale
    bidecimal::verus_model::lemma_add_exact_model
    bidecimal::verus_model::lemma_add_exact_scale
    bidecimal::verus_model::lemma_add_exact_sum
    bidecimal::verus_model::lemma_add_fast_scale
    bidecimal::verus_model::lemma_add_fit_transfer
    bidecimal::verus_model::lemma_add_sum_bound
    bidecimal::verus_model::lemma_add_input
    bidecimal::verus_model::lemma_add_signed_product
    bidecimal::verus_model::lemma_add_signed_scale
    bidecimal::verus_model::lemma_add_strip
    bidecimal::verus_model::lemma_below_max
    bidecimal::verus_model::lemma_bit_width_is_post
    bidecimal::verus_model::lemma_cancel_pow10
    bidecimal::verus_model::lemma_cancel_ten
    bidecimal::verus_model::lemma_ceil_fits
    bidecimal::verus_model::lemma_comparison_adjusted
    bidecimal::verus_model::lemma_comparison_align_128
    bidecimal::verus_model::lemma_comparison_clamp_64
    bidecimal::verus_model::lemma_comparison_dominance_64
    bidecimal::verus_model::lemma_comparison_scale_64
    bidecimal::verus_model::lemma_comparison_signs
    bidecimal::verus_model::lemma_decimal_digits
    bidecimal::verus_model::lemma_decimal_digits_le
    bidecimal::verus_model::lemma_digits_estimate
    bidecimal::verus_model::lemma_digits_estimates_below
    bidecimal::verus_model::lemma_digits_result
    bidecimal::verus_model::lemma_div_rem_pow10_64_edges
    bidecimal::verus_model::lemma_div_rem_pow10_64_main
    bidecimal::verus_model::lemma_div_rem_pow10_edges
    bidecimal::verus_model::lemma_div_rem_pow10_main
    bidecimal::verus_model::lemma_div_scaled_end
    bidecimal::verus_model::lemma_div_scaled_start
    bidecimal::verus_model::lemma_div_scaled_step
    bidecimal::verus_model::lemma_double_power
    bidecimal::verus_model::lemma_exact_bounds
    bidecimal::verus_model::lemma_fit_between
    bidecimal::verus_model::lemma_fit_down
    bidecimal::verus_model::lemma_fit_exact
    bidecimal::verus_model::lemma_fit_none
    bidecimal::verus_model::lemma_fit_scale
    bidecimal::verus_model::lemma_fit_split
    bidecimal::verus_model::lemma_fit_up
    bidecimal::verus_model::lemma_fit_zero
    bidecimal::verus_model::lemma_gap
    bidecimal::verus_model::lemma_long_divide_halves
    bidecimal::verus_model::lemma_long_divide_step
    bidecimal::verus_model::lemma_mul_below
    bidecimal::verus_model::lemma_mul_limbs
    bidecimal::verus_model::lemma_narrow_align_scale
    bidecimal::verus_model::lemma_narrow_cohort_order
    bidecimal::verus_model::lemma_narrow_compress_bounds
    bidecimal::verus_model::lemma_narrow_compress_model
    bidecimal::verus_model::lemma_narrow_compress_scale
    bidecimal::verus_model::lemma_narrow_finite_bound
    bidecimal::verus_model::lemma_narrow_fit_split
    bidecimal::verus_model::lemma_narrow_ok_excludes_error
    bidecimal::verus_model::lemma_narrow_rounded_deterministic
    bidecimal::verus_model::lemma_narrow_rounded_unique
    bidecimal::verus_model::lemma_odd_part_start
    bidecimal::verus_model::lemma_odd_part_step
    bidecimal::verus_model::lemma_overflow_low
    bidecimal::verus_model::lemma_overflow_max
    bidecimal::verus_model::lemma_pow10_38
    bidecimal::verus_model::lemma_pow10_bounds
    bidecimal::verus_model::lemma_pow10_facts
    bidecimal::verus_model::lemma_pow10_facts_128
    bidecimal::verus_model::lemma_pow10_step
    bidecimal::verus_model::lemma_pow2_128
    bidecimal::verus_model::lemma_pow2_above_coefficients
    bidecimal::verus_model::lemma_pow2_below_pow5
    bidecimal::verus_model::lemma_pow2_not_multiple_of_5
    bidecimal::verus_model::lemma_reciprocal_64_entry
    bidecimal::verus_model::lemma_reciprocal_entry
    bidecimal::verus_model::lemma_reciprocal_exact
    bidecimal::verus_model::lemma_reciprocal_start
    bidecimal::verus_model::lemma_rounded_transfer
    bidecimal::verus_model::lemma_same_side_eq
    bidecimal::verus_model::lemma_same_side_gap
    bidecimal::verus_model::lemma_scaled_base
    bidecimal::verus_model::lemma_scaled_is_multiple
    bidecimal::verus_model::lemma_scaled_is_multiple_bounds
    bidecimal::verus_model::lemma_scaled_rem_direct
    bidecimal::verus_model::lemma_scaled_rem_start
    bidecimal::verus_model::lemma_scaled_rem_step
    bidecimal::verus_model::lemma_scaled_same
    bidecimal::verus_model::lemma_scaled_total
    bidecimal::verus_model::lemma_scaled_trans
    bidecimal::verus_model::lemma_strip_end
    bidecimal::verus_model::lemma_strip_end_64
    bidecimal::verus_model::lemma_strip_start
    bidecimal::verus_model::lemma_strip_start_64
    bidecimal::verus_model::lemma_strip_step
    bidecimal::verus_model::lemma_u128_join
    bidecimal::verus_model::lemma_u128_limbs
    bidecimal::verus_model::lemma_u128_shl_one
    bidecimal::verus_model::lemma_verify_reciprocal
    bidecimal::verus_model::lemma_verify_reciprocal_64
    bidecimal::verus_model::min_exp
    bidecimal::verus_model::odd_shift
    bidecimal::verus_model::recip64_spec
    bidecimal::verus_model::recip_spec
    bidecimal::verus_model::rounded
    bidecimal::verus_model::rounded_up
    bidecimal::verus_model::same_order
    bidecimal::verus_model::same_result
    bidecimal::verus_model::same_side
    bidecimal::verus_model::scaled_eq
    bidecimal::verus_model::scaled_le
    bidecimal::verus_model::signed_at
    bidecimal::verus_model::strip_scale
    bidecimal::verus_model::strip_step_ok
    bidecimal::verus_model::sum_sign
)

trusted=(
    'core::convert::num::impl&%50::from'
    'core::num::impl&%10::bit_width'
    'core::num::impl&%10::carrying_mul'
    'core::num::impl&%10::trailing_zeros'
    'core::num::impl&%4::unsigned_abs'
    'core::num::impl&%9::bit_width'
    'core::num::impl&%9::carrying_mul'
)

verus="target/verus/$VERUS_VERSION/verus"
if [[ ! -x "$verus" ]]; then
    echo "Verus $VERUS_VERSION not found at $verus: run verification/verus-setup.sh first" >&2
    exit 1
fi

version=$("$verus" --version 2>&1 || true)
if ! grep -qE "^ *Version: ${VERUS_VERSION//./\\.}\$" <<<"$version"; then
    echo "unexpected Verus version, expected $VERUS_VERSION:" >&2
    echo "$version" >&2
    exit 1
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
printf '%s\n' "${functions[@]}" >"$work/functions.txt"
printf '%s\n' "${trusted[@]}" >"$work/trusted.txt"

crate=(src/lib.rs --crate-type=lib --crate-name bidecimal --edition 2024 --num-threads 2)
status=0
"$verus" "${crate[@]}" --output-json --log-dir "$work/log" --log vir-simple >"$work/result.json" 2>"$work/run.txt" || status=$?
cheat_status=0
"$verus" "${crate[@]}" --no-cheating >"$work/cheat.txt" 2>&1 || cheat_status=$?

python3 -I verification/verus_check.py "$work" "$VERUS_VERSION" "$EXPECTED_VERIFIED" "$status" "$cheat_status" \
    "$work/functions.txt" "$work/trusted.txt"
