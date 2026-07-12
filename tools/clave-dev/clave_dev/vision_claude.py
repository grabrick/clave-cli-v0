"""Реальный image-бэкенд зрения. Способность приёма PNG НЕ предполагается у текстового
CLI агента (спека §3): available() честно проверяет наличие канала; если нет — провайдер
недоступен, и подключение реального канала остаётся явной задачей."""
from __future__ import annotations

import base64
import json
import os
import urllib.error
import urllib.request
from pathlib import Path

from .vision import DEFAULT_VISION_PROMPT, VisionProvider, VisionUnavailableError
from .visual_verdict import VisionVerdict, extract_verdict_json, parse_verdict

ANTHROPIC_URL = "https://api.anthropic.com/v1/messages"
DEFAULT_VISION_MODEL = "claude-sonnet-5"


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


def build_vision_request(png_b64: str, prompt: str, model: str, api_key: str):
    """Чистая сборка запроса к Anthropic Messages API (image+text) → (url, headers, body_bytes)."""
    body = {
        "model": model,
        "max_tokens": 1024,
        "messages": [
            {
                "role": "user",
                "content": [
                    {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": png_b64}},
                    {"type": "text", "text": prompt},
                ],
            }
        ],
    }
    headers = {
        "x-api-key": api_key,
        "anthropic-version": "2023-06-01",
        "content-type": "application/json",
    }
    return ANTHROPIC_URL, headers, json.dumps(body).encode("utf-8")


def extract_vision_text(response: dict) -> str:
    """Текст из ответа Anthropic: склеиваем text-блоки content."""
    parts = [b.get("text", "") for b in response.get("content", []) if b.get("type") == "text"]
    return "".join(parts).strip()


def _send_via_api(env, png_path: Path, prompt: str) -> str:
    """Реальный вызов Anthropic image-API (stdlib urllib). Модель — CLAVE_VISION_MODEL
    (по умолчанию claude-sonnet-5). Сборка/разбор — в чистых build_vision_request/extract_vision_text."""
    api_key = env.get("ANTHROPIC_API_KEY")
    if not api_key:
        raise VisionUnavailableError("нет ANTHROPIC_API_KEY для image-API")
    model = env.get("CLAVE_VISION_MODEL", DEFAULT_VISION_MODEL)
    png_b64 = base64.b64encode(Path(png_path).read_bytes()).decode("ascii")
    url, headers, body = build_vision_request(png_b64, prompt, model, api_key)
    req = urllib.request.Request(url, data=body, headers=headers, method="POST")
    try:
        with urllib.request.urlopen(req, timeout=60) as resp:
            data = json.loads(resp.read().decode("utf-8"))
    except urllib.error.HTTPError as e:
        detail = e.read().decode("utf-8", "ignore")[:200]
        raise VisionUnavailableError(f"image-API HTTP {e.code}: {detail}")
    except Exception as e:
        raise VisionUnavailableError(f"image-API ошибка: {e}")
    return extract_vision_text(data)


def select_vision(name, env=None):
    """Фабрика по имени бэкенда из --vision (None → зрение выключено)."""
    if not name:
        return None
    if name == "claude":
        return ClaudeVisionProvider(env=env)
    raise ValueError(f"неизвестный vision-бэкенд: {name}")
