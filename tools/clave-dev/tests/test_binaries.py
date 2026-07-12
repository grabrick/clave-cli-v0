import hashlib
import os
import stat
import tempfile
import unittest
from pathlib import Path

from clave_dev.binaries import (
    build_command,
    fresh_binary,
    identify_binary,
    sanitized_env,
    sha256_file,
)


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


class IdentityTest(unittest.TestCase):
    def test_sha256_file(self):
        with tempfile.NamedTemporaryFile(delete=False) as f:
            f.write(b"clave-bin")
            p = f.name
        self.addCleanup(os.unlink, p)
        self.assertEqual(sha256_file(Path(p)), hashlib.sha256(b"clave-bin").hexdigest())

    def test_identify_prefers_version_over_help(self):
        with tempfile.TemporaryDirectory() as d:
            fake = Path(d) / "clave"
            fake.write_text('#!/bin/bash\n[ "$1" = "--version" ] && echo "clave 9.9.9" && exit 0\necho "help top"\n')
            fake.chmod(fake.stat().st_mode | stat.S_IEXEC)
            self.assertEqual(identify_binary(fake), "clave 9.9.9")

    def test_identify_falls_back_to_help(self):
        with tempfile.TemporaryDirectory() as d:
            fake = Path(d) / "clave"
            fake.write_text('#!/bin/bash\n[ "$1" = "--version" ] && exit 2\necho "help first line"\n')
            fake.chmod(fake.stat().st_mode | stat.S_IEXEC)
            self.assertEqual(identify_binary(fake), "help first line")
