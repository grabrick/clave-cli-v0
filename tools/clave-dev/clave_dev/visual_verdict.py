"""Структурированный вердикт зрения и его нормализация в формальный pass (спека §6)."""
from __future__ import annotations

import json
from collections import Counter, namedtuple

Issue = namedtuple("Issue", "description severity region_hint source")
ChecklistItem = namedtuple("ChecklistItem", "check required passed note")
VisionVerdict = namedtuple("VisionVerdict", "issues checklist open_critique raw")

SEVERITIES = ("low", "medium", "high")

# Пункт чеклиста, которым помечается НЕ дефект рендера, а поломка самого визуального прохода
# (нет Screen Recording, пустой кадр, недоступен бэкенд зрения). Отличать обязательно: такой
# пункт нельзя вычитать по базовой линии — иначе сломанный одинаково и на базе, и на фреше
# гейт зрения тихо превратился бы в pass.
INFRA_FAILURE = "визуальный проход не состоялся"


class VerdictParseError(ValueError):
    pass


class BaselineUnavailableError(RuntimeError):
    """Базовую линию рендера снять не удалось — гейтить зрением нечем."""


def parse_verdict(data: dict, raw: str = "") -> VisionVerdict:
    """dict от модели → VisionVerdict с fail-safe дефолтами: неизвестный severity → high,
    отсутствующий required → True, отсутствующий passed → False (никогда не в пользу pass)."""
    issues = []
    for i in data.get("issues", []) or []:
        sev = i.get("severity")
        issues.append(Issue(
            description=i.get("description", ""),
            severity=sev if sev in SEVERITIES else "high",
            region_hint=i.get("region_hint"),
            source=i.get("source", "open"),
        ))
    checklist = []
    for c in data.get("checklist_results", []) or []:
        checklist.append(ChecklistItem(
            check=c.get("check", ""),
            required=bool(c.get("required", True)),
            passed=bool(c.get("passed", False)),
            note=c.get("note", ""),
        ))
    return VisionVerdict(issues, checklist, data.get("open_critique", ""), raw or json.dumps(data))


def extract_verdict_json(text: str) -> dict:
    """Достаёт JSON-объект из ответа модели (часто обёрнут прозой/```json). Берёт от
    первой '{' до последней '}'. Не распарсилось → VerdictParseError (→ блок, не тихий pass)."""
    start, end = text.find("{"), text.rfind("}")
    if start == -1 or end == -1 or end < start:
        raise VerdictParseError("в ответе модели не найден JSON-объект")
    try:
        return json.loads(text[start : end + 1])
    except json.JSONDecodeError as e:
        raise VerdictParseError(f"битый JSON вердикта: {e}") from e


def verdict_passes(v: VisionVerdict, blocking=("high", "medium")) -> bool:
    """Формальный pass супервайзера (спека §6):
    (1) любой required-пункт чеклиста с passed=false → блок безусловно;
    (2) любой issue с блокирующим severity → блок.
    (1) закрывает дыру 'required упал, но issue low/нет'."""
    if any(item.required and not item.passed for item in v.checklist):
        return False
    if any(issue.severity in blocking for issue in v.issues):
        return False
    return True


def blocking_verdict(reason: str) -> VisionVerdict:
    """Вердикт-блокиратор: один проваленный required-пункт → verdict_passes=False.

    Пункт называется INFRA_FAILURE, а не текстом причины: так регрессионный гейт отличает
    «сломался сам проход» от «сломан рендер» и не вычитает первое по базовой линии. Причина
    едет в note — она же уходит агенту через build_visual_context.
    """
    return VisionVerdict(
        issues=[],
        checklist=[ChecklistItem(check=INFRA_FAILURE, required=True, passed=False, note=reason)],
        open_critique=reason,
        raw=reason,
    )


def is_infra_failure(v: VisionVerdict) -> bool:
    """Вердикт говорит не о продукте, а о поломке самого визуального прохода."""
    return any(c.check == INFRA_FAILURE and not c.passed for c in v.checklist)


def _key(check: str) -> str:
    """Ключ пункта чеклиста.

    Нормализация обязательна: модель возвращает имя пункта то с заглавной, то со строчной
    («Нет обрезанных глифов» / «нет обрезанных глифов») — замерено на живых прогонах. Сравнение
    строк «как есть» их бы не схлопнуло, и дефект, который был и на базе, приехал бы к агенту
    как «новая регрессия».
    """
    return check.strip().casefold()


def failed_required_keys(v: VisionVerdict) -> set:
    """Ключи проваленных required-пунктов (поломки самого прохода сюда не входят)."""
    return {
        _key(c.check)
        for c in v.checklist
        if c.required and not c.passed and c.check != INFRA_FAILURE
    }


def baseline_keys(samples) -> set:
    """Всё, что судья хоть раз забраковал на сборке ДО правок — щедро, объединением.

    Судья невоспроизводим: пять прогонов на одном и том же здоровом продукте дали ТРИ разных
    вердикта. Поэтому база берётся объединением выборок: если модель СКЛОННА видеть этот дефект
    на здоровом продукте, блокировать им агента нельзя — он его не вносил и починить не может
    (полый курсор в неактивном окне рисует сам Terminal, а фокус наблюдатель не крадёт).
    """
    keys = set()
    for v in samples:
        keys |= failed_required_keys(v)
    return keys


def consensus_verdict(samples, base_keys=(), min_hits: int = 2) -> VisionVerdict:
    """Свести N выборок одного и того же кадра в один вердикт.

    Required-пункт считается РЕГРЕССИЕЙ, только если он упал не меньше чем в `min_hits` выборках
    И не значится в базовой линии. Одна выборка гейтить не может — это подбрасывание монеты
    (замерено). Порог по совпадениям гасит одиночный шум судьи, база — его системную склонность.

    Поломку самого прохода (INFRA_FAILURE) не гасим ничем: сломанный гейт обязан блокировать,
    а не совпасть сам с собой в ноль и тихо превратиться в pass.
    """
    samples = list(samples)
    if not samples:
        raise ValueError("нет ни одной выборки вердикта")
    infra = next((v for v in samples if is_infra_failure(v)), None)
    if infra is not None:
        return infra
    if len(samples) < min_hits:
        # Ловушка, которую легко не заметить: если выборок меньше порога совпадений, ни одна
        # регрессия физически не наберёт min_hits — гейт стал бы слепым и молча пропускал всё.
        # Слепой гейт хуже отсутствующего: он создаёт видимость проверки.
        return blocking_verdict(
            f"выборок вердикта {len(samples)}, а порог совпадений {min_hits} — "
            "гейт не отличил бы шум судьи от регрессии"
        )

    hits = Counter()
    for v in samples:
        hits.update(failed_required_keys(v))
    base = set(base_keys)
    regressed = {k for k, n in hits.items() if n >= min_hits and k not in base}

    template = samples[0]
    checklist = []
    for c in template.checklist:
        if not c.required:
            checklist.append(c)
            continue
        key = _key(c.check)
        passed = key not in regressed
        note = c.note
        if not c.passed and passed:
            note = f"{c.note} [{hits[key]}/{len(samples)} выборок; не регрессия]".strip()
        checklist.append(c._replace(passed=passed, note=note))

    # Регрессия, которую первая выборка не назвала, всё равно обязана попасть в вердикт —
    # иначе шум в выборке №1 прятал бы настоящую поломку.
    present = {_key(c.check) for c in checklist}
    for key in sorted(regressed - present):
        checklist.append(
            ChecklistItem(
                check=key, required=True, passed=False, note=f"{hits[key]}/{len(samples)} выборок"
            )
        )

    issues = [i for v in samples for i in v.issues]  # мнения — справочно, все
    return template._replace(checklist=checklist, issues=issues)


def severities_at_or_above(threshold: str) -> tuple:
    """Блокирующий набор severity из порога: 'medium' → ('medium','high'), 'high' → ('high',).

    Особый порог `none` → пустой набор: тогда блокирует ТОЛЬКО провал required-чеклиста
    (объективные дефекты рендеринга), а находки и открытая критика модели идут агенту
    СПРАВОЧНО. Это защита автономной петли: модель охотно выдаёт эстетические мнения
    («логотип тусклый», «разделитель на колонку короче»), и если ими гейтить сходимость,
    агент будет жечь раунды, «починяя» несломанное."""
    if threshold == "none":
        return ()
    if threshold not in SEVERITIES:
        raise ValueError(f"неизвестный severity-порог: {threshold}")
    return SEVERITIES[SEVERITIES.index(threshold):]
