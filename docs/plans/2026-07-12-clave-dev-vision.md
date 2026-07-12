# clave-dev — Фаза 2 (vision): Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Надстроить над супервайзером Фазы 1 «зрение»: observer снимает реальный рендер Terminal.app и зрячая модель выносит структурированный вердикт о визуальных дефектах, дающий супервайзеру формальный `pass` и агенту — пригодный фидбэк.

**Architecture:** Новые модули в `tools/clave-dev/clave_dev/` поверх готовых `observer/loop/context`. Ядро логики (нормализация вердикта, критерий, разрешение окна) — чистые функции с юнит-тестами здесь; GUI-части (AppleScript/screencapture/Quartz/бэкенд зрения) — тонкие, тестируются конструированием команд + фикстурами; реальный e2e — на машине пользователя.

**Tech Stack:** Python 3.9 stdlib + (для реального прогона) `pyobjc-framework-Quartz` (разрешение CGWindowID) и image-бэкенд зрения. Тесты — stdlib `unittest` + фейки. Spec: `docs/design/2026-07-12-clave-dev-vision.md`.

## Global Constraints

- Никогда не «ложный pass» (§8): неразрешённое окно, чёрный кадр (нет Screen Recording), непарсящийся вердикт, недоступный vision-бэкенд → **блокирующий результат/ошибка**, не тихий проход.
- `pass` — **нормализованное поле супервайзера**, не берётся из ответа модели (§6).
- **Проваленный required-пункт чеклиста блокирует безусловно** (§6), независимо от severity и наличия issue. Порог severity применяется к open-critique/необязательным находкам.
- Vision-интерфейс `analyze_image(png, prompt) -> VisionVerdict` **не привязан** к текстовому провайдеру агента (§3); реальный бэкенд подключается явно, text-only → отдельная задача (Task 7).
- Терминальная среда фиксируется (§4): минимум — логировать, цель — активно выставлять bounds/профиль.
- Всё на ветке `worktree-clave-dev-headless`; в main не сливаем. Зрение включается флагом `--vision`; без него — поведение Фазы 1 с явной записью в лог, что зрение выключено (§7).
- Python 3.9: `from __future__ import annotations` во всех модулях; namedtuple для структур; ленивый импорт GUI-зависимостей (pyobjc и т.п.), как pyte в Фазе 1.

---

### Task 1: Схема и нормализация вердикта (`visual_verdict.py`)

**Files:**
- Create: `tools/clave-dev/clave_dev/visual_verdict.py`
- Create: `tools/clave-dev/tests/test_visual_verdict.py`

**Interfaces:**
- Produces: `Issue`, `ChecklistItem`, `VisionVerdict` (namedtuple); `SEVERITIES`; `parse_verdict(data: dict, raw: str="") -> VisionVerdict`; `extract_verdict_json(text: str) -> dict`; `verdict_passes(v: VisionVerdict, blocking=("high","medium")) -> bool`; `class VerdictParseError(ValueError)`.

- [ ] **Step 1: Написать модуль и падающие тесты**

`tools/clave-dev/clave_dev/visual_verdict.py`:
```python
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
```

`tools/clave-dev/tests/test_visual_verdict.py`:
```python
import unittest

from clave_dev.visual_verdict import (
    VerdictParseError,
    extract_verdict_json,
    parse_verdict,
    verdict_passes,
)


class ParseTest(unittest.TestCase):
    def test_parse_defaults_are_fail_safe(self):
        v = parse_verdict({"issues": [{"description": "x", "severity": "weird"}],
                           "checklist_results": [{"check": "c"}]})
        self.assertEqual(v.issues[0].severity, "high")   # неизвестный severity → high
        self.assertTrue(v.checklist[0].required)          # required по умолчанию True
        self.assertFalse(v.checklist[0].passed)           # passed по умолчанию False

    def test_extract_json_from_wrapped_text(self):
        text = 'вот вердикт:\n```json\n{"open_critique": "ок"}\n```\nконец'
        self.assertEqual(extract_verdict_json(text)["open_critique"], "ок")

    def test_extract_json_raises_on_garbage(self):
        with self.assertRaises(VerdictParseError):
            extract_verdict_json("никакого json тут нет")


class PassTest(unittest.TestCase):
    def test_all_good_passes(self):
        v = parse_verdict({"checklist_results": [{"check": "c", "required": True, "passed": True}]})
        self.assertTrue(verdict_passes(v))

    def test_required_checklist_failure_blocks_even_with_low_or_no_issue(self):
        v = parse_verdict({"checklist_results": [{"check": "c", "required": True, "passed": False}],
                           "issues": [{"description": "мелочь", "severity": "low"}]})
        self.assertFalse(verdict_passes(v))   # required-провал блокирует вопреки low

    def test_optional_high_issue_blocks(self):
        v = parse_verdict({"checklist_results": [{"check": "c", "required": False, "passed": True}],
                           "issues": [{"description": "big", "severity": "high", "source": "open"}]})
        self.assertFalse(verdict_passes(v))

    def test_optional_low_issue_passes(self):
        v = parse_verdict({"issues": [{"description": "tiny", "severity": "low", "source": "open"}]})
        self.assertTrue(verdict_passes(v))
```

- [ ] **Step 2: Прогнать тесты**

Run: `cd tools/clave-dev && python3 -m unittest tests.test_visual_verdict -v`
Expected: 7 тестов PASS.

- [ ] **Step 3: Commit**

```bash
git add tools/clave-dev/clave_dev/visual_verdict.py tools/clave-dev/tests/test_visual_verdict.py
git commit -m "clave-dev: vision verdict schema and supervisor-normalized pass"
```

---

### Task 2: Интерфейс vision-провайдера + фейк (`vision.py`)

**Files:**
- Create: `tools/clave-dev/clave_dev/vision.py`
- Create: `tools/clave-dev/tests/test_vision.py`

**Interfaces:**
- Consumes: `VisionVerdict`, `parse_verdict` (Task 1).
- Produces: `class VisionProvider(ABC)` c `available()`/`analyze_image(png_path, prompt)`; `class VisionUnavailableError(RuntimeError)`; `class FakeVisionProvider(VisionProvider)` (канонный вердикт для тестов); `DEFAULT_VISION_PROMPT`.

- [ ] **Step 1: Написать модуль и падающие тесты**

`tools/clave-dev/clave_dev/vision.py`:
```python
"""Интерфейс зрения — не привязан к текстовому провайдеру агента (спека §3)."""
from __future__ import annotations

from abc import ABC, abstractmethod
from pathlib import Path

from .visual_verdict import VisionVerdict, parse_verdict

DEFAULT_VISION_PROMPT = (
    "Ты ревьюишь скриншот TUI-приложения в терминале. Верни СТРОГО JSON с полями "
    "issues[], checklist_results[], open_critique. Прогони required-чеклист: "
    "текст не касается правой границы; нет обрезанных глифов; рамки замкнуты; "
    "нет наложения текста. Затем открытая критика: что ещё выглядит не так."
)


class VisionUnavailableError(RuntimeError):
    pass


class VisionProvider(ABC):
    @abstractmethod
    def available(self) -> bool:
        ...

    @abstractmethod
    def analyze_image(self, png_path: Path, prompt: str = DEFAULT_VISION_PROMPT) -> VisionVerdict:
        ...


class FakeVisionProvider(VisionProvider):
    """Возвращает заранее заданный вердикт — для юнит-тестов петли без реального бэкенда."""

    def __init__(self, verdict_dict: dict, available: bool = True):
        self._verdict = verdict_dict
        self._available = available

    def available(self) -> bool:
        return self._available

    def analyze_image(self, png_path: Path, prompt: str = DEFAULT_VISION_PROMPT) -> VisionVerdict:
        if not self._available:
            raise VisionUnavailableError("fake: недоступен")
        return parse_verdict(self._verdict, raw="<fake>")
```

`tools/clave-dev/tests/test_vision.py`:
```python
import unittest
from pathlib import Path

from clave_dev.vision import FakeVisionProvider, VisionUnavailableError
from clave_dev.visual_verdict import verdict_passes


class VisionInterfaceTest(unittest.TestCase):
    def test_fake_returns_parsed_verdict(self):
        fake = FakeVisionProvider({"checklist_results": [{"check": "c", "required": True, "passed": True}]})
        self.assertTrue(fake.available())
        v = fake.analyze_image(Path("/nope.png"))
        self.assertTrue(verdict_passes(v))

    def test_unavailable_raises(self):
        fake = FakeVisionProvider({}, available=False)
        self.assertFalse(fake.available())
        with self.assertRaises(VisionUnavailableError):
            fake.analyze_image(Path("/nope.png"))
```

- [ ] **Step 2: Прогнать тесты**

Run: `cd tools/clave-dev && python3 -m unittest tests.test_vision -v`
Expected: 2 теста PASS.

- [ ] **Step 3: Commit**

```bash
git add tools/clave-dev/clave_dev/vision.py tools/clave-dev/tests/test_vision.py
git commit -m "clave-dev: vision provider interface and fake backend"
```

---

### Task 3: Терминальный профиль (`terminal_profile.py`)

**Files:**
- Create: `tools/clave-dev/clave_dev/terminal_profile.py`
- Create: `tools/clave-dev/tests/test_terminal_profile.py`

**Interfaces:**
- Produces: `TerminalProfile` (namedtuple `app cols rows font font_size theme opacity locale bounds`); `default_profile() -> TerminalProfile`; `describe(p) -> dict` (для лога/отчёта); `apply_bounds_applescript(p) -> str` (AppleScript-команда установки bounds окна).

- [ ] **Step 1: Написать модуль и падающие тесты**

`tools/clave-dev/clave_dev/terminal_profile.py`:
```python
"""Фиксированная терминальная среда, чтобы vision не ловил шум (спека §4)."""
from __future__ import annotations

from collections import namedtuple

TerminalProfile = namedtuple(
    "TerminalProfile", "app cols rows font font_size theme opacity locale bounds"
)


def default_profile() -> TerminalProfile:
    return TerminalProfile(
        app="Terminal",
        cols=100,
        rows=30,
        font="SF Mono",
        font_size=13,
        theme="clave-dev",
        opacity=1.0,
        locale="ru_RU.UTF-8",
        bounds=(100, 100, 900, 640),  # x, y, w, h
    )


def describe(p: TerminalProfile) -> dict:
    """Плоский dict для лога/отчёта — атрибуция любого визуального вывода к среде."""
    return dict(p._asdict())


def apply_bounds_applescript(p: TerminalProfile) -> str:
    """AppleScript: выставить bounds фронтового окна Terminal (цель §4 — детерминизм среды)."""
    x, y, w, h = p.bounds
    return (
        'tell application "Terminal" to set bounds of front window '
        f"to {{{x}, {y}, {x + w}, {y + h}}}"
    )
```

`tools/clave-dev/tests/test_terminal_profile.py`:
```python
import unittest

from clave_dev.terminal_profile import apply_bounds_applescript, default_profile, describe


class TerminalProfileTest(unittest.TestCase):
    def test_describe_is_flat_dict_with_all_fields(self):
        d = describe(default_profile())
        for key in ("app", "cols", "rows", "font", "font_size", "theme", "opacity", "locale", "bounds"):
            self.assertIn(key, d)

    def test_apply_bounds_applescript_uses_x2_y2(self):
        p = default_profile()._replace(bounds=(10, 20, 800, 600))
        script = apply_bounds_applescript(p)
        self.assertIn("Terminal", script)
        self.assertIn("{10, 20, 810, 620}", script)  # x2=x+w, y2=y+h
```

- [ ] **Step 2: Прогнать тесты**

Run: `cd tools/clave-dev && python3 -m unittest tests.test_terminal_profile -v`
Expected: 2 теста PASS.

- [ ] **Step 3: Commit**

```bash
git add tools/clave-dev/clave_dev/terminal_profile.py tools/clave-dev/tests/test_terminal_profile.py
git commit -m "clave-dev: fixed terminal profile (log + apply bounds)"
```

---

### Task 4: Разрешение окна CGWindowID (`window_resolve.py`)

**Files:**
- Create: `tools/clave-dev/clave_dev/window_resolve.py`
- Create: `tools/clave-dev/tests/test_window_resolve.py`

**Interfaces:**
- Produces: `class WindowNotFoundError(RuntimeError)`; `resolve_cgwindow_id(window_infos: list, owner: str, title: str) -> int` (чистая функция над списком window-info dict); `list_windows() -> list` (тонкая обёртка Quartz, ленивый импорт).

- [ ] **Step 1: Написать модуль и падающие тесты**

`tools/clave-dev/clave_dev/window_resolve.py`:
```python
"""Слой 'Terminal window → CGWindowID' (спека §5). AppleScript id ≠ CGWindowID."""
from __future__ import annotations


class WindowNotFoundError(RuntimeError):
    pass


def resolve_cgwindow_id(window_infos: list, owner: str, title: str) -> int:
    """Чистая логика: из списка window-info (ключи Quartz kCGWindow*) выбрать окно
    владельца `owner` с титулом, содержащим `title`. 0 или >1 совпадений → ошибка
    с перечислением кандидатов (не угадываем)."""
    matches = [
        w for w in window_infos
        if w.get("kCGWindowOwnerName") == owner and title in (w.get("kCGWindowName") or "")
    ]
    if len(matches) == 1:
        return int(matches[0]["kCGWindowNumber"])
    candidates = [(w.get("kCGWindowOwnerName"), w.get("kCGWindowName")) for w in window_infos]
    raise WindowNotFoundError(
        f"ожидалось ровно одно окно {owner!r} с титулом ~{title!r}, найдено {len(matches)}; "
        f"кандидаты: {candidates}"
    )


def list_windows() -> list:
    """Тонкая обёртка над Quartz (ленивый импорт: чистая логика тестируется без pyobjc)."""
    import Quartz  # pyobjc-framework-Quartz

    infos = Quartz.CGWindowListCopyWindowInfo(
        Quartz.kCGWindowListOptionOnScreenOnly, Quartz.kCGNullWindowID
    )
    return list(infos or [])
```

`tools/clave-dev/tests/test_window_resolve.py`:
```python
import unittest

from clave_dev.window_resolve import WindowNotFoundError, resolve_cgwindow_id


def _w(owner, name, num):
    return {"kCGWindowOwnerName": owner, "kCGWindowName": name, "kCGWindowNumber": num}


class WindowResolveTest(unittest.TestCase):
    def test_single_match_returns_cgwindow_number(self):
        infos = [_w("Finder", "x", 1), _w("Terminal", "clave-dev abc123", 42)]
        self.assertEqual(resolve_cgwindow_id(infos, "Terminal", "clave-dev abc123"), 42)

    def test_zero_matches_raises(self):
        with self.assertRaises(WindowNotFoundError):
            resolve_cgwindow_id([_w("Finder", "x", 1)], "Terminal", "clave-dev abc")

    def test_multiple_matches_raises(self):
        infos = [_w("Terminal", "clave-dev z", 7), _w("Terminal", "clave-dev z", 8)]
        with self.assertRaises(WindowNotFoundError):
            resolve_cgwindow_id(infos, "Terminal", "clave-dev z")
```

- [ ] **Step 2: Прогнать тесты**

Run: `cd tools/clave-dev && python3 -m unittest tests.test_window_resolve -v`
Expected: 3 теста PASS.

- [ ] **Step 3: Commit**

```bash
git add tools/clave-dev/clave_dev/window_resolve.py tools/clave-dev/tests/test_window_resolve.py
git commit -m "clave-dev: Terminal window -> CGWindowID resolution with diagnostics"
```

---

### Task 5: Драйвер терминала и захват кадра (`terminal_driver.py`, `capture.py`)

**Files:**
- Create: `tools/clave-dev/clave_dev/terminal_driver.py`
- Create: `tools/clave-dev/clave_dev/capture.py`
- Create: `tools/clave-dev/tests/test_capture.py`

**Interfaces:**
- Produces (driver): `launch_applescript(binary, title, cwd) -> str`; `keystroke_applescript(keys) -> str`.
- Produces (capture): `screencapture_cmd(cgwindow_id, out_path) -> list`; `class ScreenPermissionError(RuntimeError)`; `is_blank_frame(pixels: bytes, threshold: float=0.02) -> bool` (эвристика пустого/чёрного кадра — нет Screen Recording).

- [ ] **Step 1: Написать модули и падающие тесты**

`tools/clave-dev/clave_dev/terminal_driver.py`:
```python
"""Запуск clave в Terminal.app и отправка клавиш через AppleScript (спека §2, §5)."""
from __future__ import annotations

from pathlib import Path


def launch_applescript(binary: Path, title: str, cwd: Path) -> str:
    """Открыть новое окно Terminal, задать уникальный титул (для разрешения окна) и
    запустить clave в нужном каталоге."""
    cmd = f"cd {cwd}; clear; printf '\\\\033]0;{title}\\\\007'; {binary}"
    return (
        'tell application "Terminal"\n'
        "  activate\n"
        f'  do script "{cmd}"\n'
        f'  set custom title of front window to "{title}"\n'
        "end tell"
    )


def keystroke_applescript(keys: str) -> str:
    """Отправить строку клавиш активному окну через System Events (нужен Accessibility)."""
    escaped = keys.replace("\\", "\\\\").replace('"', '\\"')
    return f'tell application "System Events" to keystroke "{escaped}"'
```

`tools/clave-dev/clave_dev/capture.py`:
```python
"""Снимок конкретного окна и детект пустого кадра (нет Screen Recording) — спека §5, §8."""
from __future__ import annotations

from pathlib import Path


class ScreenPermissionError(RuntimeError):
    pass


def screencapture_cmd(cgwindow_id: int, out_path: Path) -> list:
    """-x без звука, -o без тени окна, -l<id> конкретное окно по CGWindowID."""
    return ["screencapture", "-x", "-o", f"-l{cgwindow_id}", str(out_path)]


def is_blank_frame(pixels: bytes, threshold: float = 0.02) -> bool:
    """Доля не-нулевых байтов ниже threshold → кадр практически пустой/чёрный
    (типичный признак отсутствия разрешения на запись экрана). Пустой ввод → пустой кадр."""
    if not pixels:
        return True
    nonzero = sum(1 for b in pixels if b != 0)
    return (nonzero / len(pixels)) < threshold
```

`tools/clave-dev/tests/test_capture.py`:
```python
import unittest
from pathlib import Path

from clave_dev.capture import is_blank_frame, screencapture_cmd
from clave_dev.terminal_driver import keystroke_applescript, launch_applescript


class CaptureTest(unittest.TestCase):
    def test_screencapture_cmd_targets_window(self):
        cmd = screencapture_cmd(42, Path("/tmp/o.png"))
        self.assertEqual(cmd[:1], ["screencapture"])
        self.assertIn("-l42", cmd)
        self.assertIn("/tmp/o.png", cmd)

    def test_is_blank_frame_detects_black_and_content(self):
        self.assertTrue(is_blank_frame(bytes(1000)))              # всё нули → пусто
        self.assertTrue(is_blank_frame(b""))                       # пусто → пусто
        self.assertFalse(is_blank_frame(bytes([200]) * 1000))      # насыщено → не пусто

    def test_launch_applescript_sets_unique_title(self):
        script = launch_applescript(Path("/wt/target/debug/clave"), "clave-dev xyz", Path("/wt"))
        self.assertIn("clave-dev xyz", script)
        self.assertIn("do script", script)

    def test_keystroke_applescript_escapes_quotes(self):
        self.assertIn('\\"', keystroke_applescript('say "hi"'))
```

- [ ] **Step 2: Прогнать тесты**

Run: `cd tools/clave-dev && python3 -m unittest tests.test_capture -v`
Expected: 4 теста PASS.

- [ ] **Step 3: Commit**

```bash
git add tools/clave-dev/clave_dev/terminal_driver.py tools/clave-dev/clave_dev/capture.py tools/clave-dev/tests/test_capture.py
git commit -m "clave-dev: Terminal driver and window capture with blank-frame detection"
```

---

### Task 6: Интеграция зрения в петлю (`observer.py`, `loop.py`, `context.py`)

**Files:**
- Modify: `tools/clave-dev/clave_dev/loop.py` (расширить `converged`, `RunConfig`, `run_loop`)
- Modify: `tools/clave-dev/clave_dev/context.py` (блок «Визуальные дефекты»)
- Create: `tools/clave-dev/clave_dev/visual_observer.py` (визуальный проход поверх сценария)
- Create: `tools/clave-dev/tests/test_visual_loop.py`

**Interfaces:**
- Consumes: `VisionProvider`/`FakeVisionProvider` (Task 2), `verdict_passes`/`VisionVerdict` (Task 1).
- Produces: `converged(checks, assertion_results, vision_verdicts=()) -> bool` (расширение); `build_visual_context(verdicts) -> str`; `RunConfig` получает поля `vision`, `blocking_severities`.

- [ ] **Step 1: Расширить `converged` (тест сперва)**

Добавить в `tools/clave-dev/tests/test_visual_loop.py`:
```python
import unittest

from clave_dev.assertions import AssertionResult
from clave_dev.checks import ChecksResult
from clave_dev.loop import converged
from clave_dev.vision import FakeVisionProvider
from clave_dev.context import build_visual_context


def _green_checks():
    return ChecksResult(build_ok=True, test_failures=0, clippy_ok=True, fmt_ok=True, raw={})


class VisualConvergeTest(unittest.TestCase):
    def test_vision_fail_blocks_even_if_text_green(self):
        good_text = [AssertionResult("a", True, "")]
        bad_vision = FakeVisionProvider(
            {"checklist_results": [{"check": "правая граница", "required": True, "passed": False}]}
        ).analyze_image.__self__.analyze_image  # см. ниже — используем сам вердикт
        verdict = FakeVisionProvider(
            {"checklist_results": [{"check": "правая граница", "required": True, "passed": False}]}
        ).analyze_image(None)
        self.assertFalse(converged(_green_checks(), good_text, [verdict]))

    def test_all_green_with_passing_vision_converges(self):
        verdict = FakeVisionProvider(
            {"checklist_results": [{"check": "ok", "required": True, "passed": True}]}
        ).analyze_image(None)
        self.assertTrue(converged(_green_checks(), [AssertionResult("a", True, "")], [verdict]))

    def test_no_vision_verdicts_keeps_phase1_behavior(self):
        self.assertTrue(converged(_green_checks(), [AssertionResult("a", True, "")]))

    def test_build_visual_context_lists_failed_checks(self):
        verdict = FakeVisionProvider(
            {"checklist_results": [{"check": "правая граница", "required": True, "passed": False, "note": "срез"}],
             "open_critique": "иначе ок"}
        ).analyze_image(None)
        text = build_visual_context([verdict])
        self.assertIn("правая граница", text)
        self.assertIn("иначе ок", text)
```

- [ ] **Step 2: Реализовать расширения**

В `tools/clave-dev/clave_dev/loop.py` заменить `converged` на версию с визуальными вердиктами и добавить поля в `RunConfig`:
```python
from .visual_verdict import verdict_passes

def converged(checks, assertion_results, vision_verdicts=()) -> bool:
    """Спека §5+§6: проверки зелёные И текстовые assertions pass И все visual-вердикты pass."""
    if checks is None or not checks.build_ok:
        return False
    checks_ok = checks.test_failures == 0 and checks.clippy_ok and checks.fmt_ok
    asserts_ok = all(r.passed for r in assertion_results)
    vision_ok = all(verdict_passes(v) for v in vision_verdicts)
    return checks_ok and asserts_ok and vision_ok
```
Обновить `RunConfig` namedtuple, добавив в конец поля `vision blocking_severities` (со значениями по умолчанию через явную передачу из cli). В `run_loop`, если `cfg.vision` задан и `available()`, после текстового наблюдения снять кадр и получить вердикт (см. `visual_observer.run_visual` ниже); иначе `log`-заметка «зрение выключено» и пустой список вердиктов.

`tools/clave-dev/clave_dev/visual_observer.py`:
```python
"""Визуальный проход: снять окно Terminal и получить вердикт зрения (спека §7).
Любая ошибка захвата/парсинга → блокирующий вердикт (никогда тихий pass, §8)."""
from __future__ import annotations

import tempfile
from pathlib import Path

from .capture import ScreenPermissionError, is_blank_frame, screencapture_cmd
from .visual_verdict import VisionVerdict, ChecklistItem


def _blocking(reason: str) -> VisionVerdict:
    return VisionVerdict(
        issues=[], open_critique=reason, raw=reason,
        checklist=[ChecklistItem(check=reason, required=True, passed=False, note=reason)],
    )


def run_visual(cgwindow_id, vision, run_cmd, read_pixels, prompt=None):
    """cgwindow_id: id окна; vision: VisionProvider; run_cmd(list)->int код;
    read_pixels(path)->bytes. Возвращает VisionVerdict (блокирующий при любой беде)."""
    import clave_dev.vision as _v

    out = Path(tempfile.mkdtemp(prefix="clave-dev-shot-")) / "frame.png"
    code = run_cmd(screencapture_cmd(cgwindow_id, out))
    if code != 0:
        return _blocking(f"screencapture код {code} (нет Screen Recording?)")
    if is_blank_frame(read_pixels(out)):
        return _blocking("кадр пустой/чёрный — вероятно нет разрешения на запись экрана")
    try:
        return vision.analyze_image(out, prompt or _v.DEFAULT_VISION_PROMPT)
    except Exception as e:  # непарсящийся вердикт/недоступность → блок
        return _blocking(f"vision-бэкенд: {e}")
```

В `tools/clave-dev/clave_dev/context.py` добавить:
```python
def build_visual_context(verdicts) -> str:
    lines = ["## Визуальные дефекты"]
    if not verdicts:
        lines.append("- (зрение выключено или нет вердиктов)")
    for i, v in enumerate(verdicts):
        for c in v.checklist:
            if not c.passed:
                lines.append(f"- сценарий {i}: FAIL чеклист '{c.check}' {('(required)' if c.required else '')} {c.note}")
        for iss in v.issues:
            lines.append(f"- сценарий {i}: [{iss.severity}] {iss.description} region={iss.region_hint}")
        if v.open_critique:
            lines.append(f"- сценарий {i}: критика: {v.open_critique}")
    return "\n".join(lines)
```

- [ ] **Step 3: Прогнать тесты (упростить тестовый вердикт)**

> Примечание исполнителю: в тесте выше `.analyze_image(None)` у `FakeVisionProvider` игнорирует путь и возвращает вердикт из dict — это законно (фейк не читает файл). Убрать артефактную строку `bad_vision = ...__self__...` (осталась от черновика) и пользоваться напрямую `verdict = FakeVisionProvider({...}).analyze_image(None)`.

Run: `cd tools/clave-dev && python3 -m unittest tests.test_visual_loop tests.test_loop -v`
Expected: новые визуальные тесты + прежние тесты петли PASS.

- [ ] **Step 4: Commit**

```bash
git add tools/clave-dev/clave_dev/loop.py tools/clave-dev/clave_dev/context.py tools/clave-dev/clave_dev/visual_observer.py tools/clave-dev/tests/test_visual_loop.py
git commit -m "clave-dev: integrate vision verdict into convergence and agent context"
```

---

### Task 7: Реальный бэкенд, CLI-флаги, проба способности, e2e-инструкция

**Files:**
- Create: `tools/clave-dev/clave_dev/vision_claude.py` (реальный image-бэкенд + `available()`-проба)
- Modify: `tools/clave-dev/clave_dev/cli.py` (`--vision`, `--terminal-profile`, `--severity-threshold`)
- Create: `tools/clave-dev/scripts/e2e_vision.md` (процедура прогона у пользователя)
- Create: `tools/clave-dev/tests/test_vision_claude.py`

**Interfaces:**
- Consumes: `VisionProvider` (Task 2), `extract_verdict_json`/`parse_verdict` (Task 1).
- Produces: `class ClaudeVisionProvider(VisionProvider)`; `select_vision(name, env) -> VisionProvider|None`.

- [ ] **Step 1: Реальный бэкенд с пробой способности (тест на парсинг, без сети)**

`tools/clave-dev/clave_dev/vision_claude.py`:
```python
"""Реальный image-бэкенд зрения. Способность приёма PNG НЕ предполагается у текстового
CLI агента (спека §3): available() честно проверяет наличие ключа/канала; если нет —
провайдер недоступен, и подключение реального канала остаётся явной задачей."""
from __future__ import annotations

import os
from pathlib import Path

from .vision import VisionProvider, VisionUnavailableError, DEFAULT_VISION_PROMPT
from .visual_verdict import VisionVerdict, extract_verdict_json, parse_verdict


class ClaudeVisionProvider(VisionProvider):
    def __init__(self, env=None, sender=None):
        # sender(png_path, prompt)->str: инъекция канала к модели (реальный API в проде,
        # фейковый в тестах). По умолчанию требует ANTHROPIC_API_KEY.
        self._env = env if env is not None else os.environ
        self._sender = sender

    def available(self) -> bool:
        return self._sender is not None or bool(self._env.get("ANTHROPIC_API_KEY"))

    def analyze_image(self, png_path: Path, prompt: str = DEFAULT_VISION_PROMPT) -> VisionVerdict:
        if not self.available():
            raise VisionUnavailableError(
                "нет канала к зрячей модели (ANTHROPIC_API_KEY/sender) — подключение image-бэкенда это отдельная задача"
            )
        raw = self._sender(png_path, prompt) if self._sender else _send_via_api(self._env, png_path, prompt)
        return parse_verdict(extract_verdict_json(raw), raw=raw)


def _send_via_api(env, png_path: Path, prompt: str) -> str:
    """Отправка изображения в Claude image-API. Реализация с сетью — прод-путь; в тестах
    подменяется `sender`. Реализовать при подключении реального ключа."""
    raise VisionUnavailableError("прямой image-API ещё не подключён; передай sender или подключи API")


def select_vision(name, env=None):
    """Фабрика по имени бэкенда из --vision (None → зрение выключено)."""
    if not name:
        return None
    if name == "claude":
        return ClaudeVisionProvider(env=env)
    raise ValueError(f"неизвестный vision-бэкенд: {name}")
```

`tools/clave-dev/tests/test_vision_claude.py`:
```python
import unittest
from pathlib import Path

from clave_dev.vision import VisionUnavailableError
from clave_dev.vision_claude import ClaudeVisionProvider, select_vision
from clave_dev.visual_verdict import verdict_passes


class ClaudeVisionTest(unittest.TestCase):
    def test_unavailable_without_key_or_sender(self):
        p = ClaudeVisionProvider(env={})
        self.assertFalse(p.available())
        with self.assertRaises(VisionUnavailableError):
            p.analyze_image(Path("/x.png"))

    def test_sender_channel_parses_verdict(self):
        raw = '```json\n{"checklist_results":[{"check":"c","required":true,"passed":true}]}\n```'
        p = ClaudeVisionProvider(env={}, sender=lambda png, prompt: raw)
        self.assertTrue(p.available())
        self.assertTrue(verdict_passes(p.analyze_image(Path("/x.png"))))

    def test_select_vision_none_disables(self):
        self.assertIsNone(select_vision(None))
        self.assertIsInstance(select_vision("claude", env={"ANTHROPIC_API_KEY": "k"}), ClaudeVisionProvider)
```

- [ ] **Step 2: CLI-флаги**

В `tools/clave-dev/clave_dev/cli.py` добавить аргументы и проброс в `RunConfig`:
```python
p.add_argument("--vision", default=None, help="бэкенд зрения (напр. claude); без него — текст-онли Фаза 1")
p.add_argument("--terminal-profile", default=None, help="имя профиля Terminal.app для среды прогона")
p.add_argument("--severity-threshold", default="medium", choices=["low", "medium", "high"],
               help="минимальный severity необязательной находки, который блокирует")
```
Собрать `vision = select_vision(args.vision, env)`; `blocking = severities_at_or_above(args.severity_threshold)`; передать в `RunConfig`. Если `args.vision` задан, но `vision`/`vision.available()` ложны — напечатать явное предупреждение (зрение выключено, это отдельная задача) и продолжить текст-онли (не тихо).

- [ ] **Step 3: E2e-инструкция для пользователя**

`tools/clave-dev/scripts/e2e_vision.md` — Markdown с шагами: (1) выдать Accessibility + Screen Recording для терминала/раннера в System Settings → Privacy; (2) выбрать/создать профиль Terminal.app `clave-dev`; (3) собрать fresh clave ветки; (4) прогон `python3 -m clave_dev "..." --vision claude --terminal-profile clave-dev` на заведомо-битом футере → ожидать `pass=false` с issue про правую границу; (5) на чистом UI → `pass=true`. Явно: этот шаг выполняет пользователь (GUI + разрешения), см. §1 спеки.

- [ ] **Step 4: Прогнать всё**

Run: `cd tools/clave-dev && python3 -m unittest discover -s tests -v`
Expected: все юнит-тесты (Фаза 1 + Фаза 2) PASS.

- [ ] **Step 5: Commit**

```bash
git add tools/clave-dev/clave_dev/vision_claude.py tools/clave-dev/clave_dev/cli.py tools/clave-dev/scripts/e2e_vision.md tools/clave-dev/tests/test_vision_claude.py
git commit -m "clave-dev: real vision backend (capability-gated), CLI flags, e2e procedure"
```

---

## Self-Review

- **Покрытие спеки:** §3 контракт — Task 2 (интерфейс+фейк) + Task 7 (реальный бэкенд, `available()`-проба, text-only=отдельная задача); §4 среда — Task 3 (лог+bounds); §5 CGWindowID — Task 4 (+фолбэк отмечен) и уникальный титул — Task 5; §6 вердикт+нормализация+required-блок — Task 1; §7 интеграция/деградация — Task 6 (+флаг в Task 7); §8 «не ложный pass» — fail-safe дефолты (Task 1), блокирующий вердикт при беде (Task 6 `visual_observer`), детект чёрного кадра (Task 5); §9 тесты — юниты в каждом таске + e2e-инструкция (Task 7).
- **Плейсхолдеры:** нет; исключение — `_send_via_api` намеренно бросает (реальная сеть = отдельный прод-шаг), но интерфейс и тестовый `sender`-канал полны и покрыты (Task 7).
- **Согласованность типов:** `VisionVerdict`/`Issue`/`ChecklistItem` (Task 1) → потребляются `vision`/`visual_observer`/`context`/`loop` едино; `verdict_passes` — единственная точка pass; `converged` расширен обратносовместимо (`vision_verdicts=()` по умолчанию → тесты Фазы 1 не ломаются); `VisionProvider.analyze_image` сигнатура одна и та же у фейка и реального.
- **Правка для исполнителя:** в Task 6 Step 1 убрать артефактную строку `bad_vision = ...__self__...` (см. примечание Step 3) — пользоваться `FakeVisionProvider({...}).analyze_image(None)` напрямую.

## Границы

Даёт локальное зрение поверх Фазы 1, юнит-проверенное здесь; реальный proof — e2e у пользователя (Task 7). Дальше — **Фаза 3** (интеграция самопиления в TUI, сразу следом по директиве), переиспользует этот vision-слой.
