# clave-dev Фаза 2 — e2e vision (запускает пользователь на macOS)

Реальный proof зрения выполняется на твоей машине: нужен GUI Terminal.app и системные
разрешения, которых нет в headless-сессии (см. спеку §1). Юнит-логика (нормализация вердикта,
разрешение окна, fail-safe захвата) уже проверена автотестами; здесь — настоящий прогон.

## 1. Разрешения (one-time)

System Settings → Privacy & Security:
- **Accessibility** — включить для приложения, из которого запускаешь (Terminal.app и/или раннер),
  иначе System Events не сможет слать клавиши.
- **Screen Recording** — включить там же, иначе `screencapture` вернёт пустой/чёрный кадр
  (clave-dev это детектит и выдаёт **блокирующий** вердикт, не «pass»).

## 2. Профиль Terminal.app (детерминизм среды, §4)

Создай профиль `clave-dev` (Terminal → Settings → Profiles): фикс. шрифт (SF Mono 13),
тема без прозрачности (opacity 1.0), размер под `default_profile()` (100×30). Профиль задаётся
флагом `--terminal-profile clave-dev`; среда логируется в stderr в начале прогона.

## 3. Канал зрения (§3) — ПОДКЛЮЧЁН

Image-backend реализован: прямой вызов Anthropic Messages API (stdlib `urllib`, без новых
зависимостей) — см. `vision_claude.py::_send_via_api`. Нужно только:

```bash
export ANTHROPIC_API_KEY="sk-ant-..."         # обязательно для --vision claude
export CLAVE_VISION_MODEL="claude-sonnet-5"    # опц.; дефолт — vision-capable Sonnet 5
```

Без `ANTHROPIC_API_KEY` (и без инъекции `sender`) `available()` вернёт False → clave-dev честно
печатает «зрение выключено» и идёт текст-онли (Фаза 1), не выдавая ложный pass. Промпт уже просит
строго JSON `checklist_results/issues/open_critique`; ответ парсится `extract_verdict_json`+`parse_verdict`
(fail-safe: непарсящийся ответ → блокирующий вердикт).

## 4. Прогон

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cd <repo>/tools/clave-dev
python3 -m clave_dev "почини срез футера у правой стенки" \
  --repo <repo> --known-good <ветковый clave с --run> \
  --build-profile debug --vision claude --terminal-profile clave-dev \
  --severity-threshold medium --max-rounds 3
```

## 5. Ожидаемый результат (критерии proof)

**Как получить контролируемо «битый» кадр** (без правок кода): сузь окно Terminal.app до
минимальной ширины (или уменьши размер/увеличь шрифт в профиле `clave-dev`) — футер и правый
край контента налезут на стенку, ровно тот класс ambiguous-width среза. «Чистый» — нормальная
ширина. Сравнение узкое/нормальное и даёт пару pass=false / pass=true.

- **Битый UI** (заведомо срезанный футер): vision-вердикт `pass=false`, среди `checklist_results`
  провален required-пункт «текст не касается правой границы» и/или есть `high`-issue про правый край;
  супервайзер **не** сходится, фидбэк с описанием уходит агенту.
- **Чистый UI**: `pass=true`, петля сходится (если и проверки, и текстовые assertions зелёные).
- **Нет Screen Recording**: блокирующий вердикт «кадр пустой/чёрный», не ложный pass.
- **Окно не найдено/неоднозначно**: `WindowNotFoundError` с перечислением кандидатов.

Сообщи результат (скриншоты + вердикт) — это и есть настоящая проверка Фазы 2.
