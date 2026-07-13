"""Мутационный гейт для PYTHON-половины: тест агента обязан уметь провалиться и здесь.

Rust-код агента проверяет `cargo mutants`, а `tools/clave-dev/` не проверял НИЧТО — то есть в
собственном инструменте оставалась ровно та дыра, ради которой гейт и заведён: агент правит
супервайзер, пишет к правке `assertTrue(True)`, набор зелёный, отчёт хвалит «добавлено тестов: 1».
Пробел архитектурный, а не мелкий: половина проекта жила без доказательств.

Механизм тот же, что у правила 1, только цель другая. Правило 1 обезвреживает ГЕЙТЫ (список
фиксирован, пишу его я). Здесь обезвреживаются ФУНКЦИИ, КОТОРЫЕ ДОБАВИЛ АГЕНТ — список берётся из
диффа, и заранее его никто не знает.

ГРАНИЦА, и она честная. Обезвреживание тут одно: функция начинает возвращать `None`. Это ловит
тест, который НЕ СМОТРИТ на результат, — то есть декорацию. Тест, который смотрит слабо
(`assertIsNotNone`), устоит и здесь: cargo-mutants перебирает валидные подмены (0, true, Default),
а перебирать их в Python — это прогон набора на каждую, и гейт стал бы дороже самой петли.
Дешёвый гейт, который ловит главное, лучше дорогого, который выключат.

Как и в Rust: блокируем ТОЛЬКО добавленные функции. Требовать «ноль выживших» на всём пакете —
непроходимый гейт, а непроходимый гейт выключают первым.
"""
from __future__ import annotations

import re
import subprocess
import sys
from collections import namedtuple
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

PyMutant = namedtuple("PyMutant", "module function")

# Заголовок файла в диффе: «+++ b/clave_dev/loop.py», а для не-ASCII имени — «+++ "b/clave_dev/…"»
# (кавычка стоит ПЕРЕД `b/`, поэтому «^\+\+\+ b/» такой файл не видит вовсе).
_FILE = re.compile(r"^\+\+\+ (.+)$", re.M)
# «+def unproven(...)» / «+    def tests_added(...)» / «+async def ...»
_ADDED_DEF = re.compile(r"^\+\s*(?:async\s+)?def\s+(\w+)", re.M)


def _unquote(path: str) -> str:
    """Путь из диффа, каким бы git его ни выдал.

    С `core.quotepath` (умолчание git) не-ASCII имя приезжает в кавычках и octal-escape'ах:
    `"b/clave_dev/\\320\\277.py"`. Парсер, который этого не знает, ПРОПУСКАЕТ такой файл молча —
    и гейт на нём слеп: код есть, проверок нет, отчёт зелёный. Свой дифф мы теперь просим с
    `core.quotepath=false`, но полагаться на это одно нельзя: дифф может прийти и со стороны.
    """
    if not (path.startswith('"') and path.endswith('"')):
        return path
    body, out, i = path[1:-1], bytearray(), 0
    while i < len(body):
        if body[i] == "\\" and body[i + 1 : i + 4].isdigit():
            out.append(int(body[i + 1 : i + 4], 8))
            i += 4
        elif body[i] == "\\":
            out.append(ord(body[i + 1]))
            i += 2
        else:
            out.extend(body[i].encode())
            i += 1
    return out.decode("utf-8", "replace")


def _module_of(path: str):
    """`b/tools/clave-dev/clave_dev/loop.py` → clave_dev.loop (не пакет — None)."""
    path = _unquote(path)
    if path.startswith("b/"):
        path = path[2:]
    marker = "tools/clave-dev/clave_dev/"
    if not path.startswith(marker) or not path.endswith(".py"):
        return None
    stem = path[len(marker):-len(".py")]
    if stem.startswith("_"):  # __init__ и приватные точки входа не мутируем
        return None
    return "clave_dev." + stem.replace("/", ".")


def added_functions(diff_text: str) -> list:
    """Функции, которые агент ДОБАВИЛ в python-пакет супервайзера.

    Тесты сознательно НЕ мутируем: подменить тест и увидеть, что набор покраснел, — это доказать,
    что тест сам себя проверяет. Бессмыслица. Мутировать надо КОД, который тесты стерегут.
    """
    found = []
    chunks = re.split(r"^diff --git ", diff_text or "", flags=re.M)
    for chunk in chunks:
        path = _FILE.search(chunk)
        if not path:
            continue
        module = _module_of(path.group(1))
        if not module:
            continue
        for name in _ADDED_DEF.findall(chunk):
            if name.startswith("_") and not name.startswith("__"):
                continue  # приватные помощники проверяются через своих публичных вызывающих
            found.append(PyMutant(module, name))
    return found


def _survives(pkg: Path, mutant: PyMutant, python: str) -> bool:
    """Набор ЗЕЛЁНЫЙ с обезвреженной функцией → её не проверяет ни один тест."""
    res = subprocess.run(
        [python, str(pkg / "scripts" / "neuter.py"), f"{mutant.module}:{mutant.function}"],
        cwd=str(pkg),
        capture_output=True,
        text=True,
        check=False,
    )
    return res.returncode == 1  # 0 — набор заметил, 1 — выжил, 2 — функции нет/поломка


def unproven(pkg: Path, diff_text: str, python: str = None) -> list:
    """Добавленные функции, чьё обезвреживание набор НЕ ЗАМЕТИЛ. Непусто → тесты ничего не доказали."""
    mutants = added_functions(diff_text)
    if not mutants:
        return []
    python = python or sys.executable
    with ThreadPoolExecutor(max_workers=min(8, len(mutants))) as pool:
        survived = pool.map(lambda m: (m, _survives(Path(pkg), m, python)), mutants)
    return [m for m, alive in survived if alive]


def describe(mutants: list) -> list:
    """Строки для промпта агента: что именно осталось недоказанным."""
    return [
        f"{m.module}:{m.function} — обезвредил, набор остался зелёным: тест не смотрит на результат"
        for m in mutants
    ]
