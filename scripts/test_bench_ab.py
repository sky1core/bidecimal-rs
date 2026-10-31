"""Fast runner guard tests; no compiler, benchmark, or network is invoked."""
import importlib.util
import os
import sys
import json
import subprocess
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location("bench_ab", Path(__file__).with_name("bench-ab.py"))
BENCH = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BENCH)
PROFILE_OVERRIDE = '[profile.bench]\nlto = "fat"\n'


class BuildEnvironmentTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.work = self.root / "work" / "candidate"
        self.work.mkdir(parents=True)
        self.home = self.root / "home"
        self.home.mkdir()
        self.env = patch.dict(os.environ, {"HOME": str(self.home)}, clear=True)
        self.env.start()
        self.addCleanup(self.env.stop)

    def test_clean_environment_and_explicit_compiler(self):
        os.environ["RUSTC"] = "/explicit/compiler"
        BENCH.check_build_environment([self.work])

    def test_profile_flags_targets_and_wrappers_rejected_even_empty(self):
        names = ["CARGO_PROFILE_BENCH_LTO", "CARGO_PROFILE_RELEASE_OPT_LEVEL",
                 "CARGO_PROFILE_BENCH_FAT_LTO_CODEGEN_UNITS", "CARGO_BUILD_TARGET",
                 "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS",
                 "RUSTFLAGS", "RUSTDOCFLAGS", "RUSTC_BOOTSTRAP", "RUSTC_WRAPPER",
                 "RUSTC_WORKSPACE_WRAPPER", "CARGO_BUILD_RUSTC", "CARGO", "RUSTDOC"]
        for name in names:
            for value in ("", "override"):
                with self.subTest(name=name, value=value), patch.dict(os.environ, {name: value}):
                    with self.assertRaisesRegex(SystemExit, name):
                        BENCH.check_build_environment([self.work])

    def test_cache_transport_and_log_settings_allowed(self):
        values = {name: "x" for name in BENCH.ALLOWED_CARGO_VARIABLES}
        values.update(CARGO_HOME=str(self.home), CARGO_NET_OFFLINE="true", CARGO_HTTP_TIMEOUT="30",
                      CARGO_TERM_COLOR="never", CARGO_ALIAS_B="build", CARGO_REGISTRY_TOKEN="x",
                      CARGO_REGISTRIES_CRATES_IO_PROTOCOL="sparse")
        with patch.dict(os.environ, values):
            BENCH.check_build_environment([self.work])

    def test_local_and_ancestor_configs_rejected(self):
        for directory in (self.work, self.work.parent, self.root):
            for name in ("config", "config.toml"):
                p = directory / ".cargo" / name
                p.parent.mkdir(exist_ok=True)
                p.write_text(PROFILE_OVERRIDE)
                with self.subTest(path=p), self.assertRaisesRegex(SystemExit, "Cargo configuration"):
                    BENCH.check_build_environment([self.work])
                p.unlink()

    def test_cache_transport_and_log_configuration_allowed(self):
        p = self.home / ".cargo" / "config.toml"
        p.parent.mkdir()
        p.write_text('[build]\nbuild-dir = "/cache"\ntarget-dir = "/target"\n[net]\nretry = 3\n'
                     '[term]\ncolor = "never"\n[http]\ntimeout = 30\n[cache]\nauto-clean-frequency = "never"\n'
                     '[alias]\nb = "build"\n[registries.crates-io]\nprotocol = "sparse"\n'
                     '[source.crates-io]\nreplace-with = "mirror"\n[source.mirror]\nregistry = "sparse+https://example.com/"\n')
        BENCH.check_build_environment([self.work])

    def test_build_settings_in_configuration_rejected(self):
        p = self.home / ".cargo" / "config.toml"
        p.parent.mkdir()
        cases = (
            ('[build]\nbuild-dir = "/cache"\nrustflags = ["-Ctarget-cpu=native"]\n', "sets build.rustflags:"),
            ('build.target = "x86_64-unknown-linux-gnu"\n', "sets build.target:"),
            ('[target.x86_64-unknown-linux-gnu]\nrustflags = []\n', "sets target:"),
            ('[env]\nRUSTFLAGS = ""\n', "sets env:"),
            ("[build\n", "cannot check Cargo configuration"),
        )
        for text, message in cases:
            p.write_text(text)
            with self.subTest(text=text), self.assertRaisesRegex(SystemExit, message):
                BENCH.check_build_environment([self.work])

    def test_default_and_empty_cargo_home(self):
        p = self.home / ".cargo" / "config.toml"
        p.parent.mkdir()
        p.write_text(PROFILE_OVERRIDE)
        for env in ({}, {"CARGO_HOME": ""}):
            with patch.dict(os.environ, env), self.assertRaisesRegex(SystemExit, "Cargo configuration"):
                BENCH.check_build_environment([self.work])

    def test_explicit_and_relative_cargo_home(self):
        for value in (str(self.home), "../cache"):
            home = self.work / value
            home.mkdir(parents=True, exist_ok=True)
            for name in ("config", "config.toml"):
                p = home / name
                p.write_text(PROFILE_OVERRIDE)
                with patch.dict(os.environ, {"CARGO_HOME": value}):
                    with self.assertRaisesRegex(SystemExit, "Cargo configuration"):
                        BENCH.check_build_environment([self.work])
                p.unlink()

    def test_rechecks_configuration_before_build(self):
        BENCH.check_build_environment([self.work])
        p = self.work / ".cargo" / "config"
        p.parent.mkdir()
        p.write_text(PROFILE_OVERRIDE)
        with patch.object(BENCH.subprocess, "run") as run:
            with self.assertRaisesRegex(SystemExit, "Cargo configuration"):
                BENCH.build(self.work, "ops", "bench", 1, Path("rustc"), self.root / "log")
            run.assert_not_called()

    def test_override_rejected_before_subprocess(self):
        with patch.dict(os.environ, {"CARGO_PROFILE_BENCH_LTO": "fat"}):
            with patch.object(BENCH.subprocess, "run") as run:
                with self.assertRaisesRegex(SystemExit, "CARGO_PROFILE_BENCH_LTO"):
                    BENCH.build(self.work, "ops", "bench", 1, Path("rustc"), self.root / "log")
                run.assert_not_called()

    def test_valid_build_keeps_selected_profile_and_compiler(self):
        artifact = {"reason": "compiler-artifact", "target": {"kind": ["bench"], "name": "ops"},
                    "executable": "/not/executed"}
        result = subprocess.CompletedProcess([], 0, json.dumps(artifact) + "\n", "")
        for profile in ("bench", "bench-fat-lto"):
            with patch.object(BENCH.subprocess, "run", return_value=result) as run:
                self.assertEqual(BENCH.build(self.work, "ops", profile, 1, Path("/pinned/rustc"),
                                             self.root / "log"), "/not/executed")
                command = run.call_args.args[0]
                self.assertEqual(command[command.index("--profile") + 1], profile)
                env = run.call_args.kwargs["env"]
                self.assertEqual(env["RUSTC"], "/pinned/rustc")
                self.assertEqual(env["CARGO_TARGET_DIR"], str(self.work / "target"))
                self.assertEqual(env["CARGO_BUILD_BUILD_DIR"], str(self.work / "target"))

    def test_configuration_through_symlink(self):
        linked = self.root / "linked"
        linked.symlink_to(self.work, target_is_directory=True)
        p = self.work.parent / ".cargo" / "config.toml"
        p.parent.mkdir()
        p.write_text(PROFILE_OVERRIDE)
        with self.assertRaisesRegex(SystemExit, "Cargo configuration"):
            BENCH.check_build_environment([linked])

    def test_filter_forwarding_and_empty_selection(self):
        with patch.object(BENCH, "output", return_value="one/set: benchmark\n") as output:
            self.assertEqual(BENCH.listed_titles("binary", self.work, filter="^one/"), {"one/set"})
            self.assertEqual(output.call_args.args[0][-1], "^one/")
        with patch.object(BENCH, "output", return_value=""):
            with self.assertRaisesRegex(SystemExit, "lists no benchmarks matching"):
                BENCH.listed_titles("binary", self.work, filter="^missing$")


if __name__ == "__main__":
    unittest.main()
