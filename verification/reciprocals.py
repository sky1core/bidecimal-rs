from pathlib import Path
import subprocess

import z3


ROOT = Path(__file__).resolve().parents[1]
WIDTHS = ((64, 19), (128, 38))
TIMEOUT_MS = 15_000


def production_tables():
    output = ROOT / "target" / "reciprocal-verification"
    output.mkdir(parents=True, exist_ok=True)
    binary = output / "export-tables"
    subprocess.run(
        [
            "rustc",
            "--edition=2024",
            str(ROOT / "verification" / "export_tables.rs"),
            "-o",
            str(binary),
        ],
        cwd=ROOT,
        check=True,
    )
    exported = subprocess.check_output([str(binary)], text=True)
    entries = {}
    for line in exported.splitlines():
        kind, width, exponent, *values = line.split(",")
        key = (kind, int(width), int(exponent))
        if key in entries:
            raise RuntimeError(f"duplicate production table entry: {key}")
        entries[key] = tuple(map(int, values))
    expected = {
        (kind, bits, k)
        for bits, max_k in WIDTHS
        for kind, first in (("power", 0), ("reciprocal", 1))
        for k in range(first, max_k + 1)
    }
    if entries.keys() != expected:
        raise RuntimeError("missing or unexpected production table entries")
    for bits, max_k in WIDTHS:
        for k in range(max_k + 1):
            if entries["power", bits, k] != (10**k,):
                raise RuntimeError(f"incorrect power constant: {bits}, {k}")
        for k in range(1, max_k + 1):
            m, post = entries["reciprocal", bits, k]
            divisor = 10**k
            shift = bits - 1 + divisor.bit_length()
            if not (
                0 < m < 1 << bits
                and post == divisor.bit_length() - 1
                and m == ((1 << shift) + divisor - 1) // divisor
                and 0 <= m * divisor - (1 << shift) < divisor
            ):
                raise RuntimeError(f"incorrect reciprocal constant: {bits}, {k}")
    return entries


def prove(name, premises, conclusion):
    solver = z3.Solver()
    solver.set(timeout=TIMEOUT_MS)
    solver.add(*premises, z3.Not(conclusion))
    result = solver.check()
    if result == z3.sat:
        raise RuntimeError(f"{name}: counterexample: {solver.model()}")
    if result != z3.unsat:
        raise RuntimeError(f"{name}: {result}: {solver.reason_unknown()}")
    print(f"proved {name}", flush=True)


def reciprocal(bits, k, m, post):
    divisor = 10**k
    scale = 1 << (bits + post)
    x, quotient, remainder = z3.Ints(f"x_{bits}_{k} q_{bits}_{k} r_{bits}_{k}")
    prove(
        f"reciprocal_{bits}_k{k:02}",
        [
            0 <= x,
            x < 1 << (bits - 1),
            0 <= quotient,
            0 <= remainder,
            remainder < divisor,
            x == quotient * divisor + remainder,
        ],
        z3.And(
            quotient * scale <= x * m,
            x * m < (quotient + 1) * scale,
            0 <= x * m,
            x * m < 1 << (2 * bits),
            0 <= quotient * divisor,
            quotient * divisor <= x,
            x - quotient * divisor == remainder,
        ),
    )
    product = z3.Int(f"product_{bits}_{k}")
    prove(
        f"high_half_then_shift_{bits}_k{k:02}",
        [0 <= product, product < 1 << (2 * bits)],
        (product / (1 << bits)) / (1 << post) == product / scale,
    )


def main():
    if z3.get_version() != (4, 15, 3, 0):
        raise RuntimeError(f"expected Z3 4.15.3.0, found {z3.get_full_version()}")
    entries = production_tables()
    count = 0
    for bits, max_k in WIDTHS:
        if not 1 << (bits - 1) < 10 ** (max_k + 1):
            raise RuntimeError(f"incorrect large-power boundary: {bits}")
        for k in range(1, max_k + 1):
            reciprocal(bits, k, *entries["reciprocal", bits, k])
            count += 2
    print(f"{count} integer-model obligations proved; {len(entries)} production entries checked")


if __name__ == "__main__":
    main()
