"""Структурированный вердикт зрения и его нормализация в формальный pass (спека §6)."""
from __future__ import annotations

import json
from collections import namedtuple

Issue = namedtuple("Issue", "description severity region_hint source")
ChecklistItem = namedtuple("ChecklistItem", "check required passed note")
VisionVerdict = namedtuple("VisionVerdict", "issues checklist open_critique raw")

SEVERITIES = ("low", "medium", "high")


class VerdictParseError(ValueError):
    pass


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
