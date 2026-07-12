"""Разделение и изоляция бинарей: known-good (инструмент) vs fresh (объект)."""
from __future__ import annotations

import os
import shutil
import subprocess
from collections import namedtuple
from pathlib import Path
from typing import Mapping, Optional

PROFILE_DIRS = {"debug": "debug", "release": "release"}

KnownGood = namedtuple("KnownGood", "path version")


def build_command(profile: str) -> list:
    """Команда сборки для профиля (единый источник вместе с fresh_binary)."""
    if profile not in PROFILE_DIRS:
        raise ValueError(f"неизвестный build_profile: {profile}")
    return ["cargo", "build"] + (["--release"] if profile == "release" else [])


def fresh_binary(worktree: Path, profile: str) -> Path:
    """Путь к свежесобранному бинарю (только для observer)."""
    return Path(worktree) / "target" / PROFILE_DIRS[profile] / "clave"


def sanitized_env(worktree: Path, base_env: Optional[Mapping] = None) -> dict:
    """Окружение для дочерних процессов без каталогов, где мог бы оказаться fresh clave:
    из PATH выкидываем target/debug, target/release и корень worktree ('.')."""
    env = dict(base_env if base_env is not None else os.environ)
    worktree = Path(worktree).resolve()
    forbidden = {
        str(worktree),
        str(worktree / "target" / "debug"),
        str(worktree / "target" / "release"),
    }
    parts = [
        p
        for p in env.get("PATH", "").split(os.pathsep)
        if p and str(Path(p).resolve()) not in forbidden
    ]
    env["PATH"] = os.pathsep.join(parts)
    return env


def snapshot_known_good(known_good: Path, tmp_dir: Path) -> KnownGood:
    """Копируем known-good в приватный temp (чтобы посторонний cargo install не подменил)
    и логируем идентификацию версии."""
    known_good = Path(known_good).resolve()
    if not known_good.is_file():
        raise FileNotFoundError(f"known-good clave не найден: {known_good}")
    dest_dir = Path(tmp_dir) / "known-good"
    dest_dir.mkdir(parents=True, exist_ok=True)
    dest = dest_dir / "clave"
    shutil.copy2(known_good, dest)
    dest.chmod(0o755)
    try:
        version = (
            subprocess.run(
                [str(dest), "--help"], capture_output=True, text=True, timeout=10
            )
            .stdout.splitlines()[0]
            .strip()
        )
    except Exception:
        version = "unknown"
    return KnownGood(path=dest, version=version)
