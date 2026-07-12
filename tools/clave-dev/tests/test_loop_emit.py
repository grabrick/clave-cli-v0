import unittest

from clave_dev.emit import Emitter


class CapturingOut:
    def __init__(self):
        self.lines = []

    def write(self, s):
        if s.strip():
            self.lines.append(s.strip())

    def flush(self):
        pass


class LoopEmitTest(unittest.TestCase):
    def test_emitter_lines_are_framed_by_type(self):
        out = CapturingOut()
        em = Emitter(enabled=True, out=out)
        em.progress("round 1")
        em.check({"name": "build", "ok": True})
        em.report({"converged": False, "rounds": 1})
        self.assertTrue(any(l.startswith("CLAVE-DEV progress") for l in out.lines))
        self.assertTrue(any(l.startswith("CLAVE-DEV check") for l in out.lines))
        self.assertTrue(any(l.startswith("CLAVE-DEV report") for l in out.lines))
