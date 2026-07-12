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
