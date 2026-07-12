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
