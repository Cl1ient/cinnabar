"""Check median comparisons and reject incomplete Criterion artifacts."""

import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch


MODULE = Path(__file__).resolve().parents[1] / "scripts/runtime_profile.py"
SPEC = importlib.util.spec_from_file_location("runtime_profile", MODULE)
profile = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(profile)


class RuntimeSummaryTests(unittest.TestCase):
    """Use small Criterion-shaped artifacts without compiling or running benchmarks."""

    def setUp(self):
        """Give every variant one measured case in each requested bench target."""
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        for variant, settings in profile.VARIANTS.items():
            artifact = self.root / f"runtime-profile-{variant}"
            artifact.mkdir()
            metadata = {
                "variant": variant,
                "head_sha": "same-revision",
                "sample_size": profile.SAMPLE_SIZE,
                "profile": "release",
                "rustc": "same-compiler",
                "lto": settings.get("lto", "thin"),
                "rustflags": settings.get("rustflags", ""),
                "allocator": "mimalloc" if settings.get("mimalloc") else "system",
                "cpu": {"lscpu": [{"field": "Model name:", "data": "Test CPU"}]},
                "completed": True,
                "completed_benches": [
                    f"{package}/{bench}"
                    for package, benches in profile.BENCHES.items() for bench in benches
                ],
            }
            (artifact / "runtime-profile.json").write_text(json.dumps(metadata))
            for package, benches in profile.BENCHES.items():
                for bench in benches:
                    directory = artifact / "criterion" / package / bench / "case" / "new"
                    directory.mkdir(parents=True)
                    self.write_case(directory, "case|one", 100 if variant == "baseline" else 80)

    def write_case(self, directory, identity, median):
        """Write only the Criterion fields needed for a comparison."""
        (directory / "estimates.json").write_text(json.dumps({
            "median": {"point_estimate": median}, "mean": {"point_estimate": 999},
        }))
        (directory / "benchmark.json").write_text(json.dumps({"full_id": identity}))
        (directory / "sample.json").write_text(json.dumps({
            "times": [median] * profile.SAMPLE_SIZE, "iters": [1] * profile.SAMPLE_SIZE,
        }))

    def case_directory(self, variant):
        """Locate one case without repeating the configured target list."""
        return next((self.root / f"runtime-profile-{variant}").glob("criterion/**/new"))

    def test_medians_and_signed_change_use_baseline(self):
        """Report the median, with a faster candidate shown as a negative percentage."""
        report = profile.summary(self.root)
        self.assertIn("100.00 / +0.00%", report)
        self.assertIn("80.00 / -20.00%", report)
        self.assertNotIn("999.00", report)
        self.assertIn("case&#124;one", report)
        self.assertIn("Test CPU", report)

    def test_slower_variant_has_positive_change(self):
        """Show a slower candidate as a positive percentage rather than a speedup."""
        directory = self.case_directory("all")
        self.write_case(directory, "case|one", 120)
        self.assertIn("120.00 / +20.00%", profile.summary(self.root))

    def test_missing_variant_fails(self):
        """A missing artifact cannot produce a successful partial comparison."""
        (self.root / "runtime-profile-all" / "runtime-profile.json").unlink()
        with self.assertRaisesRegex(ValueError, "Incomplete results; all: missing"):
            profile.summary(self.root)

    def test_matching_partial_runs_fail(self):
        """Reject equal partial case sets and name every unfinished bench and variant."""
        for variant in profile.VARIANTS:
            path = self.root / f"runtime-profile-{variant}" / "runtime-profile.json"
            data = json.loads(path.read_text())
            del data["completed"]
            missing_bench = data["completed_benches"].pop()
            path.write_text(json.dumps(data))
        output = self.root / "summary.md"
        arguments = [str(MODULE), "summary", str(self.root), str(output)]
        with patch.object(sys, "argv", arguments):
            with self.assertRaises(SystemExit):
                profile.main()
        report = output.read_text()
        self.assertIn("Runtime measurements incomplete", report)
        self.assertNotIn("median (ns)", report)
        for variant in profile.VARIANTS:
            self.assertIn(f"{variant}: missing {missing_bench}", report)

    def test_completion_marker_is_required(self):
        """Reject a run whose commands are recorded but final completion is absent."""
        path = self.root / "runtime-profile-all" / "runtime-profile.json"
        data = json.loads(path.read_text())
        del data["completed"]
        path.write_text(json.dumps(data))
        with self.assertRaisesRegex(ValueError, "all: missing run completion marker"):
            profile.summary(self.root)

    def test_missing_bench_fails(self):
        """Every requested bench target must emit estimates."""
        (self.case_directory("fat-lto") / "estimates.json").unlink()
        with self.assertRaisesRegex(ValueError, "Missing estimates"):
            profile.summary(self.root)

    def test_different_cases_fail(self):
        """Variant coverage must match baseline rather than silently omitting a row."""
        path = self.case_directory("all") / "benchmark.json"
        path.write_text(json.dumps({"full_id": "different-case"}))
        with self.assertRaisesRegex(ValueError, "Different benchmark cases"):
            profile.summary(self.root)

    def test_group_sample_override_fails(self):
        """Detect a benchmark group that overrides the fixed experiment sample count."""
        path = self.case_directory("x86-64-v2") / "sample.json"
        path.write_text(json.dumps({"times": [1] * 10, "iters": [1] * 10}))
        with self.assertRaisesRegex(ValueError, "Wrong sample count"):
            profile.summary(self.root)

    def test_invalid_medians_fail(self):
        """Zero or nonfinite medians cannot yield meaningful percentage comparisons."""
        for median in [0, -1, float("nan"), float("inf")]:
            with self.subTest(median=median):
                path = self.case_directory("mimalloc") / "estimates.json"
                path.write_text(json.dumps({"median": {"point_estimate": median}}))
                with self.assertRaisesRegex(ValueError, "Invalid median"):
                    profile.summary(self.root)

    def test_different_revisions_fail(self):
        """Never compare an artifact from another source revision."""
        path = self.root / "runtime-profile-all" / "runtime-profile.json"
        data = json.loads(path.read_text())
        data["head_sha"] = "other-revision"
        path.write_text(json.dumps(data))
        with self.assertRaisesRegex(ValueError, "Different source revision"):
            profile.summary(self.root)


class RuntimeCompletionTests(unittest.TestCase):
    """Check the completion contract with mocked benchmark commands."""

    def setUp(self):
        """Prepare runner metadata inputs in a temporary workspace without building."""
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        (self.root / "Cargo.toml").write_text((MODULE.parents[2] / "Cargo.toml").read_text())
        self.target = self.root / "target"
        self.metadata = self.target / "runtime-profile.json"
        self.benches = [
            f"{package}/{bench}"
            for package, benches in profile.BENCHES.items() for bench in benches
        ]
        environment = {"CARGO_TARGET_DIR": str(self.target), "GITHUB_SHA": "same-revision"}
        self.enterContext(patch.dict(os.environ, environment))
        self.enterContext(patch.object(
            profile.subprocess, "check_output", side_effect=["same-compiler", '{"lscpu": []}'],
        ))

    def test_success_marks_completion_after_all_commands(self):
        """Record completion only after the last successful benchmark command returns."""
        def succeed(*args, **kwargs):
            """Observe the pending marker while each simulated command is running."""
            self.assertNotIn("completed", json.loads(self.metadata.read_text()))

        with patch.object(profile.subprocess, "run", side_effect=succeed) as command:
            profile.run("baseline", self.root)
        metadata = json.loads(self.metadata.read_text())
        self.assertTrue(metadata["completed"])
        self.assertEqual(metadata["completed_benches"], self.benches)
        self.assertEqual(command.call_count, len(self.benches))

    def test_metadata_failure_removes_previous_completion(self):
        """Discard a stale success marker even if runner metadata collection fails."""
        self.target.mkdir()
        self.metadata.write_text(json.dumps({"completed": True}))
        with patch.object(profile.subprocess, "check_output", side_effect=OSError("missing tool")):
            with self.assertRaises(OSError):
                profile.run("baseline", self.root)
        self.assertFalse(self.metadata.exists())

    def test_later_failure_leaves_run_incomplete(self):
        """A final bench failure cannot retain a marker from an earlier successful run."""
        self.target.mkdir()
        self.metadata.write_text(json.dumps({"completed": True, "completed_benches": self.benches}))
        outcomes = [None] * (len(self.benches) - 1) + [subprocess.CalledProcessError(1, "bench")]
        with patch.object(profile.subprocess, "run", side_effect=outcomes):
            with self.assertRaises(subprocess.CalledProcessError):
                profile.run("baseline", self.root)
        metadata = json.loads(self.metadata.read_text())
        self.assertNotIn("completed", metadata)
        self.assertEqual(metadata["completed_benches"], self.benches[:-1])


if __name__ == "__main__":
    unittest.main()
