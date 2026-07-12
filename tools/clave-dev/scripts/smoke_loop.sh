#!/bin/bash
# End-to-end смоук супервайзера на моках: тривиальная задача, реальный headless clave
# (known-good) с мок-провайдерами, свежий build в observer. Проверяем, что петля
# отрабатывает раунд, собирает отчёт и завершается (сходимость на моках не требуется —
# моки не правят код). known-good ОБЯЗАН поддерживать --run (сборка ветки, не main).
set -u
root="$(cd "$(dirname "$0")/../../.." && pwd)"        # корень репо clave
selfdev="$root/scripts/selfdev"
kg="${1:?путь к known-good clave с поддержкой --run (напр. target/release/clave ветки)}"
py="${2:?путь к python с установленным pyte (venv)}"
export CLAVE_CLAUDE="$selfdev/mock-claude.sh" CLAVE_CODEX="$selfdev/mock-codex.sh"
cd "$root/tools/clave-dev"
out="$("$py" -m clave_dev "поменяй подпись в футере" \
  --repo "$root" --known-good "$kg" --build-profile debug --max-rounds 1)"
code=$?
printf '%s\n' "$out"
printf '%s\n' "$out" | grep -q "итог прогона" \
  && echo "SMOKE OK: loop ran and produced a report (exit $code)" \
  || { echo "SMOKE FAIL: нет отчёта"; exit 1; }
