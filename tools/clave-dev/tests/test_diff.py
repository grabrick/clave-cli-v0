import subprocess
import tempfile
import unittest
from pathlib import Path

from clave_dev.diff import build_diff, changed_paths


def _repo(path: Path):
    for a in (["init", "-q"], ["config", "user.email", "t@t"], ["config", "user.name", "t"]):
        subprocess.run(["git", "-C", str(path), *a], check=True)
    (path / "f.txt").write_text("one\n")
    subprocess.run(["git", "-C", str(path), "add", "."], check=True)
    subprocess.run(["git", "-C", str(path), "commit", "-q", "-m", "init"], check=True)


class DiffTest(unittest.TestCase):
    """Патч всегда пишем ВНЕ репозитория: внутри он попал бы в собственный диф."""

    def test_build_diff_reports_changed_files_and_writes_patch(self):
        with tempfile.TemporaryDirectory() as d, tempfile.TemporaryDirectory() as out_dir:
            wt = Path(d)
            _repo(wt)
            (wt / "f.txt").write_text("two\n")
            patch = Path(out_dir) / "patch.diff"
            out = build_diff(wt, patch)
            self.assertIn("f.txt", out["changed_files"])
            self.assertFalse(out["truncated"])
            self.assertTrue(patch.is_file() and "two" in patch.read_text())

    def test_clean_tree_is_empty(self):
        with tempfile.TemporaryDirectory() as d, tempfile.TemporaryDirectory() as out_dir:
            wt = Path(d)
            _repo(wt)
            out = build_diff(wt, Path(out_dir) / "p.diff")
            self.assertEqual(out["changed_files"], [])
            self.assertEqual(changed_paths(wt), [])

    def test_changed_paths_sees_new_untracked_file(self):
        # `git diff` новых файлов НЕ видит — если бы смотрели только его, созданный
        # агентом файл выглядел бы как «изменений нет».
        with tempfile.TemporaryDirectory() as d:
            wt = Path(d)
            _repo(wt)
            (wt / "new.txt").write_text("hi\n")
            self.assertIn("new.txt", changed_paths(wt))

    def test_build_diff_includes_new_file_via_intent_to_add(self):
        with tempfile.TemporaryDirectory() as d, tempfile.TemporaryDirectory() as out_dir:
            wt = Path(d)
            _repo(wt)
            (wt / "new.txt").write_text("hi\n")
            patch = Path(out_dir) / "p.diff"
            out = build_diff(wt, patch)
            self.assertIn("new.txt", out["changed_files"])
            self.assertIn("new.txt", patch.read_text())
