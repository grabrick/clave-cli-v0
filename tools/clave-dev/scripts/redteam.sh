#!/bin/bash
# Red-team: совершаем каждый известный обход и смотрим, ловит ли защита.
#
# Защита, не проверенная атакой, — декларация. Гейт, который умеет отвечать только «OK», выглядит
# точно так же, как работающий, — отличить их можно ТОЛЬКО попыткой обхода. Этот скрипт и есть
# доказательство; он живёт в репозитории, чтобы его можно было перепроверить, а не поверить мне.
#
#   ./scripts/redteam.sh
#
# Два раунда, и разница между ними — суть.
#
# РАУНД 1 — просто сломать защиту. Ловится почти всё, но часть ловит только RULES.lock, а замок
# это не запрет, а СИРЕНА: `relock_rules.py` выключает её одной командой.
#
# РАУНД 2 — сломать И перевыпустить замок, как поступит уставший человек, которому правило
# «мешает». Что осталось красным здесь — настоящий запрет. Что позеленело — держится только на
# человеке, читающем дифф.
#
# Именно раунд 2 нашёл дыру, которую раунд 1 показывал как «поймано»: prove_gate.py, выпотрошенный
# до `sys.exit(0)`, отвечал «доказано» на все девять гейтов разом.

set -u
cd "$(dirname "$0")/.." || exit 1
PY=${CLAVE_DEV_PYTHON:-.venv/bin/python3}
[ -x "$PY" ] || PY=python3

WF=../../.github/workflows/dev-rules.yml
REL=../../.github/workflows/release.yml
GUARD=../../.github/workflows/self-dev-guard.yml
DIST=../../dist-workspace.toml
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

pass=0
holes=0

# Зелёный — ровно то, что считает зелёным сама петля (py_ok): код 0 И ПРЕДЪЯВЛЕННОЕ число тестов.
#
# Одного кода возврата мало, и это не теория: `sys.exit(0)` в любом модуле, который импортирует
# тест, гасит unittest молча — ноль тестов, пустой вывод, код 0. Прогон, который не состоялся,
# читается как прогон, который прошёл. Инструмент проверки обязан быть честнее проверяемого.
rules_are_green() {
	local out code ran
	out=$($PY -m unittest tests.test_gates_can_fail tests.test_unverified \
		tests.test_no_dead_modules tests.test_rules_are_enforced 2>&1)
	code=$?
	ran=$(printf '%s' "$out" | grep -oE '^Ran ([0-9]+) tests?' | grep -oE '[0-9]+' || echo 0)
	[ "$code" -eq 0 ] && [ "${ran:-0}" -ge 4 ]
}

# $1 — что делаем, $2 — чем ломаем, $3 — чем чиним, $4 — relock после поломки (да/нет),
# $5 — «не-в-раунде-2», если атака теряет смысл после перевыпуска замка.
attack() {
	local name="$1" break_cmd="$2" restore_cmd="$3" relock="${4:-нет}" only_round1="${5:-}"

	# Снести замок и тут же перевыпустить — значит вернуть его на место: атака отменяет сама себя.
	# Пропускаем, но ВСЛУХ: молчаливо выкинутая проверка читается как пройденная — ровно та
	# болезнь, которую этот скрипт и ищет.
	if [ -n "$only_round1" ] && [ "$relock" = "да" ]; then
		printf '  \033[90m— пропуск\033[0m      %s (relock возвращает файл — атака бессмысленна)\n' "$name"
		return
	fi

	eval "$break_cmd" >/dev/null 2>&1
	[ "$relock" = "да" ] && $PY scripts/relock_rules.py >/dev/null 2>&1

	if rules_are_green; then
		printf '  \033[31m✗ ПРОШЁЛ\033[0m       %s\n' "$name"
		holes=$((holes + 1))
	else
		printf '  \033[32m✓ пойман\033[0m       %s\n' "$name"
		pass=$((pass + 1))
	fi

	eval "$restore_cmd" >/dev/null 2>&1
	$PY scripts/relock_rules.py >/dev/null 2>&1
}

run_round() {
	local relock="$1"

	for rule in test_gates_can_fail test_unverified test_no_dead_modules test_rules_are_enforced; do
		attack "снести правило $rule" \
			"mv tests/$rule.py $TMP/" "mv $TMP/$rule.py tests/" "$relock"
	done

	attack "выпотрошить правило 1 (GATES = [])" \
		"cp tests/test_gates_can_fail.py $TMP/g.py && printf '\nGATES = []\n' >> tests/test_gates_can_fail.py" \
		"cp $TMP/g.py tests/test_gates_can_fail.py" "$relock"

	# Тот самый обход, который раунд 1 показывал как пойманный: скрипт цел, отвечает «0» — и
	# девять гейтов разом читаются как доказанные. Ловится канарейкой (гейт, не проверенный
	# ничем: на нём скрипт ОБЯЗАН сказать «нет») и счётчиком прогнанных тестов.
	attack "выпотрошить prove_gate.py до sys.exit(0)" \
		"cp scripts/prove_gate.py $TMP/p.py && printf 'import sys; sys.exit(0)\n' > scripts/prove_gate.py" \
		"cp $TMP/p.py scripts/prove_gate.py" "$relock"

	attack "снести RULES.lock" \
		"mv RULES.lock $TMP/" "mv $TMP/RULES.lock ." "$relock" "не-в-раунде-2"

	attack "снести workflow правил (тишина вместо красного)" \
		"mv $WF $TMP/dr.yml" "mv $TMP/dr.yml $WF" "$relock"

	attack "continue-on-error (шаг падает молча)" \
		"cp $WF $TMP/dr.yml && printf '\n# continue-on-error: true\n' >> $WF" \
		"cp $TMP/dr.yml $WF" "$relock"

	attack "убрать имя правила из RULE_TESTS" \
		"cp clave_dev/checks.py $TMP/c.py && sed -i '' '/test_no_dead_modules\",/d' clave_dev/checks.py" \
		"cp $TMP/c.py clave_dev/checks.py" "$relock"

	# Тег — второй путь в прод: cargo-dist собирает и ПУБЛИКУЕТ релиз по тегу с любой ветки,
	# мимо стража main.
	attack "снести стража релиза" \
		"mv $GUARD $TMP/sg.yml" "mv $TMP/sg.yml $GUARD" "$relock"

	attack "регенерация выбросила стража из release.yml" \
		"cp $REL $TMP/r.yml && sed -i '' '/self-dev-guard/d' $REL" \
		"cp $TMP/r.yml $REL" "$relock"

	attack "убрать plan-jobs из dist-workspace.toml" \
		"cp $DIST $TMP/d.toml && sed -i '' '/plan-jobs/d' $DIST" \
		"cp $TMP/d.toml $DIST" "$relock"

	attack "модуль, который не гоняет ни один тест" \
		"printf 'def never_called():\n    return 42\n' > clave_dev/nobody_runs_me.py" \
		"rm -f clave_dev/nobody_runs_me.py" "$relock"

	# Прогон, который НЕ СОСТОЯЛСЯ, не должен читаться как прогон, который прошёл.
	attack "sys.exit(0) в clave_dev/checks.py" \
		"cp clave_dev/checks.py $TMP/c2.py && printf 'import sys; sys.exit(0)\n' | cat - clave_dev/checks.py > $TMP/x && mv $TMP/x clave_dev/checks.py" \
		"cp $TMP/c2.py clave_dev/checks.py" "$relock"

	attack "снести весь каталог tests/" \
		"mv tests $TMP/tests" "mv $TMP/tests ." "$relock"
}

echo "════ РАУНД 1: сломать защиту ════"
run_round "нет"

echo
echo "════ РАУНД 2: сломать И перевыпустить замок (сирена выключена) ════"
run_round "да"

echo
echo "════════════════════════════════════════════════════"
printf '  поймано: %s     ПРОШЛО: %s\n' "$pass" "$holes"
if [ "$holes" -eq 0 ]; then
	echo "  все известные обходы закрыты — и по существу, а не только замком"
else
	echo "  ⚠ ЕСТЬ ДЫРЫ: защита, которую можно снять молча"
fi
exit "$holes"
