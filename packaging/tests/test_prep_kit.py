"""Check the files staged by the production preparation-kit packagers."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
STAGER = ROOT / "packaging/common/stage-payload.sh"


class PrepKitTests(unittest.TestCase):
    def assert_resources(self, resources, platform):
        """Require the declared payload and exact bytes for added compiler inputs."""
        manifest = subprocess.run(
            ["bash", "-c", 'source "$1"; resource_manifest "$2"',
             "--", str(STAGER), platform],
            env={**os.environ, "CINNABAR_SOURCE_ROOT": str(ROOT)},
            check=True, capture_output=True, text=True,
        ).stdout.splitlines()
        actual = {path.relative_to(resources).as_posix()
                  for path in resources.rglob("*") if path.is_file()}
        self.assertEqual(actual, set(manifest))
        for line in (ROOT / "packaging/common/prep-kit-files.tsv").read_text().splitlines():
            source, destination = line.split("\t")
            path = resources / "prep-kit" / destination
            self.assertIn(f"prep-kit/{destination}", manifest)
            self.assertEqual(path.read_bytes(), (ROOT / source).read_bytes())

    @unittest.skipUnless(shutil.which("bash"), "Bash is required")
    def test_shared_stager_and_payload_manifests(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for platform in ("macos", "linux", "windows"):
                with self.subTest(platform=platform):
                    compiler = root / ("assetc.exe" if platform == "windows" else "assetc")
                    compiler.write_bytes(b"compiler fixture")
                    resources = root / platform
                    subprocess.run(
                        ["bash", "-c", 'source "$1"; stage_resources "$2" "$3"',
                         "--", str(STAGER), str(resources), str(compiler)],
                        env={**os.environ, "CINNABAR_UPDATE_URL": "",
                             "CINNABAR_SOURCE_ROOT": str(ROOT)},
                        check=True, capture_output=True, text=True,
                    )
                    self.assert_resources(resources, platform)

    @unittest.skipUnless(shutil.which("pwsh") and shutil.which("bash"),
                         "PowerShell and Bash are required")
    def test_windows_installer_stages_the_declared_payload(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binaries = root / "binaries"
            binaries.mkdir()
            for name in ("bedrock-client", "bedrock-core", "bedrock-local-server", "assetc"):
                (binaries / f"{name}.exe").write_bytes(b"binary fixture")
            wrapper = root / "stage.ps1"
            wrapper.write_text(
                'param($Builder, $Out, $Binaries)\n'
                '$ErrorActionPreference = "Stop"\n'
                '# Stop after payload staging, before icon rendering and signing.\n'
                'function magick { throw "fixture: payload staged" }\n'
                'try { & $Builder -Out $Out -Binaries $Binaries } catch {\n'
                '    if ($_.Exception.Message -eq "fixture: payload staged") { exit 0 }\n'
                '    throw\n'
                '}\n'
                'throw "installer did not reach icon rendering"\n'
            )
            output = root / "dist"
            subprocess.run(
                ["pwsh", "-NoProfile", "-File", str(wrapper),
                 str(ROOT / "packaging/windows/build-installer.ps1"),
                 str(output), str(binaries)],
                env={**os.environ, "CINNABAR_UPDATE_URL": ""},
                check=True, capture_output=True, text=True,
            )
            self.assert_resources(output / "payload/resources", "windows")


if __name__ == "__main__":
    unittest.main()
