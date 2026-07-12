#!/bin/bash
# Мок-провайдер, который ЗАСТАВЛЯЕТ петлю сделать второй раунд.
#
# Раунд 1: вносит заведомо ЛОМАЮЩУЮ правку (падающий python-тест) — супервайзер обязан
#          увидеть красную проверку, НЕ сойтись и дать фидбэк.
# Раунд 2: видит, что правка сломана, и чинит её — супервайзер обязан сойтись.
#
# Состояние берём из самого worktree (провайдер запускается с cwd = worktree), а не из
# промпта — так мок не зависит от того, как именно clave передаёт задачу.
set -u

case "$*" in
  *"auth status"*|*"login status"*) echo "logged in as mock-agent"; exit 0 ;;
esac

PROBE="tools/clave-dev/tests/test_zz_probe.py"
NOTE=""

if [ ! -f "$PROBE" ]; then
  NOTE="раунд 1: вношу правку (намеренно ломающую — петля обязана это поймать)"
  cat > "$PROBE" <<'PY'
import unittest


class ProbeTest(unittest.TestCase):
    def test_probe(self):
        # Намеренно сломано: супервайзер обязан увидеть красный python-набор,
        # не объявить сходимость и вернуть агенту фидбэк.
        self.assertTrue(False, "сломано намеренно")
PY
elif grep -q "assertTrue(False" "$PROBE" 2>/dev/null; then
  NOTE="раунд 2: увидел фидбэк о падении — чиню"
  cat > "$PROBE" <<'PY'
import unittest


class ProbeTest(unittest.TestCase):
    def test_probe(self):
        self.assertTrue(True)
PY
else
  NOTE="править нечего"
fi

# Ответ в формате провайдера (codex пишет ответ в файл из -o, claude — stream-json).
outfile=""; prev=""
for a in "$@"; do
  [ "$prev" = "-o" ] && outfile="$a"
  prev="$a"
done
if [ -n "$outfile" ]; then
  printf '%s\n' "$NOTE" > "$outfile"
  printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":10,"output_tokens":5}}'
else
  printf '{"type":"content_block_delta","delta":{"type":"text_delta","text":"%s"}}\n' "$NOTE"
  printf '%s\n' "{\"type\":\"result\",\"result\":\"$NOTE\",\"is_error\":false,\"usage\":{\"input_tokens\":10,\"output_tokens\":5}}"
fi
exit 0
