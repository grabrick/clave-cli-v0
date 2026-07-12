"""ПРАВИЛО 1: гейт обязан уметь провалиться.

Проверяется единственным способом — сломать гейт и убедиться, что набор это заметил. Заменяем
каждый гейт на «всегда пропускай»; набор ОБЯЗАН покраснеть. Остался зелёным — значит гейт не
проверен ничем, и это не гейт, а декорация.

Правило выведено из собственных граблей, и все они были одной формы: гейт, который умел отвечать
только «OK». `|| true` в страже prod-ветки проглатывал даже поломку регулярки. `all([])` — это
`True`, поэтому зрение, которое НЕ ОТРАБОТАЛО, читалось как «зрение не возражает». Критерий
сходимости считал зелёный репозиторий заслугой агента, не тронувшего ни строки.

Ни одну из них не поймали 86 юнит-тестов: они проверяли, что гейт пропускает хорошее, и ни разу —
что он задерживает плохое.

Мета-тест сам проверен тем же правилом: подсунутый заведомо непокрытый «гейт» он ловит (см.
prove_gate.py, код возврата 1).
"""
import subprocess
import sys
import unittest
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PROVE = ROOT / "scripts" / "prove_gate.py"

GATES = [
    "clave_dev.loop:converged",
    "clave_dev.loop:outcome",
    "clave_dev.visual_verdict:verdict_passes",
    "clave_dev.visual_verdict:consensus_verdict",
    "clave_dev.visual_verdict:is_infra_failure",
    "clave_dev.assertions:evaluate",
    "clave_dev.checks:parse_test_failures",
    "clave_dev.diff:changed_paths",
]


def _prove(gate: str):
    """Один прогон набора с обезвреженным гейтом. Возвращает жалобу или None."""
    res = subprocess.run(
        [sys.executable, str(PROVE), gate],
        capture_output=True,
        text=True,
        cwd=str(ROOT),
        check=False,
    )
    if res.returncode == 0:
        return None
    return f"{gate}: {res.stderr.strip() or res.stdout.strip()}"


class GatesCanFailTest(unittest.TestCase):
    def test_every_gate_is_proven_by_at_least_one_test(self):
        # Параллельно: каждый гейт — отдельный прогон всего набора, последовательно это 18с на
        # КАЖДОМ раунде петли. Правило должно быть дешёвым, иначе его начнут отключать — а
        # отключаемое правило снова превращается в совет.
        with ThreadPoolExecutor(max_workers=len(GATES)) as pool:
            theatre = [c for c in pool.map(_prove, GATES) if c]

        self.assertFalse(
            theatre,
            "гейт, который нельзя провалить, — декорация:\n" + "\n".join(theatre),
        )

    def test_the_list_of_gates_is_not_quietly_empty(self):
        # Мета-тест, который ничего не проверяет, — сам декорация. Пустой список гейтов дал бы
        # вечно-зелёный результат, и это ровно та болезнь, которую он лечит.
        self.assertGreaterEqual(len(GATES), 8)
        self.assertTrue(PROVE.is_file(), "скрипт мутации пропал — правило перестало действовать")


if __name__ == "__main__":
    unittest.main()
