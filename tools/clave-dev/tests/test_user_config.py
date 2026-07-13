import tempfile
import unittest
from pathlib import Path

from clave_dev.user_config import (
    config_mode,
    seed_config,
    single_model_warning,
    user_config_path,
)

REAL_CONFIG = """onboarding_done=true
mode="claude-codex"
theme="amber"
rounds=2
claude_effort="max"
codex_effort="xhigh"
last_chat="chat-1783853936836"
"""


class UserConfigTest(unittest.TestCase):
    def test_run_keeps_the_configured_mode_instead_of_falling_back_to_defaults(self):
        # Тот самый дефект. CLAVE_HOME уводится в temp — правильно, иначе прогон лез бы в чаты
        # пользователя. Но пустой home означал ДЕФОЛТНЫЙ конфиг, а дефолтный режим — codex-only:
        # исполнитель и критик становились одной моделью. «Тандем» тихо превращался в самокритику,
        # и заметить это было неоткуда — ярлык не менялся.
        with tempfile.TemporaryDirectory() as tmp:
            src = Path(tmp) / "real"
            src.write_text(REAL_CONFIG)
            home = Path(tmp) / "home"
            home.mkdir()

            seeded = seed_config(src, home)

            self.assertEqual(config_mode(seeded), "claude-codex")

    def test_pointer_to_the_users_chats_is_not_carried_over(self):
        # last_chat указывает в чаты, которых в изолированном home нет.
        with tempfile.TemporaryDirectory() as tmp:
            src = Path(tmp) / "real"
            src.write_text(REAL_CONFIG)
            home = Path(tmp) / "home"
            home.mkdir()

            text = seed_config(src, home).read_text()

            self.assertNotIn("last_chat", text)
            self.assertIn('theme="amber"', text)  # тема нужна: зрение судит рендер пользователя

    def test_missing_config_is_reported_not_guessed(self):
        with tempfile.TemporaryDirectory() as tmp:
            self.assertIsNone(seed_config(Path(tmp) / "нет-такого", Path(tmp)))

    def test_config_path_follows_the_same_order_as_the_product(self):
        self.assertEqual(user_config_path({"CLAVE_CONFIG": "/x/cfg"}), Path("/x/cfg"))
        self.assertEqual(user_config_path({"CLAVE_HOME": "/x/home"}), Path("/x/home/config"))
        self.assertEqual(user_config_path({}), Path.home() / ".clave" / "config")

    def test_single_model_tandem_is_called_out(self):
        # Молчать нельзя: прогон называется тандемом, а второго независимого взгляда в нём нет.
        for mode in ("codex-only", "claude-only"):
            self.assertIn("самокритику", single_model_warning(mode))
        for mode in ("claude-codex", "codex-claude"):
            self.assertIsNone(single_model_warning(mode))


if __name__ == "__main__":
    unittest.main()


class CostWarningTest(unittest.TestCase):
    """Инструмент обязан назвать свою цену ДО того, как человек начнёт ждать.

    Дефект был не в том, что прогон долгий, а в том, что он МОЛЧАЛ о длительности: «раунд 1: агент
    правит код» на неподвижном экране не говорит, отойти за кофе или на полдня. Замерено: тандем с
    rounds=2 и effort=high спорил около двух часов над задачей в 50 строк.
    """

    def test_a_generous_tandem_says_so_up_front(self):
        from clave_dev.user_config import cost_warning

        warn = cost_warning("2", "high")

        self.assertIsNotNone(warn)
        self.assertIn("дебатов: 2", warn)
        self.assertIn("--rounds 1", warn, "предупреждение обязано сказать, ЧТО делать, а не только пугать")

    def test_a_cheap_run_stays_quiet(self):
        # Предупреждение, которое горит всегда, перестают читать — и отключают.
        from clave_dev.user_config import cost_warning

        self.assertIsNone(cost_warning("1", "medium"))
        self.assertIsNone(cost_warning(None, None))

    def test_high_effort_alone_is_enough_to_warn(self):
        from clave_dev.user_config import cost_warning

        self.assertIsNotNone(cost_warning("1", "max"))

    def test_a_broken_config_value_does_not_crash_the_run(self):
        # Конфиг пишет человек. Мусор в нём — не повод ронять прогон ДО его начала.
        from clave_dev.user_config import cost_warning

        self.assertIsNone(cost_warning("не-число", None))


class ConfigValueTest(unittest.TestCase):
    def test_it_reads_any_key(self):
        import tempfile
        from pathlib import Path

        from clave_dev.user_config import config_value

        with tempfile.TemporaryDirectory() as tmp:
            cfg = Path(tmp) / "config"
            cfg.write_text('mode="claude-codex"\nrounds=2\neffort="high"\n')

            self.assertEqual(config_value(cfg, "rounds"), "2")
            self.assertEqual(config_value(cfg, "effort"), "high")
            self.assertIsNone(config_value(cfg, "нет-такого"))

    def test_a_missing_file_is_not_a_crash(self):
        from pathlib import Path

        from clave_dev.user_config import config_value

        self.assertIsNone(config_value(Path("/нет/такого/конфига"), "rounds"))
