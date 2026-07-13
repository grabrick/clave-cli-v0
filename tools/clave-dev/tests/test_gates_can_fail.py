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
sys.path.insert(0, str(ROOT))

from scripts.prove_gate import CANARY  # noqa: E402

PROVE = ROOT / "scripts" / "prove_gate.py"

GATES = [
    "clave_dev.loop:converged",
    "clave_dev.loop:outcome",
    "clave_dev.visual_verdict:verdict_passes",
    "clave_dev.visual_verdict:consensus_verdict",
    "clave_dev.visual_verdict:is_infra_failure",
    "clave_dev.assertions:evaluate",
    "clave_dev.checks:parse_test_failures",
    "clave_dev.checks:tests_ran",
    "clave_dev.diff:changed_paths",
    "clave_dev.mutation:unproven",
]


def _run(gate: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, str(PROVE), gate],
        capture_output=True,
        text=True,
        cwd=str(ROOT),
        check=False,
    )


def _prove(gate: str):
    """Один прогон набора с обезвреженным гейтом. Возвращает жалобу или None."""
    res = _run(gate)
    if res.returncode != 0:
        return f"{gate}: {res.stderr.strip() or res.stdout.strip()}"
    # Кода возврата мало: скрипт обязан ПРЕДЪЯВИТЬ, что он доказал. Молчаливый ноль — это ровно
    # «ничего не сделал», и отличить его от «проверил, всё сошлось» нечем.
    if gate not in res.stdout:
        return f"{gate}: скрипт ответил «доказано», но не сказал ЧТО: {res.stdout.strip()!r}"
    return None


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

    def test_the_prover_itself_can_say_no(self):
        """Правило 1, применённое к самому доказательству: оно обязано уметь провалиться.

        До этого теста правило держалось на честном слове скрипта. Red-team-прогон это и вскрыл:
        заменяешь весь prove_gate.py на `import sys; sys.exit(0)`, перевыпускаешь замок — и девять
        гейтов разом читаются как доказанные. Набор зелёный, правила нет.

        Канарейка — гейт, не проверенный ни одним тестом. На нём скрипт ОБЯЗАН сказать «нет».
        Умеющий отвечать только «OK» тут и ловится, и никакой перевыпуск замка не спасает.
        """
        res = _run(CANARY)

        self.assertEqual(
            res.returncode,
            1,
            "prove_gate ответил «доказано» на гейт, не проверенный ничем — значит он отвечает "
            f"«доказано» вообще всегда, и правило 1 — декорация.\n{res.stdout}{res.stderr}",
        )

    def test_the_list_of_gates_is_not_quietly_empty(self):
        # Мета-тест, который ничего не проверяет, — сам декорация. Пустой список гейтов дал бы
        # вечно-зелёный результат, и это ровно та болезнь, которую он лечит.
        self.assertGreaterEqual(len(GATES), 10)
        self.assertTrue(PROVE.is_file(), "скрипт мутации пропал — правило перестало действовать")


if __name__ == "__main__":
    unittest.main()
