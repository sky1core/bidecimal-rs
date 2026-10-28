import json
import re
import sys
from pathlib import Path

STD_SPECS = "src/verus_std.rs"
TRUSTED_ROOTS = ("core::", "std::")
TERMINATION_FLAGS = ("assume_termination", "exec_allows_no_decreases_clause", "exec_assume_termination")
CHEATING = "error: external_body/assume_specification not allowed with --no-cheating"


def fail(message):
    print(f"Verus suite failed: {message}", file=sys.stderr)
    sys.exit(1)


def recorded(path):
    return {line for line in Path(path).read_text().splitlines() if line}


def compare(kind, actual, expected):
    if actual != expected:
        fail(f"{kind} differ from the recorded list: missing {sorted(expected - actual)}, unexpected {sorted(actual - expected)}")


def check_run(work, version, expected, status):
    if status != 0:
        fail(f"the verification run exited with status {status}")
    report = json.loads((work / "result.json").read_text())
    if report["verus"]["version"] != version:
        fail(f"Verus reported version {report['verus']['version']}, expected {version}")
    wanted = {
        "success": True,
        "verified": expected,
        "errors": 0,
        "encountered-error": False,
        "encountered-vir-error": False,
        "is-verifying-entire-crate": True,
    }
    results = report["verification-results"]
    for key, value in wanted.items():
        if results.get(key) != value:
            fail(f"verification result {key} is {results.get(key)!r}, expected {value!r}")
    diagnostics = [line for line in (work / "run.txt").read_text().splitlines() if line.startswith(("warning", "error"))]
    if diagnostics:
        fail("the verification run produced diagnostics: " + "; ".join(diagnostics))
    return {name for name in report["func-details"] if name.startswith("bidecimal::")}


def check_vir(work):
    vir = (work / "log" / "crate-simple.vir").read_text()
    for flag in TERMINATION_FLAGS:
        if f":{flag} false" not in vir:
            fail(f"the VIR log has no {flag} field")
        if re.search(rf":{flag} (?!false\b)", vir):
            fail(f"a function or loop sets {flag}")
    trusted = []
    for target, location, proxy in re.findall(r':name \(Fun :path (\S+)\) :proxy \(@ "([^"]+)" (\S+)\)', vir):
        if not proxy.startswith("bidecimal::"):
            continue
        line = re.fullmatch(rf"{re.escape(STD_SPECS)}:(\d+):1: \d+:\d+ \(#\d+\)", location)
        if not proxy.startswith("bidecimal::verus_std::") or line is None:
            fail(f"trusted specification {proxy} at {location} is outside {STD_SPECS}")
        if not target.startswith(TRUSTED_ROOTS):
            fail(f"the trusted specification at {location} targets {target}, which is not in core or std")
        trusted.append((target, int(line.group(1))))
    crate_proxies = re.findall(r':proxy \(@ "[^"]*"\s+bidecimal::', vir)
    if len(crate_proxies) != len(trusted):
        fail("the crate declares a trusted type, trait or other item besides the std function specifications")
    crate_types = re.findall(r"\(Datatype :name \(Dt Path bidecimal::\S+\) .*?:transparency \(DatatypeTransparency (\w+)", vir, re.S)
    if not crate_types:
        fail("the VIR log lists no crate type")
    if "Never" in crate_types:
        fail("a crate type is marked external_body")
    return trusted


def check_no_cheating(work, status, trusted_lines):
    if status != 1:
        fail(f"the --no-cheating run exited with status {status}, expected 1")
    log = [line for line in (work / "cheat.txt").read_text().splitlines() if line.strip()]
    errors = [i for i, line in enumerate(log) if not line[0].isspace() and not line[0].isdigit()]
    if not errors or errors[-1] != len(log) - 1:
        fail("the --no-cheating run did not end with the abort line")
    if not re.fullmatch(rf"error: aborting due to {len(errors) - 1} previous errors?", log[-1]):
        fail("unexpected last line of the --no-cheating run: " + log[-1])
    rejected = []
    for i in errors[:-1]:
        location = re.fullmatch(rf" *--> {re.escape(STD_SPECS)}:(\d+):1", log[i + 1])
        if log[i] != CHEATING or location is None:
            fail("unexpected --no-cheating diagnostic: " + " ".join(log[i : i + 2]))
        rejected.append(int(location.group(1)))
    if sorted(rejected) != sorted(trusted_lines):
        fail(f"--no-cheating rejected {STD_SPECS} lines {sorted(rejected)}, expected the trusted specifications at {sorted(trusted_lines)}")


def main():
    work, version, expected, status, cheat_status, functions, trusted = sys.argv[1:]
    work = Path(work)
    verified = check_run(work, version, int(expected), int(status))
    compare("verified crate functions", verified, recorded(functions))
    specs = check_vir(work)
    if len(specs) != len({target for target, _ in specs}):
        fail("a standard-library function has more than one trusted specification")
    compare("trusted specifications", {target for target, _ in specs}, recorded(trusted))
    check_no_cheating(work, int(cheat_status), [line for _, line in specs])
    print(f"Verus suite passed: {expected} items verified in {len(verified)} crate functions; trusted: {len(specs)} std specifications")


if __name__ == "__main__":
    main()
