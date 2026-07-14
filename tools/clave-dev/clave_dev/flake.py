"""Набор, который падает ВРАЗБРОС, врёт мутационному гейту.

`cargo mutants` судит мутанта по цвету набора: покраснел — значит «мутант пойман». ПОЧЕМУ
покраснел, ему всё равно. Флейкующий тест красит набор сам по себе — и НЕпойманный мутант
записывается в пойманные. Гейт начинает врать, причём в самую опасную сторону: «покрыто» там,
где не покрыто ничем.

Это та же семейная болезнь проекта — ОТСУТСТВИЕ ПРОВЕРКИ ЧИТАЕТСЯ КАК ПРОЙДЕННАЯ ПРОВЕРКА, —
только теперь она научилась подделывать цифру. И подделывает молча: гейт рапортует «тесты
агента кусаются», а тесты не кусаются, просто рядом мигнул чужой.

Не гипотеза, а замер (2026-07-14, src/runtime.rs, 209 мутантов):

    с флейком:   104 выживших, 81 пойман
    без флейка:  129 выживших, 56 пойман   ← 25 дыр гейт объявил закрытыми

Одиночный зелёный `cargo test` не значит ничего: оба пойманных тогда флейка были зелёными
25 прогонов подряд и разваливались только под ПАРАЛЛЕЛЬНОЙ нагрузкой — когда наборы делят
машину и общий /tmp. Ровно её и создаёт `cargo mutants`, гоняя набор снова и снова.

ГРАНИЦА, ЧЕСТНО: бьём РАСТОВЫЙ набор — тот самый, по которому судит `cargo mutants`. Питонову
половину (её судит `mutation_py`) не бьём: один её прогон стоит полминуты, и дюжина прогонов
сделала бы гейт дороже правки, которую он охраняет.
"""
from __future__ import annotations

import json
import re
import subprocess
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

_FAILED = re.compile(r"^test (\S+) \.\.\. FAILED", re.M)


class SuiteNotBuilt(RuntimeError):
    """Тестовый бинарь не собрался — устойчивость проверить нечем.

    Ответить на это «набор устойчив» нельзя ни при каких обстоятельствах: это и есть отсутствие
    проверки, прочитанное как пройденная проверка. Лучше уронить прогон.
    """


def failed_tests(output: str) -> set:
    """ИМЕНА упавших тестов — не их число.

    «1 failed» не говорит, КТО именно упал, а чинить надо конкретный тест. Считать по счётчику
    заодно и опасно: два разных теста, мигнувших в двух прогонах, дали бы «по одному падению»,
    и разницы с «один и тот же тест дважды» не осталось бы никакой.
    """
    return set(_FAILED.findall(output))


def test_binaries(worktree: Path, env: dict) -> list:
    """Пути к уже собранным тестовым бинарям.

    Гоняем их НАПРЯМУЮ, а не через `cargo test`: два параллельных `cargo test` в одном дереве
    встают в очередь за блокировкой `target/` и выполняются по очереди. Нагрузки они не создают,
    гонок не вскрывают — то есть проверка была бы декорацией.
    """
    res = subprocess.run(
        ["cargo", "test", "--no-run", "--message-format=json"],
        cwd=str(worktree),
        env=env,
        capture_output=True,
        text=True,
        check=False,
    )
    found = []
    for line in res.stdout.splitlines():
        try:
            msg = json.loads(line)
        except ValueError:
            continue
        if msg.get("reason") != "compiler-artifact" or not msg.get("executable"):
            continue
        if msg.get("profile", {}).get("test"):
            found.append(msg["executable"])
    return found


def _run_once(binary: str, worktree: Path, env: dict) -> str:
    res = subprocess.run(
        [binary], cwd=str(worktree), env=env, capture_output=True, text=True, check=False
    )
    return res.stdout + res.stderr


def unstable(
    worktree: Path, env: dict, rounds: int = 3, parallel: int = 4, run=None, find=None
) -> list:
    """Тесты, упавшие хотя бы раз под параллельной нагрузкой. Пусто = набор устойчив.

    Сюда приходят ТОЛЬКО после зелёного `cargo test` — значит, любое падение здесь и есть флейк,
    а не честно сломанный тест. Это и делает вердикт однозначным: разбираться, «настоящее» ли
    падение, не нужно.

    `run` и `find` — швы ради тестов: чем прогнать один бинарь и где их взять. Без них проверить
    САМ ЭТОТ ГЕЙТ можно было бы только настоящим флейкующим cargo-проектом, то есть никак — и он
    остался бы недоказуемым. А гейт, который нельзя провалить, — не гейт, а декорация; ровно то,
    от чего лечит весь этот инструмент.
    """
    run = _run_once if run is None else run
    find = test_binaries if find is None else find

    binaries = find(worktree, env)
    if not binaries:
        # Молча вернуть «устойчив» нельзя ни при каких обстоятельствах: это и есть отсутствие
        # проверки, прочитанное как пройденная проверка.
        raise SuiteNotBuilt(
            "тестовый бинарь не собран — устойчивость набора проверить нечем; "
            "мутационному гейту в таком состоянии верить нельзя"
        )

    shots = [b for b in binaries for _ in range(max(1, parallel))]
    bad = set()
    for _ in range(max(1, rounds)):
        with ThreadPoolExecutor(max_workers=len(shots)) as pool:
            outputs = list(pool.map(lambda b: run(b, worktree, env), shots))
        for out in outputs:
            bad |= failed_tests(out)
    return sorted(bad)


def describe(flaky: list) -> list:
    """Строки агенту: что именно чинить и почему это не «просто мигает»."""
    return [
        f"{name} — падает ВРАЗБРОС под параллельной нагрузкой (одиночный прогон зелёный)"
        for name in flaky
    ]
