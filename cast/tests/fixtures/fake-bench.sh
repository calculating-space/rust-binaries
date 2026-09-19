#!/bin/sh
# A stand-in for a benchmark: the loss starts at 3 and drops by one on every
# call, remembered in the state file given as $1. With METRIC as $2 it prints
# a human report and a METRIC line instead of JSON.
state=$1
n=$(cat "$state" 2>/dev/null || echo 3)
echo $((n - 1)) > "$state"
if [ "$2" = "METRIC" ]; then
  printf 'some human report\nMETRIC loss=%s\n' "$n"
else
  printf '{"loss": %s, "diagnostics": ["case %s failing"]}\n' "$n" "$n"
fi
