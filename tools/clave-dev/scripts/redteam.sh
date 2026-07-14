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

# ЭТОТ СКРИПТ ЛОМАЕТ РАБОЧЕЕ ДЕРЕВО. Пока он идёт, файлы правил, гейтов и CI-стражей физически
# отсутствуют или испорчены — на доли секунды, но по очереди и подолгу.
#
# Замок нужен от того, что уже случилось: я запустил red-team в фоне, а сам параллельно сделал
# `git add -A && git commit`. В коммит попал release.yml БЕЗ СТРАЖА — ровно та дыра, через которую
# тег с develop опубликовал бы пользователям бинарь с инструментами саморазработки. Коммит не был
# запушен только по случайности.
#
# Инструмент, который ломает дерево, обязан кричать об этом и не давать себя запустить дважды.
LOCK=.redteam.lock
if [ -e "$LOCK" ]; then
	echo "red-team уже идёт (замок $LOCK). Дождись его: он ломает дерево, и параллельный" >&2
	echo "коммит утащит в историю СНЕСЁННУЮ защиту. Если это мёртвый замок — удали его руками." >&2
	exit 2
fi
echo "$$" >"$LOCK"

cat >&2 <<'ПРЕДУПРЕЖДЕНИЕ'
  ⚠ red-team ЛОМАЕТ рабочее дерево: сносит правила, потрошит гейты, выдирает стражей из CI.
    Пока он идёт — НЕ коммить и не гоняй тесты: `git add -A` утащит в историю снесённую защиту.
    Всё унесённое возвращается в конце и при любом убийстве (INT/TERM), но не при `kill -9`.

ПРЕДУПРЕЖДЕНИЕ

WF=../../.github/workflows/dev-rules.yml
REL=../../.github/workflows/release.yml
GUARD=../../.github/workflows/self-dev-guard.yml
DIST=../../dist-workspace.toml
TMP=$(mktemp -d)

# Снимок дерева ДО погрома. Сравнить в конце с «чистым» нельзя: у человека могут быть свои
# незакоммиченные правки, и объявить их следом атаки — значит поднять ложную тревогу. Сравниваем
# с тем, что БЫЛО, — тогда любое расхождение и есть след, оставленный скриптом.
BEFORE=$(git status --porcelain)

# ВОССТАНОВЛЕНИЕ ПРИ ЛЮБОМ ИСХОДЕ, включая убийство.
#
# Скрипт ломает защиту нарочно: сносит правила, потрошит гейты, выдирает стража из CI. Прежний
# `trap 'rm -rf "$TMP"' EXIT` при этом УНИЧТОЖАЛ единственную копию: если скрипт убить между
# «снести правило» и «вернуть на место», файл оставался в $TMP — и trap сносил $TMP вместе с ним.
#
# Так и вышло: red-team прервали на атаке «снести test_rules_are_enforced», и правило исчезло.
# Спасло только то, что оно в git. Инструмент, который проверяет защиту, не имеет права её убить.
#
# Теперь восстановление идёт ПЕРЕД уборкой и ловит не только EXIT, но и INT/TERM. Возвращаем
# ровно то, что унесли: git тут не помощник — он затёр бы незакоммиченные правки заодно.
restore_all() {
	# Файлы, унесённые в $TMP целиком (mv): вернуть по исходным путям.
	[ -f "$TMP/test_gates_can_fail.py" ] && mv -f "$TMP/test_gates_can_fail.py" tests/
	[ -f "$TMP/test_unverified.py" ] && mv -f "$TMP/test_unverified.py" tests/
	[ -f "$TMP/test_no_dead_modules.py" ] && mv -f "$TMP/test_no_dead_modules.py" tests/
	[ -f "$TMP/test_rules_are_enforced.py" ] && mv -f "$TMP/test_rules_are_enforced.py" tests/
	# Тест гейта устойчивости: без него гейт становится НЕДОКАЗУЕМЫМ, то есть убитый на этой
	# атаке скрипт оставил бы защиту выключенной.
	[ -f "$TMP/test_flake.py" ] && mv -f "$TMP/test_flake.py" tests/
	[ -f "$TMP/RULES.lock" ] && mv -f "$TMP/RULES.lock" .
	[ -f "$TMP/dr.yml" ] && mv -f "$TMP/dr.yml" "$WF"
	[ -f "$TMP/sg.yml" ] && mv -f "$TMP/sg.yml" "$GUARD"
	[ -d "$TMP/tests" ] && mv -f "$TMP/tests" .
	# Файлы, испорченные на месте (cp): вернуть копию.
	[ -f "$TMP/g.py" ] && cp -f "$TMP/g.py" tests/test_gates_can_fail.py
	[ -f "$TMP/p.py" ] && cp -f "$TMP/p.py" scripts/prove_gate.py
	[ -f "$TMP/c.py" ] && cp -f "$TMP/c.py" clave_dev/checks.py
	[ -f "$TMP/c2.py" ] && cp -f "$TMP/c2.py" clave_dev/checks.py
	[ -f "$TMP/r.yml" ] && cp -f "$TMP/r.yml" "$REL"
	[ -f "$TMP/d.toml" ] && cp -f "$TMP/d.toml" "$DIST"
	# Гейт устойчивости, выпотрошенный до «всегда устойчив»: оставить его таким — вернуть
	# мутационному гейту право врать.
	[ -f "$TMP/fl.py" ] && cp -f "$TMP/fl.py" clave_dev/flake.py
	rm -f clave_dev/nobody_runs_me.py
	rm -f "$LOCK"
	return 0
}
# EXIT — просто уборка. INT/TERM — уборка И ВЫХОД: без явного exit bash выполняет обработчик и
# продолжает скрипт дальше, снова ломая то, что только что вернул. Замерено: после TERM red-team
# как ни в чём не бывало шёл дальше по атакам.
trap 'restore_all; rm -rf "$TMP"' EXIT
trap 'restore_all; rm -rf "$TMP"; exit 130' INT TERM

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

	# Гейт устойчивости. Выпотрошить его — значит вернуть мутационному гейту право ВРАТЬ:
	# флейкующий тест снова начнёт красить набор сам по себе, и cargo mutants запишет
	# непойманных мутантов в пойманные (замерено: 104 «выживших» против 129 настоящих).
	#
	# Подмена в конце модуля перекрывает настоящую функцию — так же тихо, как `sys.exit(0)`.
	attack "выпотрошить гейт устойчивости (unstable → [])" \
		"cp clave_dev/flake.py $TMP/fl.py && printf '\n\ndef unstable(*a, **k):\n    return []\n' >> clave_dev/flake.py" \
		"cp $TMP/fl.py clave_dev/flake.py" "$relock"

	# А это проверка ЦЕПОЧКИ, а не одного звена. Тест гейта не входит в мета-набор, который
	# смотрит red-team, — значит, снеся его, защиту можно было бы выключить «в обход». Ловить
	# обязано правило 1: без теста гейт становится НЕДОКАЗУЕМЫМ, и `test_gates_can_fail` кричит
	# «гейт, который нельзя провалить, — декорация». Если эта атака пройдёт — цепочка рвётся, и
	# любой новый гейт можно выключить, просто удалив его тест.
	#
	# Восстановление сохраняет ИМЯ. Первая версия уносила файл как `$TMP/tf.py` и возвращала
	# командой `mv $TMP/tf.py tests/` — то есть клала его обратно под именем бэкапа, а
	# `test_flake.py` так и оставался удалённым. Скрипт, который ломает дерево, обязан возвращать
	# его в ТО ЖЕ состояние, а не в похожее: иначе следующим шагом это уедет в коммит.
	attack "снести тест гейта устойчивости (выключить защиту в обход)" \
		"mv tests/test_flake.py $TMP/" "mv $TMP/test_flake.py tests/" "$relock"
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

# ДЕРЕВО ОБЯЗАНО ВЕРНУТЬСЯ В ТО ЖЕ СОСТОЯНИЕ. Проверяем, а не верим.
#
# Скрипт не проверял этого никогда — и однажды отрапортовал «0 дыр, все обходы закрыты», оставив
# `test_flake.py` удалённым, а его копию — лежать под чужим именем: команда восстановления клала
# файл обратно как `tf.py`. Итог: зелёный отчёт и испорченное дерево, из которого следующий
# `git add -A` увёз бы в историю снесённую защиту. Ровно то, от чего этот скрипт и стережёт.
#
# Отсутствие проверки читается как пройденная проверка — даже в инструменте, который эту болезнь
# и ищет. Особенно в нём.
AFTER=$(git status --porcelain)
if [ "$AFTER" != "$BEFORE" ]; then
	printf '\n  \033[31m✗ ДЕРЕВО НЕ ВОССТАНОВЛЕНО\033[0m — атаки оставили следы:\n'
	diff <(printf '%s\n' "$BEFORE") <(printf '%s\n' "$AFTER") | grep -E '^[<>]' | sed 's/^/      /'
	echo
	echo "  Верни файлы вручную ДО коммита: git add -A утащит снесённую защиту в историю."
	exit 1
fi
echo "  дерево восстановлено полностью — коммитить безопасно"

exit "$holes"
