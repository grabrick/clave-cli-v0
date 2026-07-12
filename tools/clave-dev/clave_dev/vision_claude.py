"""Реальный image-бэкенд зрения. Способность приёма PNG НЕ предполагается у текстового
CLI агента (спека §3): available() честно проверяет наличие канала; если нет — провайдер
недоступен, и подключение реального канала остаётся явной задачей."""
from __future__ import annotations

import os
from pathlib import Path

from .vision import DEFAULT_VISION_PROMPT, VisionProvider, VisionUnavailableError
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
                "нет канала к зрячей модели (ANTHROPIC_API_KEY/sender) — "
                "подключение image-бэкенда это отдельная задача"
            )
        raw = self._sender(png_path, prompt) if self._sender else _send_via_api(self._env, png_path, prompt)
        return parse_verdict(extract_verdict_json(raw), raw=raw)


def _send_via_api(env, png_path: Path, prompt: str) -> str:
    """Отправка изображения в image-API. Прод-путь с сетью; в тестах подменяется `sender`.
    Реализовать при подключении реального ключа/канала."""
    raise VisionUnavailableError("прямой image-API ещё не подключён; передай sender или подключи API")


def select_vision(name, env=None):
    """Фабрика по имени бэкенда из --vision (None → зрение выключено)."""
    if not name:
        return None
    if name == "claude":
        return ClaudeVisionProvider(env=env)
    raise ValueError(f"неизвестный vision-бэкенд: {name}")
