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
