"""Точечный vision-probe для e2e Фазы 2: снять ОДИН скриншот clave в Terminal.app и
получить вердикт, без полной петли (агент/cargo/worktree). Для быстрой пары
«узкое окно → pass=false / нормальное → pass=true» (спека §1, e2e).

GUI-запуск (`run_probe`) — только на машине с Terminal.app + разрешениями; чистая
`probe_summary` тестируется здесь."""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from .terminal_profile import default_profile
from .vision_claude import select_vision
from .visual_verdict import verdict_passes


def probe_summary(verdict, blocking=("high", "medium")):
    """Вердикт → (сводка для печати, код выхода). pass → 0, иначе 1."""
    ok = verdict_passes(verdict, blocking)
    summary = {
        "pass": ok,
        "issues": [{"description": i.description, "severity": i.severity} for i in verdict.issues],
        "failed_required": [c.check for c in verdict.checklist if c.required and not c.passed],
        "open_critique": verdict.open_critique,
    }
    return summary, (0 if ok else 1)


def run_probe(binary, profile, vision, prompt=None):
    """E2e-only: поднять clave в Terminal.app с bounds профиля, снять окно, оценить зрением.
    Делит ОДИН безопасный GUI-проход с петлёй (`gui_capture_verdict`): без System Events,
    с изолированным CLAVE_HOME, без кражи фокуса. Любая беда → блокирующий вердикт (§8)."""
    from .visual_observer import blocking_verdict, gui_capture_verdict

    try:
        return gui_capture_verdict(binary, Path.cwd(), profile, vision, prompt=prompt)
    except Exception as e:
        return blocking_verdict(f"probe упал: {e}")


def main(argv=None) -> int:
    p = argparse.ArgumentParser(
        prog="clave_dev.vision_probe",
        description="скриншот clave в Terminal.app + vision-вердикт (Фаза 2 e2e)",
    )
    p.add_argument("binary", help="путь к бинарю clave для показа в окне")
    p.add_argument("--width", type=int, default=None, help="ширина окна (узкая → провокация среза)")
    p.add_argument("--height", type=int, default=None)
    p.add_argument("--vision", default="claude")
    p.add_argument("--terminal-profile", default=None)
    args = p.parse_args(argv)

    profile = default_profile()
    if args.terminal_profile:
        profile = profile._replace(theme=args.terminal_profile)
    if args.width and args.height:
        x, y, _, _ = profile.bounds
        profile = profile._replace(bounds=(x, y, args.width, args.height))

    vision = select_vision(args.vision)
    if vision is None or not vision.available():
        print("vision-бэкенд недоступен (задай ANTHROPIC_API_KEY)", file=sys.stderr)
        return 2

    verdict = run_probe(args.binary, profile, vision)
    summary, code = probe_summary(verdict)
    print(json.dumps(summary, ensure_ascii=False, indent=2))
    return code


if __name__ == "__main__":
    sys.exit(main())
