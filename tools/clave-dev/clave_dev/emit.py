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


class Emitter:
    """enabled=False → no-op (standalone-CLI Фазы 1/2 не засоряется). enabled=True →
    печатает обрамлённые строки в out (stdout по умолчанию)."""

    def __init__(self, enabled: bool, out=None):
        self.enabled = enabled
        self._out = out if out is not None else sys.stdout

    def emit(self, type_: str, payload) -> None:
        if not self.enabled:
            return
        print(format_line(type_, payload), file=self._out, flush=True)

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


def no_op_emitter() -> Emitter:
    return Emitter(enabled=False)
