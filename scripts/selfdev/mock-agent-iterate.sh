#!/bin/bash
# Мок-провайдер, который ЗАСТАВЛЯЕТ петлю сделать второй раунд.
#
# Раунд 1: вносит заведомо ЛОМАЮЩУЮ правку (падающий python-тест) — супервайзер обязан
#          увидеть красную проверку, НЕ сойтись и вернуть агенту фидбэк.
# Раунд 2: чинит правку — супервайзер обязан сойтись.
#
# ВАЖНО, как отличается раунд. Тандем зовёт провайдера НЕСКОЛЬКО раз за раунд (исполнитель,
# критик, ревью), поэтому ключеваться на состоянии правки нельзя: первый вызов сломает,
# второй тут же «починит» — всё внутри одного раунда, и петля не зациклится (проверено).
# Нужен признак, меняющийся строго МЕЖДУ раундами: `target/`. В свежем worktree его нет,
# а создаёт его `cargo build` на фазе проверок — то есть после всех вызовов агента раунда.
# Правки идемпотентны, поэтому сколько бы раз ни позвали внутри раунда — результат один.
set -u

case "$*" in
  *"auth status"*|*"login status"*) echo "logged in as mock-agent"; exit 0 ;;
esac

PROBE="tools/clave-dev/tests/test_zz_probe.py"

if [ -d "target" ]; then
  NOTE="раунд 2+: увидел фидбэк о падении — чиню тест"
  cat > "$PROBE" <<'PY'
import unittest


class ProbeTest(unittest.TestCase):
    def test_probe(self):
        self.assertTrue(True)
PY
else
  NOTE="раунд 1: вношу правку (намеренно ломающую — петля обязана это поймать)"
  cat > "$PROBE" <<'PY'
import unittest


class ProbeTest(unittest.TestCase):
    def test_probe(self):
        # Намеренно сломано: супервайзер обязан увидеть красный python-набор,
        # не объявить сходимость и вернуть агенту фидбэк.
        self.assertTrue(False, "сломано намеренно")
PY
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
