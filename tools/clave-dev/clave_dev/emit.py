"""Типизированный протокол прогресса CLAVE-DEV <type> <payload> для TUI (спека §5)."""
from __future__ import annotations

import json
import sys

EMIT_TYPES = ("progress", "log", "check", "vision", "diff", "report", "error")


def format_line(type_: str, payload) -> str:
    """Одна обрамлённая строка. Текст для progress/log/error, JSON для check/vision/diff/report."""
    if type_ not in EMIT_TYPES:
        raise ValueError(f"неизвестный тип события: {type_}")
    if type_ in ("progress", "log", "error"):
        body = payload if isinstance(payload, str) else json.dumps(payload, ensure_ascii=False)
    else:
        body = json.dumps(payload, ensure_ascii=False)
    return f"CLAVE-DEV {type_} {body}"


def human_line(type_: str, payload):
    """Событие → строка для человека. None — показывать нечего.

    `report` и `diff` пропускаем сознательно: финальный отчёт человеку печатает render_report,
    а дифф — это весь патч целиком, ему место в файле, а не в терминале.
    """
    if type_ == "log":
        return payload  # собственный вывод агента — отдаём как есть, без украшений
    if type_ == "progress":
        return f"· {payload}"
    if type_ == "error":
        return f"✗ {payload}"
    if type_ == "check":
        mark = "✓" if payload.get("ok") else "✗"
        detail = payload.get("detail")
        head = f"  {mark} {payload.get('name')}" + (f" — {detail}" if detail else "")
        # Счётчик не объясняет провала. Раньше «✗ test — 1 failed» было всё, что видел человек.
        lines = [head] + [f"      · {f}" for f in payload.get("failures") or ()]
        hidden = payload.get("failures_truncated")
        if hidden:
            lines.append(f"      · … и ещё {hidden}")
        return "\n".join(lines)
    if type_ == "vision":
        mark = "✓" if payload.get("pass") else "✗"
        lines = [
            f"  {mark} зрение — регрессий: {payload.get('regressions', 0)}, "
            f"находок: {payload.get('issues', 0)}"
        ]
        # Одни счётчики не объясняют, ПОЧЕМУ зрение заблокировало прогон: перечень забракованного
        # раньше уезжал только в промпт агента. Пустые списки → строка ровно как прежде.
        lines += [f"      ✗ чеклист: {item}" for item in payload.get("failed_required") or ()]
        lines += [f"      • {item}" for item in payload.get("findings") or ()]
        return "\n".join(lines)
    return None


class Emitter:
    """enabled=True → обрамлённые строки CLAVE-DEV в out (их читает TUI).
    enabled=False → человек в терминале: те же события, но читаемым текстом в stderr.

    Немым эмиттер быть не может, а раньше был: при enabled=False emit() просто возвращался, и
    человек, запустивший супервайзер из терминала, не видел НИ ОДНОЙ стадии — ни раунда, ни
    результатов проверок, ни визуального прохода. Только сырой поток агента, минутами подряд.
    Понять, где прогон и жив ли он вообще, было неоткуда.

    Почему stderr: в человеческом режиме stdout занят финальным отчётом, а в protocol-mode обязан
    содержать ТОЛЬКО обрамлённые строки (§5).
    """

    def __init__(self, enabled: bool, out=None, human_out=None):
        self.enabled = enabled
        self._out = out if out is not None else sys.stdout
        self._human = human_out if human_out is not None else sys.stderr

    def emit(self, type_: str, payload) -> None:
        if self.enabled:
            print(format_line(type_, payload), file=self._out, flush=True)
            return
        line = human_line(type_, payload)
        if line is not None:
            print(line, file=self._human, flush=True)

    def progress(self, text):
        self.emit("progress", text)

    def log(self, text):
        self.emit("log", text)

    def check(self, payload):
        self.emit("check", payload)

    def vision(self, payload):
        self.emit("vision", payload)

    def diff(self, payload):
        self.emit("diff", payload)

    def report(self, payload):
        self.emit("report", payload)

    def error(self, text):
        self.emit("error", text)


class _Discard:
    """Сток в никуда. io.StringIO не годится: log() зовётся на КАЖДУЮ строку агента, и весь его
    вывод копился бы в памяти до конца прогона."""

    def write(self, _text) -> int:
        return 0

    def flush(self) -> None:
        pass


def no_op_emitter() -> Emitter:
    """По-настоящему немой — для тестов и вызовов run_loop без эмиттера.

    Просто Emitter(enabled=False) больше не годится: он теперь печатает человеку в stderr.
    """
    return Emitter(enabled=False, human_out=_Discard())
