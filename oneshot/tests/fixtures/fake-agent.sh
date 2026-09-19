#!/bin/sh
# A stand-in for `claude -p --output-format json`: reads the prompt on stdin
# and answers with one JSON object. Keywords in the prompt pick the behaviour.
prompt=$(cat)
case "$prompt" in
  *SOFTFAIL*)
    printf '{"type":"result","is_error":true,"result":"reported but exit 0"}\n' ;;
  *FAIL*)
    printf '{"type":"result","subtype":"error_during_execution","is_error":true,"result":"it went wrong","session_id":"s-fail","num_turns":2,"total_cost_usd":0.5}\n'
    exit 1 ;;
  *SLOW*)
    sleep 30; printf '{"result":"too late"}\n' ;;
  *GARBAGE*)
    printf 'this is not json\n' ;;
  *NOISY*)
    printf 'warning: something on stderr\n' >&2
    printf '{"result":"fine","is_error":false}\n' ;;
  *CWD*)
    printf '{"result":"%s","is_error":false}\n' "$(pwd -P)" ;;
  *)
    printf '{"type":"result","subtype":"success","is_error":false,"result":"echo: %s","session_id":"s-1","num_turns":1,"total_cost_usd":0.01,"duration_ms":5,"usage":{"input_tokens":9,"output_tokens":3}}\n' "$prompt" ;;
esac
