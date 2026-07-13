#!/usr/bin/env python3
"""Перевыпустить RULES.lock — замок на правилах самопроверки.

Замок нужен от самого тонкого обхода: **выпотрошить правило, не удаляя его**. Файл на месте,
имя на месте, CI зовёт его поимённо и находит — а внутри `GATES = []` или `assertTrue(True)`.
Именно так обходит уставший человек: не сносит защиту, а «временно» вынимает из неё один пункт,
потому что он «флаки».

Замок это не запрещает. Он делает это ГРОМКИМ: любая правка защищённого файла роняет тест, пока
замок не перевыпущен вот этим скриптом — то есть пока в диффе не появится строка «я поменял
правила». Одна строка, которую человек может грепнуть.

Если видишь RULES.lock в диффе — спрашивай зачем. Скорее всего, я буду уверен, что у меня
хорошая причина.
"""
from __future__ import annotations

import hashlib
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LOCK = ROOT / "RULES.lock"

# Всё, что охраняет правила, — и сами правила, и то, чем они проверяются, и CI, который их зовёт.
PROTECTED = [
    "tests/test_gates_can_fail.py",
    "tests/test_unverified.py",
    "tests/test_no_dead_modules.py",
    "tests/test_rules_are_enforced.py",
    "scripts/prove_gate.py",
    "scripts/prove_no_dead_modules.py",
    "clave_dev/unverified.py",
    "../../.github/workflows/dev-rules.yml",
    # Релиз — второй путь в прод: тег собирает бинарь с ЛЮБОЙ ветки, мимо стража main.
    "../../.github/workflows/self-dev-guard.yml",
    "../../dist-workspace.toml",
]

HEADER = """\
# Замок правил самопроверки clave-dev. Не редактировать руками.
#
# Любая правка защищённого файла роняет тест, пока замок не перевыпущен ОСОЗНАННО:
#     python3 scripts/relock_rules.py
#
# Смысл: правило нельзя выпотрошить тихо. Снести можно — но только с этой строкой в диффе.
# Увидел RULES.lock в изменениях — спроси зачем.
"""


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def expected() -> dict:
    return {name: digest(ROOT / name) for name in PROTECTED}


def drifted(locked: dict, actual: dict) -> list:
    """Что изменилось против замка. Чистая функция — её и проверяет тест.

    Отдельно от файлов нарочно: прежний тест доказывал работу замка тем, что ПОРТИЛ настоящий
    файл правила и чинил его в `finally`. А мета-тесты гоняют набор в восьми параллельных
    подпроцессах, и каждый из них тоже запускал этого сторожа — восемь потоков наперегонки
    портили и чинили один файл. Локально везло, CI поймал. Общего изменяемого состояния в тестах
    быть не должно.
    """
    return sorted(name for name, sha in actual.items() if locked.get(name) != sha)


def load() -> dict:
    if not LOCK.is_file():
        return {}
    out = {}
    for line in LOCK.read_text().splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        name, _, sha = line.partition(" ")
        out[name.strip()] = sha.strip()
    return out


def write() -> None:
    lines = [HEADER]
    for name, sha in expected().items():
        lines.append(f"{name} {sha}")
    LOCK.write_text("\n".join(lines) + "\n")


if __name__ == "__main__":
    missing = [n for n in PROTECTED if not (ROOT / n).is_file()]
    if missing:
        print("нечего запирать — эти файлы пропали:", file=sys.stderr)
        for n in missing:
            print(f"  · {n}", file=sys.stderr)
        sys.exit(1)
    write()
    print(f"RULES.lock перевыпущен: {len(PROTECTED)} файлов")
