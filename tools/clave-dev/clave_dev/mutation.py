"""Мутационный гейт: тест агента обязан уметь провалиться.

Дыра, которую он закрывает, — зеркало всех прочих, только этажом выше. Мы доказали, что НАШИ
гейты умеют падать. Но тесты, которыми гейтится работа агента, пишет САМ АГЕНТ, — и `cargo test:
124 passed` не отличает тест, который кусается, от теста вида:

    #[test]
    fn git_indicator_works_correctly() {
        assert!(true || false);   // не умеет упасть НИКОГДА
    }

Замерено: такой тест проходит cargo, clippy и fmt, поднимает счётчик тестов — а отчёт про него
пишет «добавлено тестов: 1», то есть ХВАЛИТ. Отсутствие проверки снова читается как проверка.

Лечение — та же техника, что в prove_gate.py, только для продукта: мутируем изменённые строки и
смотрим, заметит ли это хоть один тест. Выживший мутант = строка, которую тесты не доказывают.

БЛОКИРУЕМ ТОЛЬКО НОВЫЙ КОД. Функции, которые агент ДОБАВИЛ, обязаны быть покрыты. Старые
непокрытые функции (а их в любом живом проекте полно — замерено: 29 выживших мутантов на том, что
уже уехало в прод) агент не писал, и требовать «ноль выживших» значит сделать гейт непроходимым.
Гейт, который невозможно пройти, — та же болезнь, что гейт, который невозможно провалить: и то и
другое кончается тем, что его отключают.
"""
from __future__ import annotations

import re
import shutil
from collections import namedtuple

Mutant = namedtuple("Mutant", "file line description function")

# «+pub(crate) fn git_slot_cost(...)» / «+    fn segments() -> ...» / «+async fn ...»
_ADDED_FN = re.compile(
    r"^\+\s*(?:pub(?:\([\w:]+\))?\s+)?(?:const\s+)?(?:async\s+)?(?:unsafe\s+)?fn\s+(\w+)",
    re.M,
)

# «MISSED   src/ui/footer.rs:268:5: replace git_slot_cost -> usize with 0 in 0s build + 0s test»
_MISSED = re.compile(r"^MISSED\s+(\S+?):(\d+):\d+:\s+(.+)$", re.M)

# Хвост cargo-mutants: « in 0s build + 0s test». Мешает вычислению имени функции.
_TAIL = re.compile(r"\s+in \d+\.?\d*s build \+ \d+\.?\d*s test\s*$")


def added_functions(diff_text: str) -> set:
    """Имена функций, которые агент ДОБАВИЛ в этой правке."""
    return set(_ADDED_FN.findall(diff_text or ""))


def _function_of(description: str) -> str:
    """Из описания мутации вытащить имя функции.

    Два вида, и оба надо разобрать:
      «replace git_slot_cost -> usize with 0»  → git_slot_cost
      «replace + with - in git_slot_cost»      → git_slot_cost
    """
    tail = re.search(r"\bin ([\w:]+)$", description)
    if tail:
        return tail.group(1).split("::")[-1]
    head = re.match(r"replace ([\w:]+)", description)
    if head:
        return head.group(1).split("::")[-1]
    return ""


def parse_missed(output: str) -> list:
    """Выжившие мутанты из вывода cargo-mutants."""
    found = []
    for file, line, rest in _MISSED.findall(output or ""):
        description = _TAIL.sub("", rest).strip()
        found.append(Mutant(file, int(line), description, _function_of(description)))
    return found


def unproven(diff_text: str, mutants_output: str) -> list:
    """Мутанты, выжившие В КОДЕ, КОТОРЫЙ АГЕНТ ДОБАВИЛ. Непусто → тесты агента ничего не доказали."""
    new = added_functions(diff_text)
    return [m for m in parse_missed(mutants_output) if m.function in new]


def tested(mutants_output: str) -> int:
    """Сколько мутантов гейт РЕАЛЬНО проверил («N mutants tested in …»).

    Нужно, чтобы правило 2 не врало в другую сторону. Оно считает добавленные `#[test]` и, не найдя
    ни одного, объявляет правку недоказанной. Но агент может ПЕРЕПИСАТЬ существующий тест — тогда
    `#[test]` в диффе не появится, а правка при этом проверена. Живой прогон это и показал: гейт
    сказал «тесты агента кусаются», а отчёт строкой ниже — «правку не проверяет ничто».

    Ложная тревога стоит дорого ровно так же, как ложное «сошлось»: правило, которое кричит всегда,
    перестают читать, а потом отключают. Ноль здесь означает «гейт ничего не проверил», а не
    «проверил и всё чисто».
    """
    match = re.search(r"^(\d+) mutants? tested", mutants_output or "", re.M)
    return int(match.group(1)) if match else 0


def mutants_cmd(patch_path) -> list:
    """Команда мутационного прогона. Только по диффу: полный прогон — часы, по диффу — минуты."""
    return ["cargo", "mutants", "--in-diff", str(patch_path), "--no-shuffle"]


def mutation_preflight() -> str:
    """Причина, по которой гейт не сможет работать, или пустая строка.

    Молча пропустить гейт нельзя: отсутствие проверки прочтётся как пройденная проверка — ровно
    та болезнь, от которой этот гейт и лечит. Нет инструмента — отказываемся стартовать.
    """
    if shutil.which("cargo-mutants") or shutil.which("cargo_mutants"):
        return ""
    return (
        "cargo-mutants не установлен (cargo install cargo-mutants). "
        "Без него тест агента нечем проверить на «умеет ли он упасть». "
        "Запусти с --no-mutants, если осознанно отказываешься от этой проверки."
    )


def describe(mutants: list) -> list:
    """Строки для агента: что именно его тесты не доказывают."""
    return [
        f"{m.file}:{m.line} — мутация «{m.description}» ВЫЖИЛА: ни один тест её не заметил"
        for m in mutants
    ]
