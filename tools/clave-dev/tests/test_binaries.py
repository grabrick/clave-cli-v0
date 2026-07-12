import os
import unittest
from pathlib import Path

from clave_dev.binaries import build_command, fresh_binary, sanitized_env


class BinariesTest(unittest.TestCase):
    def test_build_command_and_fresh_path_share_profile(self):
        self.assertEqual(build_command("debug"), ["cargo", "build"])
        self.assertEqual(build_command("release"), ["cargo", "build", "--release"])
        self.assertEqual(
            fresh_binary(Path("/wt"), "debug"), Path("/wt/target/debug/clave")
        )
        self.assertEqual(
            fresh_binary(Path("/wt"), "release"), Path("/wt/target/release/clave")
        )

    def test_build_command_rejects_unknown_profile(self):
        with self.assertRaises(ValueError):
            build_command("nope")

    def test_sanitized_env_drops_target_and_repo_from_path(self):
        wt = Path("/tmp/wt").resolve()
        base = {
            "PATH": os.pathsep.join(
                [
                    "/usr/bin",
                    str(wt),
                    str(wt / "target" / "debug"),
                    str(wt / "target" / "release"),
                    "/usr/local/bin",
                ]
            )
        }
        env = sanitized_env(wt, base)
        parts = env["PATH"].split(os.pathsep)
        self.assertIn("/usr/bin", parts)
        self.assertIn("/usr/local/bin", parts)
        self.assertNotIn(str(wt), parts)
        self.assertNotIn(str(wt / "target" / "debug"), parts)
        self.assertNotIn(str(wt / "target" / "release"), parts)
