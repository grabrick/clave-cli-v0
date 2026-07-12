"""Разделение и изоляция бинарей: known-good (инструмент) vs fresh (объект)."""
from __future__ import annotations

import hashlib
import os
import shutil
import subprocess
from collections import namedtuple
from pathlib import Path
from typing import Mapping, Optional

PROFILE_DIRS = {"debug": "debug", "release": "release"}

KnownGood = namedtuple("KnownGood", "path version hash")


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
    return KnownGood(path=dest, version=identify_binary(dest), hash=sha256_file(dest))


def snapshot_baseline(worktree: Path, profile: str, env: Mapping, tmp_dir: Path) -> Path:
    """Собрать продукт ДО правок агента и отложить бинарь — это базовая линия рендера.

    Зачем: зрение судит абсолютно, а у продукта есть дефекты, которых агент не вносил (и часть
    из них не в его власти — полый курсор рисует сам Terminal в неактивном окне). Гейт обязан
    сравнивать с «как было», иначе петля не сходится никогда.

    Копия, а не путь в worktree: первая же сборка агента затрёт target/. Побочно это ранняя
    проверка, что репозиторий собирается ДО того, как мы потратили вызов агента.
    """
    res = subprocess.run(
        build_command(profile), cwd=str(worktree), env=dict(env), capture_output=True, text=True
    )
    if res.returncode != 0:
        raise RuntimeError(
            "базовая сборка не собралась — репозиторий не зелёный ещё ДО правок:\n"
            + res.stderr.strip()[-2000:]
        )
    dest_dir = Path(tmp_dir) / "baseline"
    dest_dir.mkdir(parents=True, exist_ok=True)
    dest = dest_dir / "clave"
    shutil.copy2(fresh_binary(worktree, profile), dest)
    dest.chmod(0o755)
    return dest


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def identify_binary(path: Path) -> str:
    """Идентификация: `--version` (первая строка), фолбэк на первую строку `--help`."""
    for flag in ("--version", "--help"):
        try:
            res = subprocess.run([str(path), flag], capture_output=True, text=True, timeout=10)
            if res.returncode == 0 and res.stdout.strip():
                return res.stdout.splitlines()[0].strip()
        except Exception:
            continue
    return "unknown"
