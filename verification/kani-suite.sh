#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

harnesses=(
    kernel::proofs::bounded_128_gap_p00_matches_general
    kernel::proofs::bounded_64_gap_m22_matches_general
    kernel::proofs::bounded_64_gap_p00_matches_general
    kernel::proofs::bounded_64_gap_p22_matches_general
    kernel::proofs::cmp_bounded_32_matches_general
    kernel::proofs::digits_64_is_the_decimal_length
    kernel::proofs::digits_is_the_decimal_length
    kernel::proofs::div_rem_pow10_64_k15
    kernel::proofs::div_rem_pow10_64_k16
    kernel::proofs::div_rem_pow10_64_k17
    kernel::proofs::div_rem_pow10_64_k18
    kernel::proofs::div_rem_pow10_64_k19
    kernel::proofs::div_rem_pow10_64_outside_table
    kernel::proofs::div_rem_pow10_k36
    kernel::proofs::div_rem_pow10_k37
    kernel::proofs::div_rem_pow10_k38
    kernel::proofs::div_rem_pow10_outside_table
    kernel::proofs::eq_bounded_32_matches_general
    kernel::proofs::power_tables_are_powers_of_ten
    kernel::proofs::strip_trailing_zeros_64_z15
    kernel::proofs::strip_trailing_zeros_64_zero
    kernel::proofs::zero_sign_follows_exact_zero_rule
    layout_proofs::constants_encode_their_fields
    layout_proofs::decode_128_matches_interchange_format
    layout_proofs::decode_32_matches_interchange_format
    layout_proofs::decode_64_matches_interchange_format
    layout_proofs::encode_128_round_trips_every_in_range_field
    layout_proofs::encode_32_round_trips_every_in_range_field
    layout_proofs::encode_64_round_trips_every_in_range_field
    layout_proofs::in_range_128_accepts_exactly_the_layout_bounds
    layout_proofs::in_range_32_accepts_exactly_the_layout_bounds
    layout_proofs::in_range_64_accepts_exactly_the_layout_bounds
    layout_proofs::public_storage_128_matches_decode
    layout_proofs::public_storage_32_matches_decode
    layout_proofs::public_storage_64_matches_decode
    layout_proofs::unpack_canonical_128_matches_decode
    layout_proofs::widen_32_to_128_and_narrow_back
    layout_proofs::widen_32_to_64_and_narrow_back
    layout_proofs::widen_64_to_128_and_narrow_back
    parse::proofs::load_short_len0
    parse::proofs::load_short_len1
    parse::proofs::load_short_len2
    parse::proofs::load_short_len3
    parse::proofs::load_short_len4
    parse::proofs::load_short_len5
    parse::proofs::load_short_len6
    parse::proofs::load_short_len7
    parse::proofs::load_short_len8
    parse::proofs::parse_32_fast_path_len0
    parse::proofs::parse_32_fast_path_len1
    parse::proofs::parse_32_fast_path_len2
    parse::proofs::parse_32_fast_path_len3
    parse::proofs::parse_32_fast_path_len4
    parse::proofs::parse_32_fast_path_len5
    parse::proofs::parse_32_fast_path_len6
    parse::proofs::parse_32_fast_path_len7
    parse::proofs::parse_32_fast_path_len8
    parse::proofs::parse_32_fast_path_len9
    parse::proofs::parse_64_fast_path_len8
    parse::proofs::parse_eight_reads_eight_ascii_digits
    parse::proofs::point_lane_constant
    parse::proofs::zero_lanes_marks_exactly_the_zero_bytes
)

args=()
for harness in "${harnesses[@]}"; do
    args+=(--harness "$harness")
done

log=$(mktemp)
trap 'rm -f "$log"' EXIT

status=0
cargo kani -Z stubbing -Z unstable-options \
    --harness-timeout 30m -j 3 --output-format=terse \
    --exact "${args[@]}" 2>&1 | tee "$log" || status=$?

expected="Complete - ${#harnesses[@]} successfully verified harnesses, 0 failures, ${#harnesses[@]} total."
if [[ $status -ne 0 ]] || ! grep -qxF "$expected" "$log"; then
    echo "Kani suite failed: expected \"$expected\"" >&2
    exit 1
fi
echo "Kani suite passed: ${#harnesses[@]} harnesses"
