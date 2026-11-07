#!/usr/bin/env python3
"""Recheck stored interval fixtures using independent exact Fraction division.

Standard library only. Does not build or call the Rust implementation.
The historical baseline column is preserved evidence, not regenerated here.
"""
from fractions import Fraction
from pathlib import Path
import math
import struct


def value_from_bits(bits, single=False):
    return struct.unpack('>f' if single else '>d', bits.to_bytes(4 if single else 8, 'big'))[0]


def oracle(bits, layout, single=False):
    # Independent exact rational reference: does not use decode, cache, logs,
    # candidate_finish or the baseline's limb expansion/rounding helpers.
    digits, minimum, maximum = layout
    signpos = 31 if single else 63
    negative = bool(bits >> signpos)
    x = abs(value_from_bits(bits, single))
    fb = 23 if single else 52
    if math.isnan(x):
        payload = bits & ((1 << (fb - 1)) - 1)
        return 'nan', negative, payload if payload < 10 ** (digits - 1) else 0
    if math.isinf(x):
        return 'infinite', negative, 0
    if x == 0:
        return 'finite', negative, 0, 0
    rational = Fraction.from_float(x)
    n, d = rational.numerator, rational.denominator
    preferred = 1 - d.bit_length()
    a = len(str(n)) - len(str(d))
    power = Fraction(10 ** a) if a >= 0 else Fraction(1, 10 ** -a)
    if rational < power:
        a -= 1
    elif rational >= power * 10:
        a += 1
    p = min(digits, 6 if single else 15)
    exponent = max(a - p + 1, minimum, preferred)
    scaled = rational / (10 ** exponent) if exponent >= 0 else rational * (10 ** -exponent)
    whole, rem = divmod(scaled.numerator, scaled.denominator)
    twice = rem * 2
    rounded = whole + int(twice > scaled.denominator or (twice == scaled.denominator and whole % 2 == 1))
    if rounded >= 10 ** p:
        if rounded != 10 ** p:
            raise ValueError("rounded coefficient exceeds single-digit carry bound")
        rounded //= 10
        exponent += 1
    if rounded == 0:
        return 'finite', negative, 0, 0
    if exponent > maximum:
        if len(str(rounded)) + exponent - maximum > digits:
            return 'infinite', negative, 0
        rounded *= 10 ** (exponent - maximum)
        exponent = maximum
    while exponent < 0 and rounded % 10 == 0:
        rounded //= 10
        exponent += 1
    if not (minimum <= exponent <= maximum and 0 < rounded < 10 ** digits):
        raise ValueError("reference result is outside the destination layout")
    return 'finite', negative, rounded, exponent




def encode(row, width):
    sign = int(row[1]) << (width - 1)
    if row[0] == "infinite":
        return sign | (30 << (width - 6))
    if row[0] == "nan":
        return sign | (31 << (width - 6)) | row[2]
    _, _, coefficient, exponent = row
    bias, small, large = {32: (101, 23, 21), 64: (398, 53, 51),
                          128: (6176, 113, 111)}[width]
    biased = exponent + bias
    if width == 128 or coefficient < 1 << small:
        return sign | (biased << small) | coefficient
    return sign | (3 << (width - 3)) | (biased << large) | (coefficient & ((1 << large) - 1))


def check_fixtures(fixtures):
    """Reject malformed or mismatching data, including under python -O."""
    layouts = {32: (7, -101, 90), 64: (16, -398, 369), 128: (34, -6176, 6111)}
    count = 0
    for number, line in enumerate(fixtures.read_text().splitlines(), 1):
        if not line or line.startswith("#"):
            continue
        fields = line.split("\t")
        if len(fields) != 6 or fields[0] not in {"0", "1"}:
            raise ValueError(f"fixture line {number}: invalid six-column format/source kind")
        single, width, bits, expected, baseline, _ = fields
        width = int(width)
        if width not in layouts:
            raise ValueError(f"fixture line {number}: unsupported decimal width {width}")
        source_bits = int(bits, 16)
        if not 0 <= source_bits < 1 << (32 if single == "1" else 64):
            raise ValueError(f"fixture line {number}: source bits exceed source width")
        actual = encode(oracle(source_bits, layouts[width], single == "1"), width)
        if not actual == int(expected, 16) == int(baseline, 16):
            raise ValueError(f"fixture line {number}: Fraction/expected/baseline mismatch: {line}")
        count += 1
    if count != 402:
        raise ValueError(f"expected 402 fixture rows, found {count}")
    return count


def main():
    fixtures = Path(__file__).resolve().parents[1] / "src" / "interval_fixtures.tsv"
    count = check_fixtures(fixtures)
    print(f"Independent Fraction fixture checks: {count}")


if __name__ == "__main__":
    main()
