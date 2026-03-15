#!/usr/bin/env bash

set -euo pipefail

MODE=""
OUTPUT_PATH=""
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_PATH="${ROOT_DIR}/target/release-size/cancerbroker"
DAEMON_FIXTURE_PATH="${ROOT_DIR}/fixtures/config/completion-cleanup.toml"
RA_GUARD_FIXTURE_PATH="${ROOT_DIR}/fixtures/config/rust-analyzer-guard-minimal.toml"
DEFAULT_EVIDENCE_DIR="${ROOT_DIR}/.sisyphus/evidence"
EVIDENCE_DIR="${DEFAULT_EVIDENCE_DIR}"

SAMPLE_INTERVAL_SECONDS="0.10"
BASELINE_IDLE_SECONDS=3
BASELINE_PEAK_SECONDS=5
FINAL_PEAK_RUNS=5
GROWTH_RUNS=10
SPREAD_BUDGET_MIB=5

DAEMON_PID=""
TMP_DIR=""
TMP_CONFIG=""
SOCKET_PATH=""
STATE_PATH=""

usage() {
  cat <<'EOF'
Usage:
  scripts/measure_ra_guard_rss.sh --mode <mode> [--output <path>] [--evidence-dir <path>]

Modes:
  baseline-idle    Start daemon and measure steady idle RSS.
  baseline-peak    Start daemon, send one completion event, report peak RSS during fixed window.
  final-idle       Run one-shot ra-guard once and record max RSS via /usr/bin/time -l.
  final-peak       Run one-shot ra-guard repeatedly and report peak max RSS across runs.
  growth-10        Run one-shot ra-guard 10 times (no injected events) and report spread.
  budget-check     Compare baseline daemon and final ra-guard RSS against Task 5 budgets.
  task-5-evidence  Produce baseline/final/growth/budget evidence under --evidence-dir.

Required:
  --output        Required for all modes except task-5-evidence.

Optional:
  --evidence-dir  Defaults to .sisyphus/evidence under repository root.
EOF
}

die() {
  echo "error: $*" >&2
  exit 1
}

require_tools() {
  command -v python3 >/dev/null 2>&1 || die "python3 is required"
  command -v ps >/dev/null 2>&1 || die "ps is required"
  [[ -x "/usr/bin/time" ]] || die "/usr/bin/time is required on macOS"
}

parse_args() {
  while (($# > 0)); do
    case "$1" in
      --mode)
        [[ $# -ge 2 ]] || die "--mode requires a value"
        MODE="$2"
        shift 2
        ;;
      --output)
        [[ $# -ge 2 ]] || die "--output requires a value"
        OUTPUT_PATH="$2"
        shift 2
        ;;
      --evidence-dir)
        [[ $# -ge 2 ]] || die "--evidence-dir requires a value"
        EVIDENCE_DIR="$2"
        shift 2
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      *)
        die "unknown argument: $1"
        ;;
    esac
  done

  [[ -n "$MODE" ]] || die "--mode is required"
  case "$MODE" in
    baseline-idle|baseline-peak|final-idle|final-peak|growth-10|budget-check|task-5-evidence)
      ;;
    *)
      die "unsupported mode: $MODE"
      ;;
  esac

  if [[ "$MODE" != "task-5-evidence" ]]; then
    [[ -n "$OUTPUT_PATH" ]] || die "--output is required for mode: $MODE"
  fi
}

prepare_output_path() {
  local output_dir
  output_dir="$(dirname "$OUTPUT_PATH")"
  mkdir -p "$output_dir"
}

prepare_evidence_dir() {
  mkdir -p "$EVIDENCE_DIR"
}

create_temp_config() {
  TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/measure-ra-guard-rss.XXXXXX")"
  TMP_CONFIG="${TMP_DIR}/completion-cleanup.measure.toml"
  SOCKET_PATH="${TMP_DIR}/completion.sock"
  STATE_PATH="${TMP_DIR}/completion-state.json"

  cat >"$TMP_CONFIG" <<EOF
mode = "observe"

[completion]
enabled_sources = ["status", "idle", "tool_part_completed", "error", "deleted", "inferred"]
dedupe_ttl_secs = 600
cleanup_retry_interval_secs = 15
reconciliation_interval_secs = 60
daemon_socket_path = "${SOCKET_PATH}"
state_path = "${STATE_PATH}"
EOF
}

cleanup() {
  if [[ -n "${DAEMON_PID}" ]] && kill -0 "${DAEMON_PID}" >/dev/null 2>&1; then
    kill "${DAEMON_PID}" >/dev/null 2>&1 || true
    wait "${DAEMON_PID}" 2>/dev/null || true
  fi
  if [[ -n "${TMP_DIR}" && -d "${TMP_DIR}" ]]; then
    rm -rf "${TMP_DIR}"
  fi
}

wait_for_socket() {
  local attempts=0
  while [[ ! -S "$SOCKET_PATH" ]]; do
    attempts=$((attempts + 1))
    if ((attempts > 100)); then
      die "daemon socket was not created: $SOCKET_PATH"
    fi
    sleep 0.05
  done
}

start_daemon() {
  [[ -x "$BIN_PATH" ]] || die "missing binary: $BIN_PATH (run cargo build --profile release-size)"
  [[ -f "$DAEMON_FIXTURE_PATH" ]] || die "missing fixture: $DAEMON_FIXTURE_PATH"

  "$BIN_PATH" --config "$TMP_CONFIG" daemon --json --max-events 1 >/dev/null 2>&1 &
  DAEMON_PID=$!
  wait_for_socket
}

ensure_ra_guard_fixture() {
  [[ -f "$RA_GUARD_FIXTURE_PATH" ]] || die "missing fixture: $RA_GUARD_FIXTURE_PATH"
}

read_rss_kb() {
  local pid="$1"
  local rss
  rss="$(ps -o rss= -p "$pid" | tr -d '[:space:]')"
  if [[ -z "$rss" ]]; then
    echo 0
  else
    echo "$rss"
  fi
}

sample_rss_window() {
  local duration_seconds="$1"
  local elapsed="0"
  local samples=0
  local min_rss=0
  local max_rss=0
  local last_rss=0

  while python3 - <<PY
elapsed = float("$elapsed")
duration = float("$duration_seconds")
raise SystemExit(0 if elapsed < duration else 1)
PY
  do
    local current
    current="$(read_rss_kb "$DAEMON_PID")"
    if ((samples == 0)); then
      min_rss="$current"
      max_rss="$current"
    else
      if ((current < min_rss)); then
        min_rss="$current"
      fi
      if ((current > max_rss)); then
        max_rss="$current"
      fi
    fi
    last_rss="$current"
    samples=$((samples + 1))
    sleep "$SAMPLE_INTERVAL_SECONDS"
    elapsed="$(python3 - <<PY
print(f"{float('$elapsed') + float('$SAMPLE_INTERVAL_SECONDS'):.2f}")
PY
)"
  done

  printf '%s %s %s %s\n' "$samples" "$min_rss" "$max_rss" "$last_rss"
}

send_completion_event() {
  local event_id="$1"
  local completed_at="$2"
  python3 - "$SOCKET_PATH" "$event_id" "$completed_at" <<'PY'
import socket
import sys

socket_path = sys.argv[1]
event_id = sys.argv[2]
completed_at = sys.argv[3]
payload = (
    '{"type":"session.status","event_id":"%s","session_id":"rss-harness",'
    '"status":"idle","completed_at":"%s"}\n'
) % (event_id, completed_at)

with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
    client.connect(socket_path)
    client.sendall(payload.encode("utf-8"))
PY
}

measure_ra_guard_rss_kb() {
  local time_output
  time_output="$(mktemp "${TMPDIR:-/tmp}/measure-ra-guard-time.XXXXXX")"

  /usr/bin/time -l "$BIN_PATH" --config "$RA_GUARD_FIXTURE_PATH" ra-guard --json >/dev/null 2>"$time_output"

  python3 - "$time_output" <<'PY'
import re
import sys

path = sys.argv[1]
with open(path, "r", encoding="utf-8") as handle:
    data = handle.read()
match = re.search(r"(\d+)\s+maximum resident set size", data)
if not match:
    raise SystemExit("failed to parse max RSS from /usr/bin/time -l output")
# macOS /usr/bin/time -l reports maximum resident set size in bytes.
# Normalize to KiB to match ps -o rss output used for daemon baseline modes.
print(int(match.group(1)) // 1024)
PY

  rm -f "$time_output"
}

read_kv_value() {
  local file_path="$1"
  local key="$2"
  python3 - "$file_path" "$key" <<'PY'
import sys

path = sys.argv[1]
needle = sys.argv[2]
value = None
with open(path, "r", encoding="utf-8") as handle:
    for raw in handle:
        line = raw.rstrip("\n")
        if not line or "=" not in line:
            continue
        lhs, rhs = line.split("=", 1)
        if lhs == needle:
            value = rhs
            break
if value is None:
    raise SystemExit(f"missing key {needle} in {path}")
print(value)
PY
}

write_report() {
  local body="$1"
  local tmp_output
  tmp_output="${OUTPUT_PATH}.tmp"
  {
    printf 'mode=%s\n' "$MODE"
    printf 'platform=%s\n' "$(uname -s)"
    printf 'binary=%s\n' "$BIN_PATH"
    printf 'daemon_fixture=%s\n' "$DAEMON_FIXTURE_PATH"
    printf 'ra_guard_fixture=%s\n' "$RA_GUARD_FIXTURE_PATH"
    printf 'sample_interval_seconds=%s\n' "$SAMPLE_INTERVAL_SECONDS"
    printf '%s\n' "$body"
  } >"$tmp_output"
  mv "$tmp_output" "$OUTPUT_PATH"
}

run_baseline_idle() {
  local samples min_rss max_rss last_rss body
  sleep 1
  read -r samples min_rss max_rss last_rss < <(sample_rss_window "$BASELINE_IDLE_SECONDS")
  body=$(cat <<EOF
samples=${samples}
rss_idle_min_kb=${min_rss}
rss_idle_peak_kb=${max_rss}
rss_idle_last_kb=${last_rss}
notes=No completion events injected; captures daemon idle baseline.
EOF
)
  write_report "$body"
}

run_baseline_peak() {
  local samples min_rss max_rss last_rss body
  send_completion_event "baseline-peak-1" "2026-01-01T00:00:00Z"
  read -r samples min_rss max_rss last_rss < <(sample_rss_window "$BASELINE_PEAK_SECONDS")
  body=$(cat <<EOF
events_sent=1
samples=${samples}
rss_window_min_kb=${min_rss}
rss_window_peak_kb=${max_rss}
rss_window_last_kb=${last_rss}
notes=Single completion event injected; reports fixed-window peak RSS.
EOF
)
  write_report "$body"
}

run_growth_10() {
  local rss_values=()
  local i
  local rss_kb
  local min_rss=0
  local max_rss=0
  local spread_kb
  local spread_limit_kb=$((SPREAD_BUDGET_MIB * 1024))
  local body

  ensure_ra_guard_fixture
  for ((i = 1; i <= GROWTH_RUNS; i++)); do
    rss_kb="$(measure_ra_guard_rss_kb)"
    rss_values+=("$rss_kb")
    if ((i == 1)); then
      min_rss="$rss_kb"
      max_rss="$rss_kb"
    else
      if ((rss_kb < min_rss)); then
        min_rss="$rss_kb"
      fi
      if ((rss_kb > max_rss)); then
        max_rss="$rss_kb"
      fi
    fi
  done

  spread_kb=$((max_rss - min_rss))

  body=$(cat <<EOF
runs=${GROWTH_RUNS}
rss_run_values_kb=${rss_values[*]}
rss_no_event_min_kb=${min_rss}
rss_no_event_peak_kb=${max_rss}
rss_no_event_spread_kb=${spread_kb}
rss_no_event_spread_mib=$((spread_kb / 1024))
spread_budget_mib=${SPREAD_BUDGET_MIB}
spread_budget_pass=$([[ ${spread_kb} -le ${spread_limit_kb} ]] && echo true || echo false)
notes=Ten no-event ra-guard runs; verifies bounded RSS spread for one-shot path.
EOF
)
  write_report "$body"
}

run_final_idle() {
  local rss_kb body

  ensure_ra_guard_fixture
  rss_kb="$(measure_ra_guard_rss_kb)"

  body=$(cat <<EOF
runs=1
rss_no_event_kb=${rss_kb}
rss_no_event_mib=$((rss_kb / 1024))
command=target/release-size/cancerbroker --config fixtures/config/rust-analyzer-guard-minimal.toml ra-guard --json
notes=One-shot ra-guard no-event run measured with /usr/bin/time -l maximum resident set size.
EOF
)
  write_report "$body"
}

run_final_peak() {
  local rss_values=()
  local i
  local rss_kb
  local min_rss=0
  local max_rss=0
  local body

  ensure_ra_guard_fixture
  for ((i = 1; i <= FINAL_PEAK_RUNS; i++)); do
    rss_kb="$(measure_ra_guard_rss_kb)"
    rss_values+=("$rss_kb")
    if ((i == 1)); then
      min_rss="$rss_kb"
      max_rss="$rss_kb"
    else
      if ((rss_kb < min_rss)); then
        min_rss="$rss_kb"
      fi
      if ((rss_kb > max_rss)); then
        max_rss="$rss_kb"
      fi
    fi
  done

  body=$(cat <<EOF
runs=${FINAL_PEAK_RUNS}
rss_run_values_kb=${rss_values[*]}
rss_window_min_kb=${min_rss}
rss_window_peak_kb=${max_rss}
rss_window_peak_mib=$((max_rss / 1024))
command=target/release-size/cancerbroker --config fixtures/config/rust-analyzer-guard-minimal.toml ra-guard --json
notes=Repeated one-shot ra-guard no-event runs; reports observed max RSS peak.
EOF
)
  write_report "$body"
}

run_budget_check() {
  local baseline_idle_path="${EVIDENCE_DIR}/task-5-baseline-idle-rss.txt"
  local baseline_peak_path="${EVIDENCE_DIR}/task-5-baseline-peak-rss.txt"
  local final_idle_path="${EVIDENCE_DIR}/task-5-final-idle-rss.txt"
  local final_peak_path="${EVIDENCE_DIR}/task-5-final-peak-rss.txt"
  local growth_path="${EVIDENCE_DIR}/task-5-growth-10-rss.txt"

  [[ -f "$baseline_idle_path" ]] || die "missing baseline idle evidence: $baseline_idle_path"
  [[ -f "$baseline_peak_path" ]] || die "missing baseline peak evidence: $baseline_peak_path"
  [[ -f "$final_idle_path" ]] || die "missing final idle evidence: $final_idle_path"
  [[ -f "$final_peak_path" ]] || die "missing final peak evidence: $final_peak_path"
  [[ -f "$growth_path" ]] || die "missing growth evidence: $growth_path"

  local baseline_idle_kb
  local baseline_peak_kb
  local final_idle_kb
  local final_peak_kb
  local spread_kb

  baseline_idle_kb="$(read_kv_value "$baseline_idle_path" "rss_idle_peak_kb")"
  baseline_peak_kb="$(read_kv_value "$baseline_peak_path" "rss_window_peak_kb")"
  final_idle_kb="$(read_kv_value "$final_idle_path" "rss_no_event_kb")"
  final_peak_kb="$(read_kv_value "$final_peak_path" "rss_window_peak_kb")"
  spread_kb="$(read_kv_value "$growth_path" "rss_no_event_spread_kb")"

  local baseline_idle_mib=$((baseline_idle_kb / 1024))
  local baseline_peak_mib=$((baseline_peak_kb / 1024))
  local final_idle_mib=$((final_idle_kb / 1024))
  local final_peak_mib=$((final_peak_kb / 1024))
  local spread_mib=$((spread_kb / 1024))

  local idle_budget_mib=$((baseline_idle_mib / 2))
  if ((idle_budget_mib > 40)); then
    idle_budget_mib=40
  fi

  local peak_budget_mib=$(((baseline_peak_mib * 65) / 100))
  if ((peak_budget_mib > 64)); then
    peak_budget_mib=64
  fi

  local spread_limit_kb=$((SPREAD_BUDGET_MIB * 1024))

  local idle_pass="false"
  local peak_pass="false"
  local spread_pass="false"
  local overall_pass="false"

  if ((final_idle_mib <= idle_budget_mib)); then
    idle_pass="true"
  fi
  if ((final_peak_mib <= peak_budget_mib)); then
    peak_pass="true"
  fi
  if ((spread_kb <= spread_limit_kb)); then
    spread_pass="true"
  fi
  if [[ "$idle_pass" == "true" && "$peak_pass" == "true" && "$spread_pass" == "true" ]]; then
    overall_pass="true"
  fi

  local body
  body=$(cat <<EOF
baseline_idle_file=${baseline_idle_path}
baseline_peak_file=${baseline_peak_path}
final_idle_file=${final_idle_path}
final_peak_file=${final_peak_path}
growth_file=${growth_path}
baseline_idle_rss_mib=${baseline_idle_mib}
baseline_peak_rss_mib=${baseline_peak_mib}
final_idle_rss_mib=${final_idle_mib}
final_peak_rss_mib=${final_peak_mib}
repeated_idle_spread_mib=${spread_mib}
idle_budget_formula=min(40 MiB, floor(baseline_idle_rss_mib * 0.5))
peak_budget_formula=min(64 MiB, floor(baseline_peak_rss_mib * 0.65))
spread_budget_formula=repeated_idle_spread_mib <= 5
idle_budget_mib=${idle_budget_mib}
peak_budget_mib=${peak_budget_mib}
spread_budget_mib=${SPREAD_BUDGET_MIB}
idle_budget_pass=${idle_pass}
peak_budget_pass=${peak_pass}
spread_budget_pass=${spread_pass}
overall_pass=${overall_pass}
notes=Compares daemon baseline against one-shot ra-guard final RSS and repeated no-event spread budgets.
EOF
)
  write_report "$body"
}

run_task_5_evidence() {
  local previous_mode="$MODE"
  local previous_output="$OUTPUT_PATH"

  prepare_evidence_dir

  MODE="baseline-idle"
  OUTPUT_PATH="${EVIDENCE_DIR}/task-5-baseline-idle-rss.txt"
  prepare_output_path
  create_temp_config
  trap cleanup EXIT INT TERM
  start_daemon
  run_baseline_idle
  cleanup
  trap - EXIT INT TERM

  MODE="baseline-peak"
  OUTPUT_PATH="${EVIDENCE_DIR}/task-5-baseline-peak-rss.txt"
  prepare_output_path
  create_temp_config
  trap cleanup EXIT INT TERM
  start_daemon
  run_baseline_peak
  cleanup
  trap - EXIT INT TERM

  MODE="final-idle"
  OUTPUT_PATH="${EVIDENCE_DIR}/task-5-final-idle-rss.txt"
  prepare_output_path
  run_final_idle

  MODE="final-peak"
  OUTPUT_PATH="${EVIDENCE_DIR}/task-5-final-peak-rss.txt"
  prepare_output_path
  run_final_peak

  MODE="growth-10"
  OUTPUT_PATH="${EVIDENCE_DIR}/task-5-growth-10-rss.txt"
  prepare_output_path
  run_growth_10

  MODE="budget-check"
  OUTPUT_PATH="${EVIDENCE_DIR}/task-5-budget-check.txt"
  prepare_output_path
  run_budget_check

  if [[ -n "$previous_output" ]]; then
    MODE="task-5-evidence"
    OUTPUT_PATH="$previous_output"
    prepare_output_path
    write_report "summary_files=${EVIDENCE_DIR}/task-5-baseline-idle-rss.txt ${EVIDENCE_DIR}/task-5-baseline-peak-rss.txt ${EVIDENCE_DIR}/task-5-final-idle-rss.txt ${EVIDENCE_DIR}/task-5-final-peak-rss.txt ${EVIDENCE_DIR}/task-5-growth-10-rss.txt ${EVIDENCE_DIR}/task-5-budget-check.txt"
  fi

  MODE="$previous_mode"
  OUTPUT_PATH="$previous_output"
}

main() {
  parse_args "$@"
  require_tools
  [[ -x "$BIN_PATH" ]] || die "missing binary: $BIN_PATH (run cargo build --profile release-size)"
  prepare_evidence_dir
  if [[ -n "$OUTPUT_PATH" ]]; then
    prepare_output_path
  fi

  case "$MODE" in
    baseline-idle)
      create_temp_config
      trap cleanup EXIT INT TERM
      start_daemon
      run_baseline_idle
      ;;
    baseline-peak)
      create_temp_config
      trap cleanup EXIT INT TERM
      start_daemon
      run_baseline_peak
      ;;
    final-idle)
      run_final_idle
      ;;
    final-peak)
      run_final_peak
      ;;
    growth-10)
      run_growth_10
      ;;
    budget-check)
      run_budget_check
      ;;
    task-5-evidence)
      run_task_5_evidence
      ;;
  esac
}

main "$@"
