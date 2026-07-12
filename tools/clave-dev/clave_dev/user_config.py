"""Настройки продукта для прогона: изолируем СОСТОЯНИЕ, но не подменяем ПОВЕДЕНИЕ.

CLAVE_HOME мы уводим в temp, чтобы прогон не лез в реальные чаты и конфиг пользователя. У этой
изоляции обнаружилась цена: пустой home → конфига нет → продукт берёт ДЕФОЛТЫ, а дефолтный
режим — `codex-only`, где исполнитель и критик оказываются ОДНОЙ И ТОЙ ЖЕ моделью. Тандем
вырождается в самокритику, хотя ярлык по-прежнему пишет «tandem» и никто не замечает. Заодно
терялись effort'ы и тема — то есть зрение судило рендер, которого пользователь никогда не видит.

Лечится без единой правки в продукте: `CLAVE_CONFIG` перекрывает путь к конфигу НЕЗАВИСИМО от
`CLAVE_HOME` (storage.rs:133). Состояние остаётся изолированным, поведение — таким, как настроил
человек.
"""
from __future__ import annotations

import os
from pathlib import Path
from typing import Optional

# Указывает в чаты, которых в изолированном home нет — тащить бессмысленно.
DROP_KEYS = ("last_chat",)

# Режимы, где обе роли достаются одной модели.
SINGLE_MODEL_MODES = {"codex-only", "claude-only"}


def user_config_path(environ=None) -> Path:
    """Где лежит НАСТОЯЩИЙ конфиг пользователя — до того, как мы подменим CLAVE_HOME.

    Порядок тот же, что в продукте: CLAVE_CONFIG → CLAVE_HOME/config → ~/.clave/config.
    """
    env = os.environ if environ is None else environ
    explicit = env.get("CLAVE_CONFIG")
    if explicit:
        return Path(explicit)
    home = env.get("CLAVE_HOME") or str(Path.home() / ".clave")
    return Path(home) / "config"


def seed_config(src: Path, dest_dir: Path) -> Optional[Path]:
    """Положить настройки пользователя в прогон, отбросив ссылки на его состояние.

    Возвращает путь к копии или None, если у пользователя конфига нет.
    """
    src = Path(src)
    if not src.is_file():
        return None
    kept = [
        line
        for line in src.read_text().splitlines()
        if line.split("=", 1)[0].strip() not in DROP_KEYS
    ]
    dest = Path(dest_dir) / "config"
    dest.write_text("\n".join(kept) + "\n")
    return dest


def config_mode(path: Path) -> Optional[str]:
    """Значение `mode` из конфига (None — ключа нет или файл не читается)."""
    try:
        lines = Path(path).read_text().splitlines()
    except OSError:
        return None
    for line in lines:
        key, _, value = line.partition("=")
        if key.strip() == "mode":
            return value.strip().strip('"') or None
    return None


def single_model_warning(mode: Optional[str]) -> Optional[str]:
    """Предупреждение, если «тандем» на деле критикует сам себя.

    Молчать тут нельзя: прогон выглядит и называется тандемом, но второго, НЕЗАВИСИМОГО взгляда
    в нём нет — а вся посылка тандема именно в нём.
    """
    if mode in SINGLE_MODEL_MODES:
        return (
            f"режим '{mode}': исполнитель и критик — одна и та же модель. Тандем вырождается "
            "в самокритику, независимого второго взгляда не будет."
        )
    return None
