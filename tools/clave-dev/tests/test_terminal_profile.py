import unittest

from clave_dev.terminal_profile import apply_bounds_applescript, default_profile, describe


class TerminalProfileTest(unittest.TestCase):
    def test_describe_is_flat_dict_with_all_fields(self):
        d = describe(default_profile())
        for key in ("app", "cols", "rows", "font", "font_size", "theme", "opacity", "locale", "bounds"):
            self.assertIn(key, d)

    def test_apply_bounds_applescript_uses_x2_y2(self):
        p = default_profile()._replace(bounds=(10, 20, 800, 600))
        script = apply_bounds_applescript(p)
        self.assertIn("Terminal", script)
        self.assertIn("{10, 20, 810, 620}", script)  # x2=x+w, y2=y+h


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
