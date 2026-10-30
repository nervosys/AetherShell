#!/bin/bash
# Process start-to-exit time for `ish`, with comparators, interleaved.
#   AE=path/to/ish bash benches/agentic/startup.sh [rounds]
AE=${AE:-ish}
N=${1:-30}
declare -A tot
for r in $(seq "$N"); do
  for k in true sqlite3 ish_version ae_c1 ae_b_echo; do
    s=$(date +%s%N)
    case $k in
      true) true ;;
      sqlite3) sqlite3 :memory: "select 1" >/dev/null ;;
      ish_version) "$AE" --version >/dev/null ;;
      ae_c1) "$AE" -c 1 >/dev/null ;;
      ae_b_echo) "$AE" -b -c 'echo 1' >/dev/null ;;
    esac
    tot[$k]=$(( ${tot[$k]:-0} + $(date +%s%N) - s ))
  done
done
for k in true sqlite3 ish_version ae_c1 ae_b_echo; do
  printf '%-12s %6d us\n' "$k" $(( ${tot[$k]} / N / 1000 ))
done
