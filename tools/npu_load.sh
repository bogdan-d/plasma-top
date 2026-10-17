#!/usr/bin/env bash
# Generate sustained NPU load for manual validation: serve qwen3:0.6b from the repo-local FastFlowLM in .tmp/fastflowlm/ and send chat requests for DURATION seconds.
# Run it under `tools/npu_fabric_clock.sh with --` so FCLK is held while the NPU works.
# FLM_MODEL_PATH and FLM_DISABLE_UPDATE_CHECK keep FastFlowLM from writing under the user's home.
set -euo pipefail

usage() {
    echo "usage: tools/npu_load.sh [DURATION_SECONDS]" >&2
    exit 2
}

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DURATION="${1:-60}"
[[ $DURATION =~ ^[1-9][0-9]*$ ]] || usage
PORT=52625
FLM_DIR="$ROOT/.tmp/fastflowlm"
LOG="$FLM_DIR/serve.log"

[[ -x $FLM_DIR/flm ]] || {
    echo "FastFlowLM not found at $FLM_DIR/flm; see .scratch/amd-npu-telemetry/spec.md" >&2
    exit 1
}
export FLM_MODEL_PATH="$FLM_DIR/models" FLM_DISABLE_UPDATE_CHECK=1

"$FLM_DIR/flm" serve qwen3:0.6b >"$LOG" 2>&1 &
SERVER=$!
trap 'kill "$SERVER" 2>/dev/null; wait "$SERVER" 2>/dev/null || true' EXIT

ready=0
for _ in $(seq 1 120); do
    if curl -fsS "http://127.0.0.1:$PORT/v1/models" >/dev/null 2>&1; then
        ready=1
        break
    fi
    kill -0 "$SERVER" 2>/dev/null || {
        echo "server exited; see $LOG" >&2
        exit 1
    }
    sleep 0.5
done
((ready)) || {
    echo "server not ready after 60 s; see $LOG" >&2
    exit 1
}
echo "server ready at $(date +%T)"

end=$((SECONDS + DURATION))
n=0
while ((SECONDS < end)); do
    curl -fsS "http://127.0.0.1:$PORT/v1/chat/completions" -H 'Content-Type: application/json' \
        -d '{"model":"qwen3:0.6b","messages":[{"role":"user","content":"Write a detailed 400-word explanation of how a CPU cache works. /no_think"}],"max_tokens":512}' |
        python3 -I -c 'import json,sys; u=json.load(sys.stdin).get("usage",{}); print("request done:", u.get("completion_tokens"), "tokens")'
    n=$((n + 1))
done
echo "requests: $n, finished at $(date +%T)"
