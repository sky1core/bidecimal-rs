#!/usr/bin/env python3
"""Compares the benchmark times or instruction counts of two commits on this machine.

Both commits are built with the manifest, lock file, toolchain file, and
benchmark code of one harness commit and the `src/` of the commit being
measured; nothing else of either commit is copied, so the benchmark code,
manifest, lock file, and toolchain are the same for both. Each Criterion
target runs once per side and round, base first in even rounds and candidate
first in odd rounds, and every run is saved as its own Criterion baseline. The
report gives each benchmark's median time and range per side and the
candidate/base ratio, the median over rounds of the ratio of the two runs of
each round, taken on a log scale, largest change first. Whether a ratio
exceeds noise is judged against an A/A comparison (the same commit as base and
candidate) on the same machine. A target whose benchmark code does not compile
against the base library, for example one that measures an operation the base
does not have, is reported as not comparable; its candidate results are still
checked.

With `--analyze-codegen`, the disassembly of each compared Criterion
executable, an x86-64 or AArch64 ELF executable, is reduced to the benchmark
functions of its crate and the functions of bidecimal, each within its
symbol's size, and each function's instruction count, encoded size, calls and
jumps to the 128-bit division helpers, other rip-relative references to them
on x86-64, and indirect calls and jumps whose target is not resolved are
compared; the complete and the reduced disassembly of both sides are kept.

With `--filter`, only the Criterion benchmarks whose IDs match the regular
expression are listed and timed; every selected target must have at least one
match. The filter is recorded in the report.

The `instructions` target counts the instructions of each benchmark once per
side under Valgrind Cachegrind through Gungraun, which needs Linux, Valgrind,
and `gungraun-runner` of the version in the lock file.
"""

import argparse
import gzip
import hashlib
import io
import json
import math
import os
import platform
import re
import shutil
import statistics
import subprocess
import sys
import tarfile
import tomllib
from pathlib import Path

TIMED = ("finite", "keys", "special", "ops")
COUNTED = "instructions"
COUNTED_CARGO = ["--profile", "bench-instructions", "--package", "bidecimal-instructions", "--bench", COUNTED]
SIDES = ("base", "candidate")
BENCHMARK_SOURCES = ("benches", "bench-instructions")
HARNESS = ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml", *BENCHMARK_SOURCES)
COMPILER_OVERRIDES = (
    "CARGO_BUILD_RUSTC",
    "RUSTC_WRAPPER",
    "CARGO_BUILD_RUSTC_WRAPPER",
    "RUSTC_WORKSPACE_WRAPPER",
    "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
)
ALLOWED_CARGO_VARIABLES = {
    "CARGO_HOME", "CARGO_TARGET_DIR", "CARGO_BUILD_TARGET_DIR", "CARGO_BUILD_BUILD_DIR",
    "CARGO_LOG", "CARGO_CACHE_RUSTC_INFO", "CARGO_CACHE_AUTO_CLEAN_FREQUENCY",
    "CARGO_FUTURE_INCOMPAT_REPORT_FREQUENCY",
}
ALLOWED_CARGO_PREFIXES = (
    "CARGO_TERM_", "CARGO_HTTP_", "CARGO_NET_", "CARGO_ALIAS_", "CARGO_CREDENTIAL_ALIAS_", "CARGO_REGISTRY_",
    "CARGO_REGISTRIES_",
)
ALLOWED_CONFIG_TABLES = {
    "cache", "future-incompat-report", "http", "net", "term", "alias", "credential-alias", "registry", "registries",
}
ALLOWED_CONFIG_BUILD_KEYS = {"build-dir", "target-dir"}
BUILD_OVERRIDES = {
    *COMPILER_OVERRIDES, "CARGO", "RUSTFLAGS", "RUSTDOCFLAGS", "RUSTDOC", "RUSTC_BOOTSTRAP",
}
API_MISMATCH = {
    "E0034", "E0061", "E0063", "E0107", "E0277", "E0282", "E0283", "E0308", "E0369", "E0412", "E0423", "E0425",
    "E0432", "E0433", "E0560", "E0599", "E0600", "E0603", "E0609", "E0616", "E0618", "E0624",
}
HELPERS = {"__udivti3", "__umodti3", "__divti3", "__modti3"}
MACHINES = {62: "x86-64", 183: "AArch64"}
SYMBOL = re.compile(r"^([0-9a-f]+) ([0-9a-f]+) [tTW] (\S+)$")
RELOCATION = re.compile(
    r"^([0-9a-f]+) R_(?:X86_64|AARCH64)_(?:RELATIVE\s+\*ABS\*\+0x([0-9a-f]+)|(GLOB_DAT|JUMP_SLOT)\s+([\w.$]+)(?:@@?[\w.]+)?)$"
)
FUNCTION = re.compile(r"^([0-9a-f]+) <(.+)>:$")
SEGMENT = re.compile(r"^\s*(\S+) off\s+0x[0-9a-f]+ vaddr 0x([0-9a-f]+) ")
SEGMENT_SIZE = re.compile(r"^\s*filesz 0x[0-9a-f]+ memsz 0x([0-9a-f]+) ")
INSTRUCTION = re.compile(r"^\s*([0-9a-f]+): ([0-9a-f ]+?)\s*\t(.*)$")
DIRECT = re.compile(r"0x([0-9a-f]+)(?: <(.+)>)?$")
AARCH64_INDIRECT = re.compile(r"bl?r(?:aa|ab)?z?")
X86_PREFIXES = ("lock", "rep", "repe", "repne", "repz", "repnz", "notrack", "bnd", "addr32", "data16")


def fail(message):
    sys.exit(f"bench-ab: {message}")


def output(command, cwd=None, env=None):
    result = subprocess.run(command, cwd=cwd, env=env, capture_output=True, text=True)
    if result.returncode != 0:
        fail(f"{' '.join(map(str, command))} failed:\n{result.stderr}")
    return result.stdout.strip()


def check_build_environment(working_directories):
    """Keep the manifest profile reported below equal to the effective build.

    RUSTC remains an explicit compiler selector; its resolved version is reported.
    Empty override variables are rejected as well as nonempty ones.
    """
    rejected = sorted(
        name for name in os.environ
        if name in BUILD_OVERRIDES or (
            name.startswith("CARGO_") and name not in ALLOWED_CARGO_VARIABLES
            and not name.startswith(ALLOWED_CARGO_PREFIXES)
        )
    )
    if rejected:
        fail(f"unset {', '.join(rejected)}: build overrides would invalidate the reported profile")
    configs = set()
    for directory in working_directories:
        directory = Path(directory).absolute()
        # Check both lexical and resolved ancestors (including symlink paths).
        for root in (directory, directory.resolve()):
            for ancestor in (root, *root.parents):
                configs.update(ancestor / ".cargo" / name for name in ("config", "config.toml"))
        home = os.environ.get("CARGO_HOME")
        cargo_home = directory / home if home else Path.home() / ".cargo"
        configs.update(cargo_home / name for name in ("config", "config.toml"))
    for config in sorted(configs):
        try:
            settings = tomllib.loads(config.read_text())
        except FileNotFoundError:
            continue
        except (OSError, tomllib.TOMLDecodeError) as error:
            fail(f"cannot check Cargo configuration {config}: {error}")
        rejected = sorted(
            [name for name in settings if name not in ALLOWED_CONFIG_TABLES | {"build"}]
            + [f"build.{name}" for name in settings.get("build", {}) if name not in ALLOWED_CONFIG_BUILD_KEYS]
        )
        if rejected:
            fail(f"Cargo configuration {config} sets {', '.join(rejected)}: "
                 "build settings would invalidate the reported profile")


def commit(rev):
    return output(["git", "rev-parse", "--verify", f"{rev}^{{commit}}"])


def extract(rev, tree, paths):
    """Writes `paths` of `rev` into `tree`."""
    archive = subprocess.run(["git", "archive", "--format=tar", rev, *paths], capture_output=True)
    if archive.returncode != 0:
        fail(f"git archive {rev} {' '.join(paths)} failed:\n{archive.stderr.decode()}")
    with tarfile.open(fileobj=io.BytesIO(archive.stdout)) as tar:
        tar.extractall(tree, filter="data")


def in_benchmark_code(diagnostic, tree):
    """Whether every primary span of `diagnostic` lies in the benchmark
    sources. An error in the body of a generic library function can surface
    only when a benchmark instantiates it, and then lies in the library."""
    spans = [span for span in diagnostic["spans"] if span["is_primary"]]
    return bool(spans) and all(
        any((tree / span["file_name"]).resolve().is_relative_to(tree / source) for source in BENCHMARK_SOURCES)
        for span in spans
    )


def profile_settings(manifest, name):
    """The settings of profile `name` and of the profiles it inherits, as
    written in `manifest`, so that a report shows what the profile name meant
    in the harness commit."""
    profiles = tomllib.loads(manifest.read_text()).get("profile", {})
    if name not in profiles:
        return f"{name} (not defined in Cargo.toml; Cargo's built-in settings)"
    chain = []
    while name in profiles:
        settings = profiles[name]
        chain.append(f"{name} ({', '.join(f'{key}={json.dumps(value)}' for key, value in settings.items())})")
        name = settings.get("inherits")
    return "; ".join(chain)


def build(tree, target, profile, jobs, compiler, log):
    """Builds `target` in `tree` with `compiler` and returns its executable,
    or None when the library builds and only the benchmark code fails, with
    rustc errors in the benchmark sources of the kinds in `API_MISMATCH`: the
    benchmark uses an item the library lacks or has with another signature or
    visibility. Any other failure exits. The build output goes to `log`."""
    check_build_environment([tree])
    selection = COUNTED_CARGO if target == COUNTED else ["--profile", profile, "--bench", target]
    command = ["cargo", "bench", "--locked", "--no-run", "--message-format=json", "--jobs", str(jobs), *selection]
    env = {
        **os.environ,
        "CARGO_TARGET_DIR": str(tree / "target"),
        "CARGO_BUILD_BUILD_DIR": str(tree / "target"),
        "RUSTC": str(compiler),
        "RUSTC_WRAPPER": "",
        "RUSTC_WORKSPACE_WRAPPER": "",
    }
    result = subprocess.run(command, cwd=tree, env=env, capture_output=True, text=True)
    messages = [json.loads(line) for line in result.stdout.splitlines()]
    diagnostics = [m["message"]["rendered"] for m in messages if m.get("reason") == "compiler-message"]
    log.write_text("".join(diagnostics) + result.stderr)
    if result.returncode != 0:
        errors = [m for m in messages if m.get("reason") == "compiler-message" and m["message"]["level"] == "error"]
        library = any(
            m.get("reason") == "compiler-artifact" and m["target"]["name"] == "bidecimal" and m["target"]["kind"] == ["lib"]
            for m in messages
        )
        if library and errors and all(
            m["target"]["name"] == target
            and (m["message"]["code"] or {}).get("code") in API_MISMATCH
            and in_benchmark_code(m["message"], tree)
            for m in errors
        ):
            return None
        fail(f"{tree}: building {target} failed; see {log}")
    for message in messages:
        if (
            message.get("reason") == "compiler-artifact"
            and message["target"]["kind"] == ["bench"]
            and message["target"]["name"] == target
            and message.get("executable")
        ):
            return message["executable"]
    fail(f"{tree}: no executable for {target}")


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def listed_titles(executable, tree, *arguments, filter=None):
    """The titles of the benchmarks that `executable` defines, or with
    `filter` those that match it: Criterion titles, or `module_path::id` for
    the instruction benchmarks. Listing a Criterion target also runs every
    benchmark's result check."""
    selection = [filter] if filter is not None else []
    lines = output([executable, "--bench", "--list", *arguments, *selection], cwd=tree).splitlines()
    titles = {line.removesuffix(": benchmark") for line in lines if line.endswith(": benchmark")}
    if not titles:
        matching = f" matching --filter {filter!r}" if filter is not None else ""
        fail(f"{executable} lists no benchmarks{matching}")
    return titles


def cpu_model():
    if platform.system() == "Darwin":
        return output(["sysctl", "-n", "machdep.cpu.brand_string"])
    for line in output(["lscpu"], env={**os.environ, "LC_ALL": "C"}).splitlines():
        key, _, value = line.partition(":")
        if key.strip() == "Model name":
            return value.strip()
    return "not reported by lscpu"


def collect(home, titles, baselines):
    """Median time per element (or per iteration without an element
    throughput) of every listed benchmark, for each saved baseline. Each
    baseline must hold exactly the listed titles of its target."""
    results = {}
    for target, expected in titles.items():
        for baseline in baselines:
            found = set()
            for path in sorted((home / target).glob(f"**/{baseline}/benchmark.json")):
                meta = json.loads(path.read_text())
                median = json.loads((path.parent / "estimates.json").read_text())["median"]["point_estimate"]
                elements = (meta.get("throughput") or {}).get("Elements")
                entry = results.setdefault(
                    f"{target}: {meta['full_id']}",
                    {"unit": "ns/element" if elements else "ns/iteration", "elements": elements},
                )
                entry[baseline] = median / elements if elements else median
                found.add(meta["title"])
            if found != expected:
                fail(f"{target} {baseline}: missing {sorted(expected - found)}, unexpected {sorted(found - expected)}")
    return results


def functions(tools, executable, target, full):
    """The functions of the ELF executable `executable` that are benchmark
    code of the crate `target`, outside its shared input and reference
    modules, or code of bidecimal: per demangled name, a list of copies, each
    its disassembly lines within its symbol's size and their counts. The
    complete disassembly is written to `full`, gzip-compressed."""
    with open(executable, "rb") as file:
        header = file.read(20)
    if header[:4] != b"\x7fELF":
        fail(f"{executable} is not ELF; --analyze-codegen needs the symbol sizes of an ELF executable")
    machine = MACHINES.get(int.from_bytes(header[18:20], "little"))
    if machine is None:
        fail(f"{executable}: --analyze-codegen reads x86-64 or AArch64 code only")
    sizes, helpers = {}, set(HELPERS)
    for line in output([tools / "llvm-nm", "--print-size", "--defined-only", executable]).splitlines():
        symbol = SYMBOL.match(line)
        if symbol:
            start, size = int(symbol[1], 16), int(symbol[2], 16)
            if sizes.setdefault(start, size) != size:
                fail(f"{executable}: symbols of different sizes at {symbol[1]}")
            if symbol[3] in HELPERS:
                helpers.add(start)
    segments, segment = [], None
    for line in output([tools / "llvm-objdump", "--private-headers", executable]).splitlines():
        header, size = SEGMENT.match(line), SEGMENT_SIZE.match(line)
        if header:
            segment = (header[1], int(header[2], 16))
        elif size and segment:
            segments.append((segment[0], segment[1], segment[1] + int(size[1], 16)))
            segment = None
    if not any(kind == "LOAD" for kind, _, _ in segments):
        fail(f"{executable}: no loadable segment read from its program headers")
    relro = [(start, end - end % 0x1000) for kind, start, end in segments if kind == "RELRO"]
    slots = {}
    if any(kind == "DYNAMIC" for kind, _, _ in segments):
        for line in output([tools / "llvm-objdump", "--dynamic-reloc", executable]).splitlines():
            relocation = RELOCATION.match(line)
            if relocation:
                offset = int(relocation[1], 16)
                if relocation[3] == "JUMP_SLOT" or any(start <= offset and offset + 8 <= end for start, end in relro):
                    slots[offset] = int(relocation[2], 16) if relocation[2] else relocation[4]
    crate = re.compile(rf"(?<![\w:]){target}::(?!(?:keyed|support|reference)::)")
    text = output([tools / "llvm-objdump", "--disassemble", "--demangle", executable])
    with gzip.open(full, "wt") as file:
        file.write(text + "\n")
    found, lines, end = {}, None, 0
    for line in text.splitlines():
        header = FUNCTION.match(line)
        if header:
            lines = None
            if crate.search(header[2]) or "bidecimal::" in header[2]:
                start = int(header[1], 16)
                if not sizes.get(start):
                    fail(f"{executable}: no symbol size for {header[2]}")
                lines, end = [line], start + sizes[start]
                found.setdefault(header[2], []).append((sizes[start], lines))
        elif lines is not None:
            instruction = INSTRUCTION.match(line)
            if instruction and int(instruction[1], 16) < end:
                lines.append(line)
    if not found:
        fail(f"{executable}: no functions of {target} or bidecimal; are its symbols stripped?")
    return {
        name: [{"lines": lines, **shape(lines, size, machine, slots, helpers)} for size, lines in copies]
        for name, copies in found.items()
    }


def x86_counts(instructions, slots, helpers):
    """The calls and jumps to a helper, the other instructions with a
    rip-relative operand that is a helper's GOT slot or address, and the
    indirect calls and jumps with an unresolved target of one x86-64
    function. A branch through a slot is resolved by the slot's JUMP_SLOT
    relocation, or its RELATIVE or GLOB_DAT relocation in the part of the
    GNU_RELRO segment up to its last 4 KiB page boundary, which the loader
    makes read-only after relocation; a branch through a register or other
    memory is not resolved."""
    helper_branches = helper_references = unresolved_branches = 0
    for instruction in instructions:
        mnemonic, _, operands = instruction[3].partition("\t")
        while mnemonic in X86_PREFIXES and operands:
            mnemonic, _, operands = operands.lstrip("\t").partition("\t")
        commented = re.fullmatch(r"(.*?)\s+# (.*)", operands)
        operands, comment = commented.groups() if commented else (operands, "")
        address = re.match(r"0x([0-9a-f]+)", comment)
        rip = int(address[1], 16) if address and "(%rip)" in operands else None
        if not mnemonic.startswith(("call", "j", "loop")):
            if {rip, slots.get(rip)} & helpers:
                helper_references += 1
        elif operands.startswith("*"):
            target = slots.get(rip)
            if target in helpers:
                helper_branches += 1
            elif target is None:
                unresolved_branches += 1
        else:
            direct = DIRECT.fullmatch(operands)
            if direct and {int(direct[1], 16), direct[2] and direct[2].removesuffix("@plt")} & helpers:
                helper_branches += 1
    return helper_branches, helper_references, unresolved_branches


def aarch64_counts(instructions, helpers):
    """The calls and jumps to a helper, directly or through its PLT entry, and
    the indirect calls and jumps of one AArch64 function, whose targets are
    not resolved."""
    helper_branches = unresolved_branches = 0
    for instruction in instructions:
        mnemonic, _, operands = instruction[3].partition("\t")
        if AARCH64_INDIRECT.fullmatch(mnemonic):
            unresolved_branches += 1
        elif mnemonic in ("bl", "b") or mnemonic.startswith(("b.", "bc.", "cb", "tb")):
            direct = DIRECT.search(operands)
            if direct and {int(direct[1], 16), direct[2] and direct[2].removesuffix("@plt")} & helpers:
                helper_branches += 1
    return helper_branches, unresolved_branches


def shape(lines, size, machine, slots, helpers):
    """The instruction count, encoded size in bytes, calls and jumps to the
    128-bit division helpers, other rip-relative references to them on x86-64
    (None on AArch64, where they are not decoded), and indirect calls and jumps with an
    unresolved target, jump tables included, of one function."""
    instructions = [INSTRUCTION.match(line) for line in lines[1:]]
    encoded = sum(len(instruction[2].replace(" ", "")) // 2 for instruction in instructions)
    if encoded != size:
        fail(f"{encoded} bytes of instructions parsed for {lines[0]}, whose symbol has {size}")
    if machine == "AArch64":
        helper_branches, unresolved_branches = aarch64_counts(instructions, helpers)
        helper_references = None
    else:
        helper_branches, helper_references, unresolved_branches = x86_counts(instructions, slots, helpers)
    return {
        "instructions": len(instructions),
        "bytes": encoded,
        "helper_branches": helper_branches,
        "helper_references": helper_references,
        "unresolved_branches": unresolved_branches,
    }


def change(entry, key):
    """The candidate minus base `key` count of a function; a function absent
    on one side has no code there."""
    counts = [entry[side][key] if entry[side] else 0 for side in SIDES]
    return counts[1] - counts[0]


def total(counts):
    """The sum of counts, or None for counts that are not decoded."""
    counts = list(counts)
    return None if None in counts else sum(counts)


def count_cell(count):
    """A count, or n/a for one that is not decoded for the architecture."""
    return "n/a" if count is None else str(count)


def shape_cell(entry, keys):
    return " / ".join(count_cell(entry[key]) for key in keys) if entry else "absent"


def collect_counts(home, expected):
    """The Cachegrind instruction count of every listed instruction
    benchmark. The run must hold exactly the listed titles."""
    counts, paths = {}, {}
    for path in sorted(home.glob("**/summary.json")):
        summary = json.loads(path.read_text())
        title = f"{summary['module_path']}::{summary['id']}" if "id" in summary else summary["module_path"]
        if title in paths:
            fail(f"{title} has two summaries: {paths[title]} and {path}")
        paths[title] = path
        profiles = [profile for profile in summary["profiles"] if profile["tool"] == "Cachegrind"]
        if len(profiles) != 1:
            fail(f"{path}: {len(profiles)} Cachegrind profiles")
        counts[title] = profiles[0]["data"]["total"]["metrics"]["Ir"]["values"]["new"]
    found = set(counts)
    if found != expected:
        fail(f"{home}: missing {sorted(expected - found)}, unexpected {sorted(found - expected)}")
    return counts


def by_change(items):
    return sorted(items, key=lambda item: -abs(math.log(item[1]["ratio"])))


def cell(runs):
    return f"{statistics.median(runs):.4g} ({min(runs):.4g}–{max(runs):.4g})"


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--base", required=True, help="commit measured as the base")
    parser.add_argument("--candidate", required=True, help="commit measured as the candidate")
    parser.add_argument("--harness", help="commit whose manifest, lock file, toolchain file, and benchmark code build both sides (default: the candidate)")
    parser.add_argument("--rounds", type=int, default=2, help="runs per side (default: 2)")
    parser.add_argument(
        "--bench",
        action="append",
        choices=[*TIMED, COUNTED],
        help=f"benchmark target, repeatable (default: the Criterion targets {', '.join(TIMED)})",
    )
    parser.add_argument(
        "--profile", choices=["bench", "bench-fat-lto"], default="bench",
        help="build profile for timed targets (default: bench); instructions always use bench-instructions",
    )
    parser.add_argument(
        "--filter",
        help="regular expression passed to Criterion: list and time only the matching benchmarks; "
        "every selected target must have a match (not with instructions)",
    )
    parser.add_argument("--jobs", type=int, default=2, help="parallel build jobs (default: 2)")
    parser.add_argument(
        "--analyze-codegen",
        action="store_true",
        help="also compare the disassembly of the compared Criterion executables (needs llvm-tools)",
    )
    parser.add_argument("--work", type=Path, required=True, help="new directory for the two build trees")
    parser.add_argument("--out", type=Path, required=True, help="new directory for the results")
    args = parser.parse_args()
    if args.rounds < 1:
        fail("--rounds must be at least 1")
    if args.jobs < 1:
        fail("--jobs must be at least 1")
    for directory in (args.work, args.out):
        if directory.exists():
            fail(f"{directory} already exists; use a new directory")
    targets = args.bench or list(TIMED)
    if COUNTED in targets and args.filter is not None:
        fail("--filter selects Criterion benchmarks and cannot be used with instructions")
    if COUNTED in targets and args.profile == "bench-fat-lto":
        fail("--profile bench-fat-lto cannot be used with instructions; instructions always use bench-instructions")
    settings = sorted(name for name in os.environ if name.startswith("GUNGRAUN_"))
    if COUNTED in targets and settings:
        fail(f"unset {', '.join(settings)}: they change which instruction benchmarks run and what they count")
    check_build_environment([args.work / "base", args.work / "candidate"])
    program = os.environ.get("RUSTC", "rustc")
    found = shutil.which(program)
    if found is None:
        fail(f"{program} not found")
    revs = {"base": commit(args.base), "candidate": commit(args.candidate)}
    harness = commit(args.harness) if args.harness else revs["candidate"]

    listed = output(["git", "ls-tree", "--name-only", harness]).splitlines()
    harness_paths = [path for path in HARNESS if path in listed]

    logs = args.out / "logs"
    logs.mkdir(parents=True)
    trees = {}
    for side in SIDES:
        tree = (args.work / side).resolve()
        tree.mkdir(parents=True)
        extract(harness, tree, harness_paths)
        extract(revs[side], tree, ["src"])
        trees[side] = tree
    sysroot = Path(output([os.path.abspath(found), "--print", "sysroot"], cwd=trees["base"]))
    compiler = sysroot / "bin" / "rustc"
    if not compiler.is_file():
        fail(f"{compiler} not found: the sysroot of {program} has no rustc")
    executables = {side: {} for side in SIDES}
    not_comparable = []
    for target in targets:
        for side in ("candidate", "base"):
            log = logs / f"build-{side}-{target}.log"
            executable = build(trees[side], target, args.profile, args.jobs, compiler, log)
            if executable is None and side == "candidate":
                fail(f"the candidate does not build {target}; see {log.name}")
            if executable is None:
                not_comparable.append(target)
            else:
                executables[side][target] = executable
    compared = [target for target in targets if target not in not_comparable]

    timed = [target for target in compared if target in TIMED]
    counted = [side for side in SIDES if COUNTED in targets and (side == "candidate" or COUNTED in compared)]

    titles = {}
    for target in targets:
        arguments = ("--format", "terse") if target == COUNTED else ()
        sides = SIDES if target in compared else ("candidate",)
        listed = {
            side: listed_titles(executables[side][target], trees[side], *arguments, filter=args.filter)
            for side in sides
        }
        if target in compared and listed["base"] != listed["candidate"]:
            fail(f"{target}: base and candidate list different benchmarks")
        titles[target] = listed["candidate"]

    load = os.getloadavg()
    rustc = dict(
        line.split(": ", 1) for line in output([compiler, "-vV"], cwd=trees["base"]).splitlines()[1:] if ": " in line
    )
    environment = {
        "base": revs["base"],
        "candidate": revs["candidate"],
        "harness": harness,
        "targets": targets,
        "filter": args.filter,
        "build_environment": "profile/flag/target overrides checked absent from the environment and Cargo "
        "configuration files before each build",
        "timed_profile": profile_settings(trees["candidate"] / "Cargo.toml", args.profile) if timed else None,
        "instructions_profile": profile_settings(trees["candidate"] / "Cargo.toml", "bench-instructions")
        if COUNTED in compared
        else None,
        "not_comparable": not_comparable,
        "executables_sha256": {
            target: {side: sha256(executables[side][target]) for side in SIDES} for target in compared
        },
        "cpu": cpu_model(),
        "os": f"{platform.system()} {platform.release()} {platform.machine()}",
        "rustc": output([compiler, "-V"], cwd=trees["base"]),
        "rustc_verbose": rustc,
        "load_average_at_start": [round(x, 2) for x in load],
    }
    same = [target for target, digests in environment["executables_sha256"].items() if digests["base"] == digests["candidate"]]
    summary = [
        f"## Benchmark comparison: {environment['cpu']}",
        "",
        "| Item | Value |",
        "|---|---|",
        f"| Base | `{revs['base']}` |",
        f"| Candidate | `{revs['candidate']}` |",
        f"| Harness | `{harness}` |",
        *([f"| Filter | `{args.filter}` |"] if args.filter is not None else []),
        f"| Build environment | {environment['build_environment']} |",
        *([f"| Timed profile | {environment['timed_profile']} |"] if timed else []),
        *([f"| Instructions profile | {environment['instructions_profile']} |"] if COUNTED in compared else []),
        f"| OS | {environment['os']} |",
        f"| rustc | {environment['rustc']}, LLVM {rustc.get('LLVM version', 'not reported')} |",
        f"| Load average at start | {' '.join(map(str, environment['load_average_at_start']))} |",
        f"| Identical executables on both sides | {', '.join(same) or 'none'} |",
    ]
    if not_comparable:
        summary += [
            "",
            f"Not comparable: {', '.join(not_comparable)}. Their benchmark code does not compile against the "
            "base library; see `logs/build-base-<target>.log`. Their candidate results were checked.",
        ]
    report = {}

    code_summary = []
    if args.analyze_codegen and timed:
        tools = sysroot / "lib" / "rustlib" / rustc["host"] / "bin"
        for tool in ("llvm-objdump", "llvm-nm"):
            if not (tools / tool).exists():
                fail(f"{tools / tool} not found; add the llvm-tools component to the toolchain in {sysroot}")
        environment["llvm_objdump"] = output([tools / "llvm-objdump", "--version"]).splitlines()[1].strip()
        code_dir = args.out / "codegen"
        code_dir.mkdir()
        code = {}
        keys = ("instructions", "bytes", "helper_branches", "helper_references", "unresolved_branches")
        for target in timed:
            shapes = {}
            for side in SIDES:
                found = functions(tools, executables[side][target], target, code_dir / f"{target}-{side}.s.gz")
                (code_dir / f"{target}-{side}.s").write_text(
                    "\n\n".join("\n".join(copy["lines"]) for copies in found.values() for copy in copies) + "\n"
                )
                shapes[side] = {
                    name: [{key: copy[key] for key in keys} for copy in copies] for name, copies in found.items()
                }
            ambiguous = sorted({name for side in SIDES for name, copies in shapes[side].items() if len(copies) > 1})
            names = sorted((shapes["base"].keys() | shapes["candidate"].keys()) - set(ambiguous))
            single = {side: {name: copies[0] for name, copies in shapes[side].items()} for side in SIDES}
            code[target] = {
                "totals": {
                    side: {
                        "functions": sum(len(copies) for copies in shapes[side].values()),
                        **{
                            key: total(entry[key] for copies in shapes[side].values() for entry in copies)
                            for key in keys
                        },
                    }
                    for side in SIDES
                },
                "changed": {
                    name: {side: single[side].get(name) for side in SIDES}
                    for name in names
                    if single["base"].get(name) != single["candidate"].get(name)
                },
                "ambiguous": {name: {side: shapes[side].get(name, []) for side in SIDES} for name in ambiguous},
            }
        (code_dir / "codegen.json").write_text(json.dumps(code, indent=1) + "\n")
        changes = sorted(
            ((target, name, entry) for target in code for name, entry in code[target]["changed"].items()),
            key=lambda item: (-abs(change(item[2], "helper_branches")), -abs(change(item[2], "bytes"))),
        )
        code_summary = [
            "",
            f"### Code: functions of the benchmark crate and of bidecimal in the {', '.join(timed)} executables",
            "",
            f"Disassembled with {environment['llvm_objdump']}, each function within its symbol's size. Per "
            "side: instructions; encoded bytes; calls and jumps, conditional and tail ones included, to the "
            "128-bit division helpers, directly, through a PLT entry, or on x86-64 through a GOT slot whose "
            "relocation fixes its target, a PLT slot or one in the segment read-only after relocation; on "
            "x86-64, the other instructions with a rip-relative operand that is a helper's GOT "
            "slot or address, such as a load into a register for an indirect call (n/a on AArch64, where they "
            "are not decoded); and the indirect calls and jumps whose target is not resolved, through registers "
            "and jump tables included, any of which may reach a helper. A function absent on one side may be inlined into its "
            "callers there, which is not an improvement by itself, and equal counts do not mean equal code. A "
            "name that occurs more than once on a side is not paired. `codegen/` holds the complete disassembly "
            "of both sides, the selected functions, and every function whose counts differ.",
            "",
            "| Target | Functions | Instructions | Bytes | Helper calls and jumps | Other helper references "
            "| Unresolved indirect branches | Functions whose counts differ | Unpaired names |",
            "|---|---|---|---|---|---|---|---|---|",
            *(
                f"| {target} "
                + "".join(
                    f"| {count_cell(c['totals']['base'][key])} → {count_cell(c['totals']['candidate'][key])} "
                    for key in ("functions", *keys)
                )
                + f"| {len(c['changed'])} | {len(c['ambiguous'])} |"
                for target, c in code.items()
            ),
        ]
        if changes:
            code_summary += [
                "",
                f"{min(len(changes), 20)} of {len(changes)} functions whose counts differ, helper branch changes "
                "first (instructions / bytes / helper calls and jumps / other helper references / unresolved "
                "indirect branches):",
                "",
                "| Target | Function | Base | Candidate |",
                "|---|---|---|---|",
                *(
                    f"| {target} | `{name[:120]}` | {shape_cell(entry['base'], keys)} "
                    f"| {shape_cell(entry['candidate'], keys)} |"
                    for target, name, entry in changes[:20]
                ),
            ]
        report["code"] = {target: c["totals"] for target, c in code.items()}

    if timed:
        home = (args.out / "criterion").resolve()
        order = []
        for round_index in range(args.rounds):
            sides = SIDES if round_index % 2 == 0 else SIDES[::-1]
            for target in timed:
                for side in sides:
                    baseline = f"{side}{round_index}"
                    order.append(f"{target}/{baseline}")
                    with open(logs / f"{target}-{baseline}.log", "w") as log:
                        run = subprocess.run(
                            [
                                executables[side][target],
                                "--bench",
                                "--save-baseline",
                                baseline,
                                *([args.filter] if args.filter is not None else []),
                            ],
                            cwd=trees[side],
                            env={**os.environ, "CRITERION_HOME": str(home / target)},
                            stdout=log,
                            stderr=subprocess.STDOUT,
                        )
                    if run.returncode != 0:
                        fail(f"{target} {baseline} exited with {run.returncode}; see {log.name}")
        baselines = [f"{side}{r}" for side in SIDES for r in range(args.rounds)]
        results = collect(home, {target: titles[target] for target in timed}, baselines)
        benchmarks = {}
        for key in sorted(results):
            entry = results[key]
            base = [entry[f"base{r}"] for r in range(args.rounds)]
            candidate = [entry[f"candidate{r}"] for r in range(args.rounds)]
            log_ratios = [math.log(c / b) for b, c in zip(base, candidate)]
            benchmarks[key] = {
                "unit": entry["unit"],
                "elements_per_iteration": entry["elements"],
                "base": base,
                "candidate": candidate,
                "round_log_ratios": log_ratios,
                "ratio": math.exp(statistics.median(log_ratios)),
            }
        rows = [
            f"| {key} | {b['unit']} | {cell(b['base'])} | {cell(b['candidate'])} | {b['ratio']:.3f} "
            f"({', '.join(f'{math.exp(x):.3f}' for x in b['round_log_ratios'])}) |"
            for key, b in by_change(benchmarks.items())
        ]
        environment.update(rounds=args.rounds, order=order)
        report["benchmarks"] = benchmarks
        summary += [
            "",
            f"### Times: {len(rows)} benchmarks, {args.rounds} runs per side in alternating order",
            "",
            "Largest change first. Candidate / base is the median over rounds of the ratio of the two runs "
            "of each round, taken on a log scale, with each round's ratio in parentheses. A ratio within the "
            "spread of an A/A comparison (the same commit as base and candidate) on the same runner is noise.",
            "",
            "| Benchmark | Unit | Base median (range) | Candidate median (range) | Candidate / base (rounds) |",
            "|---|---|---|---|---|",
            *rows,
        ]
    summary += code_summary

    if counted:
        home = (args.out / "gungraun").resolve()
        counts = {}
        for side in counted:
            with open(logs / f"{COUNTED}-{side}.log", "w") as log:
                run = subprocess.run(
                    [executables[side][COUNTED], "--bench", "--save-summary=json", f"--home={home / side}"],
                    cwd=trees[side],
                    stdout=log,
                    stderr=subprocess.STDOUT,
                )
            if run.returncode != 0:
                fail(f"{COUNTED} {side} exited with {run.returncode}; see {log.name}")
            counts[side] = collect_counts(home / side, titles[COUNTED])
        environment.update(
            valgrind=output(["valgrind", "--version"]),
            gungraun_runner=output(["gungraun-runner", "--version"]),
        )
    if COUNTED in compared:
        instructions = {
            title: {
                "base": counts["base"][title],
                "candidate": counts["candidate"][title],
                "ratio": counts["candidate"][title] / counts["base"][title],
            }
            for title in sorted(titles[COUNTED])
        }
        report["instructions"] = instructions
        summary += [
            "",
            f"### Instructions: {len(instructions)} benchmarks, one run per side",
            "",
            f"Executed instructions counted by {environment['valgrind']} Cachegrind through "
            f"{environment['gungraun_runner']}; they are not times. Largest change first.",
            "",
            "| Benchmark | Base | Candidate | Candidate / base |",
            "|---|---|---|---|",
            *(
                f"| {title} | {c['base']} | {c['candidate']} | {c['ratio']:.4f} |"
                for title, c in by_change(instructions.items())
            ),
        ]

    (args.out / "results.json").write_text(json.dumps({**environment, **report}, indent=1) + "\n")
    (args.out / "summary.md").write_text("\n".join(summary) + "\n")
    print(
        f"{args.out / 'summary.md'}: {', '.join(f'{len(v)} {k}' for k, v in report.items()) or 'nothing compared'}, "
        f"not comparable: {not_comparable or 'none'}"
    )


if __name__ == "__main__":
    main()
