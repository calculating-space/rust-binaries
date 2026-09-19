#!/bin/sh
# A stand-in for `oneshot --json`: reads the prompt on stdin and answers with
# one record whose result echoes the arguments and the prompt, so a test can
# see exactly what the `ai` module asked for. FAIL in the prompt fails the run.
prompt=$(cat)
esc() { printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g' | tr '\n' ' '; }
[ -n "$FAKE_LOG" ] && printf '%s\n' "$*" >> "$FAKE_LOG"
case "$prompt" in
  *FAIL*)
    printf '{"version":"oneshot/1","ok":false,"error":"agent reported is_error","result":"it went wrong","cost_usd":0.5}\n'
    exit 1 ;;
  *)
    printf '{"version":"oneshot/1","ok":true,"result":"args: %s | prompt: %s","cost_usd":0.01,"num_turns":1}\n' "$(esc "$*")" "$(esc "$prompt")" ;;
esac
