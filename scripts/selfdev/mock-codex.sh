#!/bin/bash
# Мок codex для смоука headless: auth-проба + краткий ответ (в файл -o) + usage JSONL.
case "$*" in
  *"login status"*) echo "Logged in as mock-codex"; exit 0 ;;
esac
outfile=""; prev=""
for a in "$@"; do
  [ "$prev" = "-o" ] && outfile="$a"
  prev="$a"
done
[ -n "$outfile" ] && printf 'mock codex answer\n' > "$outfile"
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":10,"output_tokens":5}}'
exit 0
