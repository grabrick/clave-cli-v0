#!/bin/bash
# Мок claude для смоука headless: auth-проба + stream-json ответ.
case "$*" in
  *"auth status"*) echo "logged in as mock-claude"; exit 0 ;;
esac
printf '%s\n' '{"type":"content_block_delta","delta":{"type":"text_delta","text":"mock claude answer"}}'
printf '%s\n' '{"type":"result","result":"mock claude answer","is_error":false,"usage":{"input_tokens":10,"output_tokens":5},"total_cost_usd":0.001}'
exit 0
