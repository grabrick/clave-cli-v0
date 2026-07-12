import unittest

from clave_dev.window_resolve import WindowNotFoundError, resolve_cgwindow_id


def _w(owner, name, num):
    return {"kCGWindowOwnerName": owner, "kCGWindowName": name, "kCGWindowNumber": num}


class WindowResolveTest(unittest.TestCase):
    def test_single_match_returns_cgwindow_number(self):
        infos = [_w("Finder", "x", 1), _w("Terminal", "clave-dev abc123", 42)]
        self.assertEqual(resolve_cgwindow_id(infos, "Terminal", "clave-dev abc123"), 42)

    def test_zero_matches_raises(self):
        with self.assertRaises(WindowNotFoundError):
            resolve_cgwindow_id([_w("Finder", "x", 1)], "Terminal", "clave-dev abc")

    def test_multiple_matches_raises(self):
        infos = [_w("Terminal", "clave-dev z", 7), _w("Terminal", "clave-dev z", 8)]
        with self.assertRaises(WindowNotFoundError):
            resolve_cgwindow_id(infos, "Terminal", "clave-dev z")
