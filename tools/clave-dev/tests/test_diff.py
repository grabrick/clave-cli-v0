import subprocess
import tempfile
import unittest
from pathlib import Path

from clave_dev.diff import build_diff


def _repo(path: Path):
    for a in (["init", "-q"], ["config", "user.email", "t@t"], ["config", "user.name", "t"]):
        subprocess.run(["git", "-C", str(path), *a], check=True)
    (path / "f.txt").write_text("one\n")
    subprocess.run(["git", "-C", str(path), "add", "."], check=True)
    subprocess.run(["git", "-C", str(path), "commit", "-q", "-m", "init"], check=True)


class DiffTest(unittest.TestCase):
    def test_build_diff_reports_changed_files_and_writes_patch(self):
        with tempfile.TemporaryDirectory() as d:
            wt = Path(d)
            _repo(wt)
            (wt / "f.txt").write_text("two\n")
            patch = wt / "patch.diff"
            out = build_diff(wt, patch)
            self.assertIn("f.txt", out["changed_files"])
            self.assertFalse(out["truncated"])
            self.assertTrue(patch.is_file() and "two" in patch.read_text())

    def test_clean_tree_is_empty(self):
        with tempfile.TemporaryDirectory() as d:
            wt = Path(d)
            _repo(wt)
            out = build_diff(wt, wt / "p.diff")
            self.assertEqual(out["changed_files"], [])
