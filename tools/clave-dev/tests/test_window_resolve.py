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

    def test_localized_owner_still_resolves_by_unique_title(self):
        # Найдено вживую: AppleScript зовёт "Terminal", а CGWindowList отдаёт «Терминал».
        # Уникальный титул-nonce обязан вытащить окно несмотря на локализованного владельца.
        infos = [
            _w("Пункт управления", "Item-0", 3),
            _w("Терминал", "kirill — clave-dev-probe da3ec2ef — clave — 49×36", 77),
        ]
        self.assertEqual(
            resolve_cgwindow_id(infos, "Terminal", "clave-dev-probe da3ec2ef"), 77
        )

    def test_owner_disambiguates_when_title_matches_several(self):
        infos = [
            _w("Терминал", "clave-dev-probe abc", 10),
            _w("Preview", "clave-dev-probe abc", 11),
        ]
        self.assertEqual(resolve_cgwindow_id(infos, "Preview", "clave-dev-probe abc"), 11)
