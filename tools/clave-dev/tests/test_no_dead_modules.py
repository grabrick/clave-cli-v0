"""ПРАВИЛО 3: запускай, а не рассуждай. Ни один модуль не освобождён от тестов.

«Это e2e-only, тестом не покрыть» — самая дорогая фраза в проекте. Под ней молча сломался
vision_probe, когда захват стал отдавать список выборок: `probe_summary` полез бы в `.issues` у
списка. Не поймал никто — модуль считался непокрываемым по определению.

Освобождение от проверки всегда обосновано и всегда выходит боком: «тут нечего ломаться», «это
просто обёртка», «я же вижу, что верно». Каждый раз, когда в этой сессии я запускал — я находил
ложь; каждый раз, когда рассуждал — производил новую.

Считаем ВЫПОЛНЕНИЕ, а не импорт: импортированный, но ни разу не вызванный модуль ничем не проверен.

Сам детектор проверен тем же правилом: подложенный мёртвый модуль он ловит (prove_no_dead_modules.py,
код возврата 1).
"""
import subprocess
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PROVE = ROOT / "scripts" / "prove_no_dead_modules.py"


class NoDeadModulesTest(unittest.TestCase):
    def test_the_suite_executes_every_module(self):
        res = subprocess.run(
            [sys.executable, str(PROVE)],
            capture_output=True,
            text=True,
            cwd=str(ROOT),
            check=False,
        )

        self.assertEqual(
            res.returncode,
            0,
            "модуль, который не выполняет ни один тест, ничем не проверен:\n" + res.stderr,
        )

    def test_the_detector_itself_is_still_there(self):
        # Детектор, который удалили, — это правило, которое отменили молча.
        self.assertTrue(PROVE.is_file())


if __name__ == "__main__":
    unittest.main()
