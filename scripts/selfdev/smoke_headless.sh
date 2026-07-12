#!/bin/bash
# Прогоняет `clave --run tandem` на мок-провайдерах и проверяет контракт:
# в stdout есть ровно одна строка CLAVE-RUN с валидным json и exit 0.
set -u
here="$(cd "$(dirname "$0")" && pwd)"
bin="${1:?путь к target/release/clave}"
home="$(mktemp -d)"
wt="$(mktemp -d)"
out="$(CLAVE_HOME="$home" CLAVE_SKIP_ONBOARDING=1 \
  CLAVE_CLAUDE="$here/mock-claude.sh" CLAVE_CODEX="$here/mock-codex.sh" \
  "$bin" --run tandem --cwd "$wt" --rounds 1 "smoke task" 2>/dev/null)"
code=$?
echo "$out"
final="$(printf '%s\n' "$out" | grep -c '^CLAVE-RUN ')"
json="$(printf '%s\n' "$out" | grep '^CLAVE-RUN ' | tail -1 | sed 's/^CLAVE-RUN //')"
python3 -c "import json,sys; json.loads(sys.argv[1])" "$json" || { echo "FAIL: невалидный json"; exit 1; }
[ "$final" = "1" ] || { echo "FAIL: ожидалась ровно одна строка CLAVE-RUN, получено $final"; exit 1; }
[ "$code" = "0" ] || { echo "FAIL: exit=$code, ожидался 0"; exit 1; }
echo "OK: headless smoke passed (exit 0, one CLAVE-RUN line, valid json)"
