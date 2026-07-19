#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
artifact_root="$repo_root/.test-artifacts"
baseline_dir="$artifact_root/baseline-validation"
live_dir="$artifact_root/live-host-evidence"
qt_dir="$live_dir/qt-shots"
summary_file="$artifact_root/summary.txt"
tmp_panel_html="/tmp/pirostats_render_panel.html"
tmp_tooltip_html="/tmp/pirostats_render_tooltip.html"
deep_dive_pages=(processes cpu_cores connections fastfetch graphs)

mkdir -p "$artifact_root"
rm -rf "$baseline_dir" "$live_dir"
mkdir -p "$baseline_dir" "$live_dir" "$qt_dir"

cd "$repo_root"

pick_required_python() {
	if [[ -x "$repo_root/.venv/bin/python" ]]; then
		printf '%s\n' "$repo_root/.venv/bin/python"
		return 0
	fi
	if command -v python3 >/dev/null 2>&1; then
		command -v python3
		return 0
	fi
	echo "[error] python3 not found in .venv/bin/python or PATH" >&2
	return 1
}

pick_optional_tool() {
	local preferred_rel="$1"
	local fallback_name="$2"

	if [[ -x "$repo_root/$preferred_rel" ]]; then
		printf '%s\n' "$repo_root/$preferred_rel"
		return 0
	fi
	if command -v "$fallback_name" >/dev/null 2>&1; then
		command -v "$fallback_name"
		return 0
	fi
	return 1
}

python_bin="$(pick_required_python)"
ruff_bin=""
vulture_bin=""

if ruff_bin="$(pick_optional_tool ".venv/bin/ruff" "ruff" 2>/dev/null)"; then
	:
fi
if vulture_bin="$(pick_optional_tool ".venv/bin/vulture" "vulture" 2>/dev/null)"; then
	:
fi

failures=0

relpath() {
	local path="$1"
	printf '%s\n' "${path#$repo_root/}"
}

note() {
	printf '%s\n' "$*" >>"$summary_file"
}

record_success() {
	local label="$1"
	local output_path="$2"
	note "PASS | $label | $(relpath "$output_path")"
}

record_failure() {
	local label="$1"
	local exit_code="$2"
	local output_path="$3"
	note "FAIL($exit_code) | $label | $(relpath "$output_path")"
	failures=$((failures + 1))
}

record_optional_failure() {
	local label="$1"
	local exit_code="$2"
	local output_path="$3"
	note "SOFT_FAIL($exit_code) | $label | $(relpath "$output_path")"
}

write_skip() {
	local output_path="$1"
	local reason="$2"
	printf '%s\n' "$reason" >"$output_path"
	note "SKIP | $(relpath "$output_path") | $reason"
}

run_capture() {
	local label="$1"
	local output_path="$2"
	shift 2

	{
		printf '# %s\n' "$label"
		printf '$'
		printf ' %q' "$@"
		printf '\n\n'
	} >"$output_path"

	if "$@" >>"$output_path" 2>&1; then
		printf '\n[exit] 0\n' >>"$output_path"
		return 0
	fi

	local exit_code=$?
	printf '\n[exit] %s\n' "$exit_code" >>"$output_path"
	return "$exit_code"
}

record_command() {
	local label="$1"
	local output_path="$2"
	shift 2

	if run_capture "$label" "$output_path" "$@"; then
		record_success "$label" "$output_path"
		return 0
	fi

	local exit_code=$?
	record_failure "$label" "$exit_code" "$output_path"
	return 0
}

capture_host_metadata() {
	cat <<EOF
capture_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)
repo_root=$repo_root
artifact_root=$artifact_root
git_head=$(git rev-parse HEAD)
git_branch=$(git branch --show-current)
python_bin=$python_bin
ruff_bin=${ruff_bin:-missing}
vulture_bin=${vulture_bin:-missing}

## git status --short
EOF
	git status --short
	echo
	echo '## uname -a'
	uname -a
	echo
	echo '## /etc/os-release'
	if [[ -f /etc/os-release ]]; then
		cat /etc/os-release
	else
		echo 'missing: /etc/os-release'
	fi
	echo
	echo '## desktop environment variables'
	printf 'XDG_RUNTIME_DIR=%s\n' "${XDG_RUNTIME_DIR-}"
	printf 'DISPLAY=%s\n' "${DISPLAY-}"
	printf 'WAYLAND_DISPLAY=%s\n' "${WAYLAND_DISPLAY-}"
	printf 'DESKTOP_SESSION=%s\n' "${DESKTOP_SESSION-}"
	printf 'XDG_CURRENT_DESKTOP=%s\n' "${XDG_CURRENT_DESKTOP-}"
	echo
	echo '## config/style/lang checksums'
	sha256sum \
		config/config.toml \
		config/machines.toml \
		style/icons.toml \
		style/style-dark.css \
		style/style-light.css \
		style/style-overlay.css \
		lang/en.toml
	echo
	echo '## tool paths'
	for tool_name in git bash python3 plasmashell kpackagetool6 fastfetch ip iw ss lspci nvidia-smi; do
		if command -v "$tool_name" >/dev/null 2>&1; then
			printf '%s=%s\n' "$tool_name" "$(command -v "$tool_name")"
		else
			printf '%s=missing\n' "$tool_name"
		fi
	done
	echo
	echo '## DRM GPU inventory'
	local drm_path driver_path
	for drm_path in /sys/class/drm/card[0-9]; do
		[[ -e "$drm_path" ]] || continue
		driver_path="$(readlink -f "$drm_path/device/driver" 2>/dev/null || true)"
		printf '%s vendor=%s device=%s driver=%s\n' \
			"$(basename "$drm_path")" \
			"$(cat "$drm_path/device/vendor" 2>/dev/null || printf missing)" \
			"$(cat "$drm_path/device/device" 2>/dev/null || printf missing)" \
			"${driver_path##*/}"
	done
	if command -v lspci >/dev/null 2>&1; then
		echo
		echo '## PCI display inventory'
		lspci -nnk 2>&1 | grep -A3 -Ei 'VGA|Display|3D' || true
	fi
	echo
	echo '## power supply inventory'
	local power_path
	for power_path in /sys/class/power_supply/*; do
		[[ -e "$power_path" ]] || continue
		printf '%s type=%s\n' \
			"$(basename "$power_path")" \
			"$(cat "$power_path/type" 2>/dev/null || printf missing)"
	done
	echo
	echo '## generic hidraw node count'
	find /sys/class/hidraw -mindepth 1 -maxdepth 1 -printf . 2>/dev/null | wc -c
	echo
	echo '## python environment'
	"$python_bin" - <<'PY'
import importlib.metadata as metadata
import importlib.util
import platform
import sys

print(f"sys.executable={sys.executable}")
print(f"sys.version={sys.version.splitlines()[0]}")
print(f"sys.implementation={sys.implementation.name}")
print(f"platform={platform.platform()}")

for module_name in ["psutil", "pytest", "ruff", "vulture", "PyQt6", "gi", "pynvml"]:
    if importlib.util.find_spec(module_name) is None:
        print(f"{module_name}=missing")
        continue
    try:
        version = metadata.version(module_name)
    except metadata.PackageNotFoundError:
        version = "importable (metadata version unavailable)"
    print(f"{module_name}={version}")
PY
	echo
	echo '## version commands'
	bash --version | head -n 1
	git --version
	"$python_bin" --version
	if [[ -n "$ruff_bin" ]]; then
		"$ruff_bin" --version
	else
		echo 'ruff missing'
	fi
	if [[ -n "$vulture_bin" ]]; then
		"$vulture_bin" --version
	else
		echo 'vulture missing'
	fi
	if command -v plasmashell >/dev/null 2>&1; then
		plasmashell --version
	else
		echo 'plasmashell missing'
	fi
}

capture_metadata_artifact() {
	local output_path="$1"
	if capture_host_metadata >"$output_path" 2>&1; then
		record_success 'host/runtime metadata' "$output_path"
		return 0
	fi
	local exit_code=$?
	record_failure 'host/runtime metadata' "$exit_code" "$output_path"
	return 0
}

capture_render_html() {
	local label="$1"
	local log_path="$2"
	local tmp_path="$3"
	local artifact_html="$4"
	shift 4

	rm -f "$tmp_path"
	if run_capture "$label" "$log_path" "$@"; then
		record_success "$label" "$log_path"
		if [[ -f "$tmp_path" ]]; then
			cp "$tmp_path" "$artifact_html"
			record_success "$label html" "$artifact_html"
		else
			printf 'Expected render artifact missing: %s\n' "$tmp_path" >"${artifact_html}.missing.txt"
			record_failure "$label html" 'missing' "${artifact_html}.missing.txt"
		fi
		return 0
	fi

	local exit_code=$?
	record_failure "$label" "$exit_code" "$log_path"
	return 0
}

capture_optional_render_html() {
	local label="$1"
	local log_path="$2"
	local tmp_path="$3"
	local artifact_html="$4"
	shift 4

	rm -f "$tmp_path"
	if run_capture "$label" "$log_path" "$@"; then
		record_success "$label" "$log_path"
		if [[ -f "$tmp_path" ]]; then
			cp "$tmp_path" "$artifact_html"
			record_success "$label html" "$artifact_html"
		else
			printf 'Expected render artifact missing: %s\n' "$tmp_path" >"${artifact_html}.missing.txt"
			record_optional_failure "$label html" 'missing' "${artifact_html}.missing.txt"
		fi
		return 0
	fi

	local exit_code=$?
	record_optional_failure "$label" "$exit_code" "$log_path"
	return 0
}

capture_qt_html() {
	local label="$1"
	local input_html="$2"
	local output_png="$3"
	local log_path="$4"
	shift 4

	if [[ ! -f "$input_html" ]]; then
		write_skip "$log_path" "Skipped: source HTML missing for $label ($input_html)."
		return 0
	fi

	if run_capture "$label" "$log_path" env QT_QPA_PLATFORM=offscreen "$python_bin" tools/qt_shot.py --html "$input_html" "$output_png" "$@"; then
		record_success "$label" "$log_path"
		record_success "$label png" "$output_png"
		return 0
	fi

	local exit_code=$?
	record_failure "$label" "$exit_code" "$log_path"
	return 0
}

capture_optional_qt_html() {
	local label="$1"
	local input_html="$2"
	local output_png="$3"
	local log_path="$4"
	shift 4

	if [[ ! -f "$input_html" ]]; then
		write_skip "$log_path" "Skipped: source HTML missing for $label ($input_html)."
		return 0
	fi

	if run_capture "$label" "$log_path" env QT_QPA_PLATFORM=offscreen "$python_bin" tools/qt_shot.py --html "$input_html" "$output_png" "$@"; then
		record_success "$label" "$log_path"
		record_success "$label png" "$output_png"
		return 0
	fi

	local exit_code=$?
	record_optional_failure "$label" "$exit_code" "$log_path"
	return 0
}

capture_deep_dive_page_renders() {
	local page_name
	for page_name in "${deep_dive_pages[@]}"; do
		capture_optional_render_html "render tooltip page ${page_name} html" \
			"$live_dir/render-page-${page_name}.log" \
			"$tmp_tooltip_html" "$live_dir/page-${page_name}.html" \
			"$python_bin" ./pirostats render --page "$page_name" --format html
	done
}

capture_deep_dive_page_qt_shots() {
	local page_name
	for page_name in "${deep_dive_pages[@]}"; do
		capture_optional_qt_html "qt shot page ${page_name}" \
			"$live_dir/page-${page_name}.html" \
			"$qt_dir/page-${page_name}.png" \
			"$qt_dir/page-${page_name}.log" \
			--fit --scale 2
	done
}

{
	printf 'capture-baseline summary\n'
	printf 'started_utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
	printf 'repo_root=%s\n' "$repo_root"
	printf 'artifact_root=%s\n' "$artifact_root"
	printf 'python_bin=%s\n' "$python_bin"
	printf 'ruff_bin=%s\n' "${ruff_bin:-missing}"
	printf 'vulture_bin=%s\n' "${vulture_bin:-missing}"
	printf '\n'
} >"$summary_file"

capture_metadata_artifact "$live_dir/host-runtime-metadata.txt"

record_command 'pytest full suite' "$baseline_dir/pytest.log" \
	"$python_bin" -m pytest tests/ -v --tb=short

if [[ -n "$ruff_bin" ]]; then
	record_command 'ruff check' "$baseline_dir/ruff.log" "$ruff_bin" check .
else
	write_skip "$baseline_dir/ruff.log" 'Skipped: ruff not found in .venv/bin or PATH.'
fi

if [[ -n "$vulture_bin" ]]; then
	record_command 'vulture dead-code sweep' "$baseline_dir/vulture.log" \
		"$vulture_bin" src/ tests/ pirostats tests/vulture_whitelist.py --min-confidence 60
else
	write_skip "$baseline_dir/vulture.log" 'Skipped: vulture not found in .venv/bin or PATH.'
fi

record_command 'bash -n shell syntax sweep' "$baseline_dir/bash-n.log" \
	bash -n install.sh uninstall.sh scripts/update-skills.sh scripts/capture-baseline.sh

record_command 'pirostats render smoke' "$baseline_dir/cli-render.txt" \
	"$python_bin" ./pirostats render
record_command 'pirostats probe smoke' "$baseline_dir/cli-probe.txt" \
	"$python_bin" ./pirostats probe --config config/config.toml
record_command 'pirostats list-items smoke' "$baseline_dir/cli-list-items.txt" \
	"$python_bin" ./pirostats list-items

record_command 'pirostats profiling' "$live_dir/profiling.txt" \
	"$python_bin" ./pirostats profiling --config config/config.toml

capture_render_html 'render tooltip html' "$live_dir/render-tooltip.log" \
	"$tmp_tooltip_html" "$live_dir/tooltip.html" \
	"$python_bin" ./pirostats render --component tooltip --format html
capture_render_html 'render panel horizontal html' "$live_dir/render-panel-horizontal.log" \
	"$tmp_panel_html" "$live_dir/panel-horizontal.html" \
	"$python_bin" ./pirostats render --component panel --layout horizontal --format html
capture_render_html 'render panel vertical html' "$live_dir/render-panel-vertical.log" \
	"$tmp_panel_html" "$live_dir/panel-vertical.html" \
	"$python_bin" ./pirostats render --component panel --layout vertical --format html
capture_deep_dive_page_renders

if "$python_bin" -c 'import importlib.util, sys; sys.exit(0 if importlib.util.find_spec("PyQt6") else 1)' >/dev/null 2>&1; then
	capture_qt_html 'qt shot tooltip' \
		"$live_dir/tooltip.html" "$qt_dir/tooltip.png" "$qt_dir/tooltip.log" \
		--fit --scale 2
	capture_qt_html 'qt shot panel horizontal' \
		"$live_dir/panel-horizontal.html" "$qt_dir/panel-horizontal.png" "$qt_dir/panel-horizontal.log" \
		--width 1400 --height 96 --point --size 8 --scale 2
	capture_qt_html 'qt shot panel vertical' \
		"$live_dir/panel-vertical.html" "$qt_dir/panel-vertical.png" "$qt_dir/panel-vertical.log" \
		--width 80 --height 1200 --point --size 8 --scale 2
	capture_deep_dive_page_qt_shots
else
	write_skip "$qt_dir/qt-shots-skipped.txt" \
		"Qt screenshots skipped: PyQt6 not available via $python_bin for panel, tooltip, and deep-dive pages."
fi

{
	printf '\nfinished_utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
	printf 'required_failures=%s\n' "$failures"
	if (( failures == 0 )); then
		printf 'status=pass\n'
	else
		printf 'status=fail\n'
	fi
} >>"$summary_file"

printf 'Artifacts written under %s\n' "$artifact_root"
printf 'Summary: %s required failures\n' "$failures"

if (( failures > 0 )); then
	exit 1
fi
