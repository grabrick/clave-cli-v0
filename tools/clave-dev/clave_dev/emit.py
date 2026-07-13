"""Типизированный протокол прогресса CLAVE-DEV <type> <payload> для TUI (спека §5)."""
from __future__ import annotations

import json
import time
import sys

EMIT_TYPES = ("progress", "log", "check", "vision", "diff", "report", "error")


def format_line(type_: str, payload) -> str:
    """Одна обрамлённая строка. Текст для progress/log/error, JSON для check/vision/diff/report.

    В JSON кладём ещё и `human` — готовые строки для показа человеку.

    Зачем: TUI рисовал payload СЫРЫМ JSON-ом («✓ {"name":"test","ok":false,...}»), да ещё и с
    иконкой по ТИПУ события, а не по результату — упавшая проверка получала зелёную галочку.
    Правило 2 при этом формально соблюдалось (строки «не проверено машиной» в отчёте были), но
    человек их не читал: они тонули в дампе. Правило, которое не дочитывают, — декорация.

    Рендер намеренно ОДИН и живёт здесь. Вторая реализация в Rust уже разъехалась бы с этой: в
    терминал failures печатались, а в TUI про них забыли.
    """
    if type_ not in EMIT_TYPES:
        raise ValueError(f"неизвестный тип события: {type_}")
    if type_ in ("progress", "log", "error"):
        body = payload if isinstance(payload, str) else json.dumps(payload, ensure_ascii=False)
    else:
        body = json.dumps({**payload, "human": human_lines(type_, payload)}, ensure_ascii=False)
    return f"CLAVE-DEV {type_} {body}"


def human_lines(type_: str, payload) -> list:
    """Событие → строки для человека (пустой список — показывать нечего)."""
    if type_ == "log":
        return [payload]  # собственный вывод агента — отдаём как есть, без украшений
    if type_ == "progress":
        return [f"· {payload}"]
    if type_ == "error":
        return [f"✗ {payload}"]

    if type_ == "check":
        mark = "✓" if payload.get("ok") else "✗"
        detail = payload.get("detail")
        head = f"  {mark} {payload.get('name')}" + (f" — {detail}" if detail else "")
        # Счётчик не объясняет провала. Раньше «✗ test — 1 failed» было всё, что видел человек.
        lines = [head] + [f"      · {f}" for f in payload.get("failures") or ()]
        hidden = payload.get("failures_truncated")
        if hidden:
            lines.append(f"      · … и ещё {hidden}")
        return lines

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
        return lines

    if type_ == "diff":
        # Ключи — из build_diff, а не из головы. Первая версия читала `files`/`patch`, которых там
        # отродясь не было, и живой прогон показал «± правок нет» строкой ниже «изменено файлов: 1».
        # Тест не поймал, потому что проверял выдуманный payload; теперь он гоняет настоящий diff.
        files = payload.get("changed_files") or []
        if not files:
            return ["  ± правок нет"]
        # У `git diff --stat` последняя строка — сводка («1 file changed, 17 insertions(+)…»).
        # Само полотно не показываем: файлы уже названы в progress, а на 200 файлах это простыня.
        stat = (payload.get("stat") or "").strip().splitlines()
        summary = stat[-1].strip() if stat else f"файлов: {len(files)}"
        lines = [f"  ± {summary}"]
        if payload.get("truncated"):
            lines.append(f"      · в списке первые {len(files)}")
        lines.append(f"      · патч: {payload.get('patch_path', '—')}")
        return lines

    if type_ == "report":
        # Самое важное событие прогона: здесь живёт правило 2. В TUI оно и приезжает — раньше
        # только как поле внутри JSON-дампа, то есть фактически не приезжало.
        mark = "⏺" if payload.get("converged") else "✗"
        head = (
            f"{mark} {payload.get('status')} — раундов: {payload.get('rounds')}"
            f"/{payload.get('max_rounds')}"
        )
        lines = [head, f"  worktree: {payload.get('worktree')}"]
        # «Сошлось» не имеет права ехать в одиночку.
        lines += [f"  ⚠ {item}" for item in payload.get("unverified") or ()]
        return lines

    return []


def human_line(type_: str, payload):
    """Совместимость: те же строки, склеенные. None — показывать нечего."""
    lines = human_lines(type_, payload)
    return "\n".join(lines) if lines else None


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

    def __init__(self, enabled: bool, out=None, human_out=None, clock=None):
        self.enabled = enabled
        self._out = out if out is not None else sys.stdout
        self._human = human_out if human_out is not None else sys.stderr
        # Часы инъекцией: тест не должен зависеть от настоящего времени, а прогон — печатать
        # стенные часы, которые в логе ничего не значат.
        self._clock = clock or time.monotonic
        self._t0 = self._clock()

    def _elapsed(self) -> str:
        """Сколько идёт прогон. Стадия без времени не говорит, ждать пять минут или два часа.

        Дефект был не в том, что прогон долгий, а в том, что он МОЛЧИТ о своей цене: человек видит
        «раунд 1: агент правит код» и не знает, отойти ему за кофе или на два часа. Тандем с
        `rounds=2` и `effort=high` крутит два полных цикла «исполнитель → критик», и это законно
        занимает десятки минут — но узнавать об этом, глядя в неподвижный экран, нельзя.
        """
        total = int(self._clock() - self._t0)
        return f"[{total // 60:d}:{total % 60:02d}]"

    # В терминале финальный отчёт печатает render_report (в stdout), а дифф — это патч целиком,
    # ему место в файле. Эмиттер их пропускает, иначе человек увидит отчёт дважды. В protocol-mode
    # оба нужны: там render_report не зовут, и отчёт до TUI доедет только событием.
    HUMAN_PRINTS_ITSELF = ("report", "diff")

    def emit(self, type_: str, payload) -> None:
        if self.enabled:
            print(format_line(type_, payload), file=self._out, flush=True)
            return
        if type_ in self.HUMAN_PRINTS_ITSELF:
            return
        lines = human_lines(type_, payload)
        if not lines:
            return
        # Стадии помечаем временем от старта. Гейты (check/vision) — нет: они идут пачкой сразу
        # после стадии, и часы на каждой строке превратились бы в шум.
        if type_ == "progress":
            lines[0] = f"{self._elapsed()} {lines[0]}"
        print("\n".join(lines), file=self._human, flush=True)

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
