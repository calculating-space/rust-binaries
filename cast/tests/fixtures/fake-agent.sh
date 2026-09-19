#!/bin/sh
# A stand-in for `claude -p --output-format json`: reads the prompt on stdin
# and answers with one JSON object. Keywords in the prompt pick the behaviour.
prompt=$(cat)
case "$prompt" in
  *FAIL*)
    printf '{"type":"result","is_error":true,"result":"it went wrong","total_cost_usd":0.5}\n'
    exit 1 ;;
  *SLEEP*)
    sleep 1
    printf '{"result":"slept","is_error":false,"total_cost_usd":0.01}\n' ;;
  *CWD*)
    printf '{"result":"%s","is_error":false}\n' "$(pwd -P)" ;;
  *)
    last=$(printf '%s' "$prompt" | tail -n 1)
    printf '{"type":"result","subtype":"success","is_error":false,"result":"echo: %s","session_id":"s-1","num_turns":1,"total_cost_usd":0.01}\n' "$last" ;;
esac
