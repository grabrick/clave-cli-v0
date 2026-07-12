import unittest

from clave_dev.terminal_profile import (
    apply_geometry_applescript,
    default_profile,
    describe,
    geometry_label,
    read_geometry_applescript,
)


class TerminalProfileTest(unittest.TestCase):
    def test_describe_is_flat_dict_with_all_fields(self):
        d = describe(default_profile())
        for key in ("app", "cols", "rows", "font", "font_size", "theme", "opacity", "locale", "bounds"):
            self.assertIn(key, d)

    def test_geometry_is_set_in_character_cells_not_only_pixels(self):
        # Пиксельных bounds мало: пересчёт в колонки делает Terminal, и он зависел от того, успело
        # ли окно открыться — в одном прогоне база вышла 123×39, а свежая сборка 120×30. А весь
        # required-чеклист зрения про ШИРИНУ, так что рендеры разной ширины сравнивать нельзя.
        p = default_profile()._replace(bounds=(10, 20, 800, 600), cols=100, rows=30)

        script = apply_geometry_applescript(p, window_id=42)

        self.assertIn("{10, 20, 810, 620}", script)  # x2=x+w, y2=y+h
        self.assertIn("set number of columns of window id 42 to 100", script)
        self.assertIn("set number of rows of window id 42 to 30", script)

    def test_geometry_can_be_read_back_for_verification(self):
        # Задать мало — надо убедиться, что задалось: иначе вердикт будет о рендере, которого
        # мы не заказывали.
        script = read_geometry_applescript(42)
        self.assertIn("number of columns of window id 42", script)
        self.assertIn("number of rows of window id 42", script)
        self.assertEqual(geometry_label(default_profile()._replace(cols=100, rows=30)), "100x30")


class ObserverProfileTest(unittest.TestCase):
    """Профиль наблюдателя обязан совпадать с рабочим профилем пользователя: ловимые баги
    (срез у стенки, обрезанные глифы) зависят от ШРИФТА. Иначе зрение молча судит рендер,
    которого пользователь никогда не видит."""

    def _props(self, observer_bg, observer_font):
        table = {
            ("default settings", "name"): "Clear Dark",
            ("default settings", "background color"): "6447,7462,10000",
            ("default settings", "font name"): "SFMono-Regular",
            ('settings set "clave-dev"', "background color"): observer_bg,
            ('settings set "clave-dev"', "font name"): observer_font,
        }
        return lambda target, prop: table.get((target, prop), "")

    def test_mismatch_is_reported(self):
        from clave_dev.terminal_profile import observer_profile_mismatch

        reason = observer_profile_mismatch(
            "clave-dev", get_prop=self._props("65535,65535,65535", "Menlo-Regular")
        )
        self.assertIn("не совпадает", reason)
        self.assertIn("Clear Dark", reason)

    def test_identical_profile_is_silent(self):
        from clave_dev.terminal_profile import observer_profile_mismatch

        self.assertIsNone(
            observer_profile_mismatch(
                "clave-dev", get_prop=self._props("6447,7462,10000", "SFMono-Regular")
            )
        )
