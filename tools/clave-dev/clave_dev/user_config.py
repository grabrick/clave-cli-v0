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


def config_value(path: Path, name: str) -> Optional[str]:
    """Значение ключа из конфига продукта (None — ключа нет или файл не читается)."""
    try:
        lines = Path(path).read_text().splitlines()
    except OSError:
        return None
    for line in lines:
        key, _, value = line.partition("=")
        if key.strip() == name:
            return value.strip().strip('"') or None
    return None


def config_mode(path: Path) -> Optional[str]:
    """Значение `mode` из конфига (None — ключа нет или файл не читается)."""
    return config_value(path, "mode")


def cost_warning(rounds: Optional[str], effort: Optional[str]) -> Optional[str]:
    """Во что человек ввязывается, ЕЩЁ ДО того как ждать.

    Прогон тандема законно идёт десятками минут: `rounds=2` — это ДВА полных цикла
    «исполнитель → критик», каждый на своём effort. Замерено: на задаче в 50 строк тандем спорил
    около двух часов.

    Дефект был не в длительности, а в молчании о ней. Человек видел «раунд 1: агент правит код» на
    неподвижном экране и не мог понять, отойти ему за кофе или на полдня. Инструмент обязан назвать
    свою цену заранее — как называет всё остальное, чего он не проверял.
    """
    debate = int(rounds) if rounds and rounds.isdigit() else 1
    if debate <= 1 and effort in (None, "low", "medium"):
        return None
    parts = [f"дебатов: {debate}"]
    if effort:
        parts.append(f"effort: {effort}")
    return (
        f"тандем настроен щедро ({', '.join(parts)}) — прогон легко займёт десятки минут, а то и "
        "часы. Дешевле: --rounds 1 --effort medium"
    )


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
