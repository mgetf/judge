#!/usr/bin/env bash
# Excise whole cases from the live court log: pull, cut, verify, push, restart.
# Usage: ./scripts/cut-cases.sh <case-id> [<case-id>...]
# Example: ./scripts/cut-cases.sh case-require-demos case-should-we-require-demos
#
# Safety: remote backup (/data/events.jsonl.pre-cut-<ts>) + local backup
# (*.bak) are kept. Replay-verified locally before anything is pushed:
# every kept line must parse, and no vote/deliberation/close may reference
# a removed case or outcome. Refuses to push on any failure.
set -euo pipefail
APP="${FLY_APP:-mgetf-judge}"
export PATH="$HOME/.fly/bin:$PATH"

[ $# -ge 1 ] || { echo "usage: $0 <case-id> [<case-id>...]" >&2; exit 2; }
LIVE="events.live.jsonl"
./scripts/pull-log.sh "$LIVE"

uv run python3 - "$LIVE" "$@" << 'PYEOF'
import json, sys, shutil
src, targets = sys.argv[1], tuple(sys.argv[2:])
lines = open(src).read().splitlines()
shutil.copy(src, src + '.bak')

def hits(line):
    try: e = json.loads(line)
    except Exception as ex: print('CORRUPT line:', ex); sys.exit(1)
    return any(t in json.dumps(e) for t in targets)

drop = {i for i, l in enumerate(lines) if hits(l)}
print('dropping lines:', sorted(drop))
# completeness: outcome ids born in dropped lines must not be referenced elsewhere
born = set()
for i in drop:
    e = json.loads(lines[i])
    if e.get('type') == 'outcome_proposed': born.add(e['id'])
for i, l in enumerate(lines):
    if i in drop: continue
    e = json.loads(l)
    if e.get('type') == 'vote_cast' and e.get('outcome') in born:
        print('STRAY vote ref at line', i); sys.exit(1)
kept = [l for i, l in enumerate(lines) if i not in drop]
# replay check: parse + no dangling case refs
cases, outcomes = set(), {}
for l in kept:
    e = json.loads(l)
    t = e.get('type')
    if t == 'case_opened': cases.add(e['id'])
    elif t == 'outcome_proposed': outcomes.setdefault(e['case'], set()).add(e['id'])
for l in kept:
    e = json.loads(l)
    t = e.get('type')
    if t in ('outcome_proposed', 'vote_cast', 'deliberation_opened', 'case_closed'):
        if e.get('case') not in cases:
            print('DANGLING', t, e.get('case')); sys.exit(1)
    if t == 'vote_cast' and e.get('outcome') not in outcomes.get(e['case'], set()):
        print('DANGLING outcome', e.get('outcome')); sys.exit(1)
for t in targets:
    assert not any(t in json.dumps(json.loads(l)) for l in kept), t
open(src, 'w').write('\n'.join(kept) + '\n')
print('kept:', len(kept), '| backup:', src + '.bak')
PYEOF

# push needs the remote path clear (sftp refuses overwrite)
TS=$(date +%Y%m%d-%H%M%S)
printf '%s\n' "cp /data/events.jsonl /data/events.jsonl.pre-cut-$TS" 'rm /data/events.jsonl' 'exit' \
  | flyctl ssh console --app "$APP" > /dev/null
./scripts/push-log.sh "$LIVE"
MID=$(flyctl machine list -a "$APP" --json 2>/dev/null | python3 -c "import json,sys; print(json.load(sys.stdin)[0]['id'])" 2>/dev/null || echo "")
if [ -n "$MID" ]; then
  flyctl machine restart "$MID" -a "$APP" 2>&1 | tail -n 2
else
  echo "pushed; restart the machine when ready: flyctl machine restart -a $APP"
fi
echo "done. remote backup: /data/events.jsonl.pre-cut-$TS  local backup: $LIVE.bak"
