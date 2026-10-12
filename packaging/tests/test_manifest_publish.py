"""Exercise update manifest publication offline with a fake release asset store."""

import os
import json
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
RELEASE_ASSETS = json.loads((ROOT / "packaging/release-assets.json").read_text())
CHANNELS = RELEASE_ASSETS["update_channels"]
PREFIX = RELEASE_ASSETS["update_payload_prefix"]
FAKE_GH = '''#!/bin/bash
set -eu
echo "$*" >> calls
case "$2" in
  view) [[ -d "releases/$3" ]] && { [[ "$*" != *--json* ]] || ls "releases/$3"; } ;;
  create) mkdir -p "releases/$3" ;;
  upload) for file in "${@:4}"; do [[ "$file" == --* ]] || cp "$file" "releases/$3/"; done ;;
  delete-asset) rm "releases/$3/$4" ;;
  edit) ;;
  *) exit 43 ;;
esac
'''
FAKE_GIT = '''#!/bin/bash
[[ "$1" != rev-parse ]] || echo 0000000
'''


def step(name):
    workflow = (ROOT / ".github/workflows/package.yml").read_text()
    block = workflow.split(f"      - name: {name}\n", 1)[1]
    script = block.split("        run: |\n", 1)[1].split("\n      - name: ", 1)[0]
    return "\n".join(line[10:] for line in script.splitlines())


class ManifestPublishTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        notes = self.root / "release-tools/packaging/release-notes.sh"
        notes.parent.mkdir(parents=True)
        notes.write_text("exit 0\n")
        for name, script in [("gh", FAKE_GH), ("git", FAKE_GIT)]:
            (self.root / name).write_text(script)
            (self.root / name).chmod(0o755)
        (self.root / "dist").mkdir()

    def run_step(self, name, channel):
        release = CHANNELS[channel]
        env = dict(os.environ, PATH=f"{self.root}:{os.environ['PATH']}", RUNNER_TEMP=str(self.root),
                   GITHUB_WORKSPACE=str(self.root), RELEASE_CHANNEL=channel,
                   RELEASE_TAG=release["tag"], RELEASE_COMMIT="0" * 40,
                   UPDATE_MANIFEST=release["manifest"], UPDATE_MANIFEST_TAG=release["tag"],
                   UPDATE_PAYLOAD_PREFIX=PREFIX)
        result = subprocess.run(["bash", "-eu", "-c", step(name)], cwd=self.root, env=env,
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        return result

    def holder(self, channel):
        return self.root / "releases" / CHANNELS[channel]["tag"]

    def publish(self, channel, signed):
        """Builds a new download and, when signed, a manifest naming a new payload."""
        (self.root / "dist/Cinnabar.dmg").write_text("new download")
        if signed:
            payloads = self.root / "update-payloads"
            payloads.mkdir()
            (payloads / f"{PREFIX}new-Cinnabar.dmg").write_text("new payload")
            (self.root / CHANNELS[channel]["manifest"]).write_text(f"names {PREFIX}new-Cinnabar.dmg")
        if channel != "stable":
            self.run_step("Publish the rolling prerelease", channel)
        return self.run_step("Publish the signed update manifest", channel)

    def published_release(self, channel):
        """A channel whose current manifest names the payload beside it."""
        holder = self.holder(channel)
        holder.mkdir(parents=True)
        (holder / CHANNELS[channel]["manifest"]).write_text(f"names {PREFIX}old-Cinnabar.dmg")
        (holder / f"{PREFIX}old-Cinnabar.dmg").write_text("old payload")
        (holder / "Cinnabar.dmg").write_text("old download")
        return holder

    def files(self, directory):
        return {path.name: path.read_text() for path in directory.iterdir()}

    def test_a_new_channel_holder_is_a_prerelease_that_never_becomes_latest(self):
        self.publish("stable", signed=True)
        self.assertTrue((self.holder("stable") / CHANNELS["stable"]["manifest"]).is_file())
        create = next(line for line in (self.root / "calls").read_text().splitlines()
                      if line.startswith("release create"))
        self.assertIn("--prerelease --latest=false", create)

    def test_signed_publication_replaces_payloads_only_after_the_new_manifest(self):
        for channel, release in CHANNELS.items():
            with self.subTest(channel=channel):
                self.setUp()
                holder = self.published_release(channel)
                self.publish(channel, signed=True)
                expected = {release["manifest"]: f"names {PREFIX}new-Cinnabar.dmg",
                            f"{PREFIX}new-Cinnabar.dmg": "new payload"}
                # The stable holder carries no downloads; those live in the version release.
                expected["Cinnabar.dmg"] = "old download" if channel == "stable" else "new download"
                self.assertEqual(self.files(holder), expected)
                calls = (self.root / "calls").read_text().splitlines()
                order = [next(index for index, call in enumerate(calls) if marker in call)
                         for marker in [f"{PREFIX}new-Cinnabar.dmg", release["manifest"],
                                        f"delete-asset {release['tag']} {PREFIX}old"]]
                self.assertEqual(order, sorted(order))

    def test_without_signing_the_current_manifest_and_its_payloads_stay_consistent(self):
        for channel in CHANNELS:
            with self.subTest(channel=channel):
                self.setUp()
                holder = self.published_release(channel)
                result = self.publish(channel, signed=False)
                self.assertIn("::notice::", result.stdout)
                files = self.files(holder)
                named = files[CHANNELS[channel]["manifest"]].removeprefix("names ")
                self.assertEqual(files[named], "old payload")
                self.assertNotIn(f"{PREFIX}new-Cinnabar.dmg", files)
