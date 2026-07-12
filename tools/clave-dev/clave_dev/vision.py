"""Интерфейс зрения — не привязан к текстовому провайдеру агента (спека §3)."""
from __future__ import annotations

from abc import ABC, abstractmethod
from pathlib import Path

from .visual_verdict import VisionVerdict, parse_verdict

DEFAULT_VISION_PROMPT = (
    "Ты ревьюишь скриншот TUI-приложения в терминале на визуальные дефекты. "
    "Верни СТРОГО один JSON-объект без прозы вокруг, с полями:\n"
    '- "checklist_results": массив {"check": str, "required": bool, "passed": bool, "note": str} — '
    "прогони required-чеклист: текст не касается правой границы; нет обрезанных глифов; "
    "рамки/бордеры замкнуты; нет наложения текста.\n"
    '- "issues": массив {"description": str, "severity": "low|medium|high", "source": "checklist|open"}.\n'
    '- "open_critique": str — что ещё выглядит не так.\n'
    "Если дефектов нет — issues пустой и все passed=true."
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


def vision_preflight(vision, capture=None, quartz=None, settings=None, profile_name="clave-dev"):
    """Причина, по которой зрение работать НЕ сможет (None — всё в порядке).

    Проверяем ДО старта прогона: узнать о невозможности на третьем раунде дорого и обидно,
    а fail-safe вердикт превратил бы каждый раунд в гарантированную не-сходимость.
    `capture`/`quartz`/`settings` инъектируются в тестах; в проде — реальные проверки."""
    if vision is None or not vision.available():
        return "нет доступного vision-бэкенда (нужен claude CLI или ANTHROPIC_API_KEY)"
    if not (quartz or _quartz_ok)():
        return "нет pyobjc (Quartz): не разрешить окно и не декодировать кадр"
    reason = (capture or _screen_probe)()
    if reason:
        return reason
    if not (settings or _settings_ok)(profile_name):
        return (
            f"нет профиля Terminal «{profile_name}» с «при выходе из shell — закрыть окно». "
            "Без него окно визуального прохода некому убрать: снаружи Terminal-окно не "
            "закрывается (проверено), и за каждый раунд копилось бы окно-мусор. "
            "Создай профиль и перезапусти Terminal"
        )
    return None


def _quartz_ok() -> bool:
    try:
        import Quartz  # noqa: F401

        return True
    except Exception:
        return False


def _settings_ok(name: str) -> bool:
    from .terminal_profile import settings_set_exists

    return settings_set_exists(name)


def _screen_probe():
    """Пробный снимок. Без доступа к оконному серверу screencapture молча отвечает
    «could not create image from display» ДАЖЕ при выданных правах (напр. из песочницы)."""
    import subprocess
    import tempfile

    with tempfile.TemporaryDirectory() as d:
        shot = Path(d) / "probe.png"
        res = subprocess.run(
            ["screencapture", "-x", str(shot)], capture_output=True, text=True
        )
        if res.returncode != 0 or not shot.is_file() or shot.stat().st_size < 1024:
            return (
                "screencapture не смог снять экран — нет доступа к оконному серверу "
                "или не выдано Screen Recording"
            )
    return None
