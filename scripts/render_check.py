#!/usr/bin/env python3
"""Headless-проверка живого рендера clave через эмулятор терминала pyte.

Ручной рендер (src/render.rs) НЕ делает CPR-запрос курсора, поэтому его можно
прогнать под обычным псевдо-TTY и «увидеть» экран. Скрипт запускает настоящий
бинарник, дёргает меню/подсказки и проверяет ключевое поведение «как в Claude
Code»: панель открывается над вводом, история в скроллбэке, без накопления.

Запуск:
    python3 scripts/render_check.py target/release/clave <CLAVE_HOME>
(нужен pyte: `pip3 install pyte`). CLAVE_HOME — каталог с config + chats/.
"""
import fcntl
import os
import pty
import select
import struct
import subprocess
import sys
import termios
import time

import pyte

COLS, ROWS = 100, 30


def main() -> int:
    binary, home = sys.argv[1], sys.argv[2]
    master, slave = pty.openpty()
    fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))
    env = dict(os.environ)
    env.update(CLAVE_HOME=home, CLAVE_SKIP_ONBOARDING="1", TERM="xterm-256color")
    proc = subprocess.Popen(
        [binary], stdin=slave, stdout=slave, stderr=slave, env=env, close_fds=True
    )
    os.close(slave)
    screen = pyte.Screen(COLS, ROWS)
    stream = pyte.ByteStream(screen)

    def pump(seconds=0.4):
        end = time.time() + seconds
        while time.time() < end:
            ready, _, _ = select.select([master], [], [], 0.05)
            if ready:
                try:
                    data = os.read(master, 65536)
                except OSError:
                    return
                if not data:
                    return
                stream.feed(data)

    def send(text):
        os.write(master, text.encode())

    def snap():
        return tuple(row.rstrip() for row in screen.display)

    def visible(needle):
        return any(needle in row for row in screen.display)

    # Футер уникален: на ОДНОЙ строке и «Shift+Tab», и «команды»/«commands». Панель
    # подсказок тоже перечисляет «Shift+Tab», поэтому одного слова недостаточно.
    def footer_shown():
        return any(
            "Shift+Tab" in r and ("команды" in r or "commands" in r)
            for r in screen.display
        )

    # Над блоком (верхней линейкой композера) должна быть пустая строка — «воздух»
    # между историей и блоком. Под инпутом отступа нет: футер сразу за композером.
    def gap_above_block():
        disp = [r.rstrip() for r in screen.display]
        rule = next((i for i, r in enumerate(disp) if r.count("─") > 50), None)
        return rule is not None and rule >= 1 and disp[rule - 1] == ""

    pump(1.2)
    footer_idle = footer_shown()
    gap_idle = gap_above_block()
    opened = []
    footer_hidden_on_palette = []
    idles = []
    for _ in range(5):
        send("/")
        pump(0.35)
        opened.append(visible("/brainstorm") or visible("/help") or visible("/btw"))
        footer_hidden_on_palette.append(not footer_shown())  # футер прячется под палитрой
        send("\x7f")  # стереть '/'
        pump(0.35)
        idles.append(snap())
    send("?")
    pump(0.35)
    shortcuts_open = visible("Отправка") or visible("Compose")
    footer_hidden_on_shortcuts = not footer_shown()  # и под подсказками тоже
    send("\x1b")  # Esc
    pump(0.35)
    shortcuts_closed = not (visible("Отправка") or visible("Compose"))
    footer_back = footer_shown()
    send("/quit\r")
    pump(0.8)

    try:
        os.close(master)
    except OSError:
        pass
    try:
        code = proc.wait(timeout=2)
    except Exception:
        proc.kill()
        code = -1

    # Идл-экраны после каждого закрытия должны совпадать — это и есть «нет
    # накопления/дрейфа». Строку футера исключаем: её правый сегмент
    # (roles/mode/chat/theme/effort) вращается по времени и к стабильности ленты
    # отношения не имеет.
    def idle_key(state):
        return tuple(r for r in state if "Shift+Tab" not in r)

    stable = all(idle_key(state) == idle_key(idles[0]) for state in idles)
    checks = {
        "палитра открывается (5/5)": all(opened),
        "подсказки '?' открываются": shortcuts_open,
        "Esc закрывает подсказки": shortcuts_closed,
        "idle стабилен (нет накопления)": stable,
        "футер виден в простое": footer_idle,
        "воздух над блоком (под текстом)": gap_idle,
        "футер прячется под палитрой (5/5)": all(footer_hidden_on_palette),
        "футер прячется под подсказками": footer_hidden_on_shortcuts,
        "футер возвращается после Esc": footer_back,
        "чистый выход /quit": code == 0,
    }
    for name, ok in checks.items():
        print(("  OK   " if ok else "  FAIL ") + name)
    ok = all(checks.values())
    print("\nИТОГ:", "ВСЁ ОК" if ok else "ЕСТЬ ПРОБЛЕМЫ")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
