"""Exercise update manifest publication offline with a fake release asset store."""

import os
import json
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
CHANNELS = json.loads((ROOT / "packaging/release-assets.json").read_text())["update_channels"]
FAKE_GH = '''#!/bin/bash
set -eu
echo "$*" >> calls
case "$2" in
  view) [[ -d "releases/$3" ]] ;;
  create) mkdir -p "releases/$3" ;;
  upload) cp "$4" "releases/$3/" ;;
  *) exit 43 ;;
esac
'''


def step(name):
    workflow = (ROOT / ".github/workflows/package.yml").read_text()
    block = workflow.split(f"      - name: {name}\n", 1)[1]
    script = block.split("        run: |\n", 1)[1].split("\n      - name: ", 1)[0]
    return "\n".join(line[10:] for line in script.splitlines())


class ManifestPublishTests(unittest.TestCase):
    def publish(self, root, channel, signed):
        release = CHANNELS[channel]
        if signed:
            (root / release["manifest"]).write_text('{"signed": "fixture"}')
        (root / "gh").write_text(FAKE_GH)
        (root / "gh").chmod(0o755)
        env = dict(os.environ, PATH=f"{root}:{os.environ['PATH']}", RUNNER_TEMP=str(root),
                   RELEASE_CHANNEL=channel, RELEASE_COMMIT="0" * 40,
                   UPDATE_MANIFEST=release["manifest"], UPDATE_MANIFEST_TAG=release["tag"])
        return subprocess.run(["bash", "-eu", "-c", step("Publish the signed update manifest")],
                              cwd=root, env=env, capture_output=True, text=True)

    def test_each_channel_manifest_lands_where_the_client_fetches_it(self):
        for channel, release in CHANNELS.items():
            with self.subTest(channel=channel), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                result = self.publish(root, channel, signed=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertTrue((root / "releases" / release["tag"] / release["manifest"]).is_file())
                create = next(line for line in (root / "calls").read_text().splitlines()
                              if line.startswith("release create"))
                self.assertIn("--prerelease --latest=false", create)

    def test_a_missing_signing_key_skips_publication_with_a_notice(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            result = self.publish(root, "stable", signed=False)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("::notice::", result.stdout)
            self.assertFalse((root / "calls").exists())
