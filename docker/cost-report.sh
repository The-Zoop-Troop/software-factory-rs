#!/usr/bin/env bash
# Host-level cost report: token spend and landing efficiency per rig, from each rig's own
# event log (works for stopped rigs — only the ledger volume is read).
#   docker/cost-report.sh            # table across every registered rig
#   USD_PER_MTOK=3 docker/cost-report.sh   # add a dollar column (blended $/1M tokens)
set -euo pipefail
cd "$(dirname "$0")/.."   # the factory repository: compose.yaml lives here

ROOT=${FACTORY_ROOT:-$HOME/.factory}
RATE=${USD_PER_MTOK:-}
rigs=$(factory rig list | awk 'NF {print $1}')
[ -n "$rigs" ] || { echo "cost-report: no rigs registered"; exit 0; }

printf '%-10s %6s %6s %8s %7s %12s %12s' rig epics tasks attempts landed tokens wasted
[ -n "$RATE" ] && printf ' %10s' usd
printf '\n'

t_tok=0; t_waste=0
for rig in $rigs; do
  json=$(docker compose -p "factory-$rig" --env-file "$ROOT/$rig/compose.env" -f compose.yaml \
    run --rm --no-deps shell sh -c 'cd /work/rig && factory metrics --json' 2>/dev/null \
    | grep -v '^\[rig\]' | sed -n '/^\[/,$p') || json='[]'
  [ -n "$json" ] || json='[]'
  row=$(jq -r '
    [.[] | .tasks[].attempts[]] as $a
    | [ length,
        ([.[] | .tasks | length] | add // 0),
        ($a | length),
        ([$a[] | select(.landed)] | length),
        ([$a[].tokens] | add // 0),
        ([$a[] | select(.landed | not) | .tokens] | add // 0)
      ] | @tsv' <<<"$json" 2>/dev/null) || row=$'0\t0\t0\t0\t0\t0'
  IFS=$'\t' read -r epics tasks attempts landed tokens wasted <<<"$row"
  t_tok=$((t_tok + tokens)); t_waste=$((t_waste + wasted))
  printf '%-10s %6s %6s %8s %7s %12s %12s' "$rig" "$epics" "$tasks" "$attempts" "$landed" "$tokens" "$wasted"
  [ -n "$RATE" ] && printf ' %10s' "$(awk -v t="$tokens" -v r="$RATE" 'BEGIN{printf "%.2f", t/1e6*r}')"
  printf '\n'
done

printf '%-10s %6s %6s %8s %7s %12s %12s' total - - - - "$t_tok" "$t_waste"
[ -n "$RATE" ] && printf ' %10s' "$(awk -v t="$t_tok" -v r="$RATE" 'BEGIN{printf "%.2f", t/1e6*r}')"
printf '\n'
if [ "$t_tok" -gt 0 ]; then
  awk -v w="$t_waste" -v t="$t_tok" 'BEGIN{printf "retry tax: %.1f%% of all tokens went to attempts that did not land\n", w/t*100}'
fi
