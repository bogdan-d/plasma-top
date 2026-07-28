#!/usr/bin/env bash
set -euo pipefail

repo_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_dir"

fail() {
	echo "repository gate: $*" >&2
	exit 1
}

required=(
	rust/Cargo.toml rust/Cargo.lock rust/src/main.rs rust/src/lib.rs
	rust/tests/golden/panel_h.html rust/tests/golden/panel_v.html
	rust/tests/golden/tooltip.html
	rust/tests/fixtures/oracle/oracle_render_full.toml
	pirostats install.sh uninstall.sh packaging/pirostats-launcher
	service/pirostats.service service/pirostats-user.service
	packaging/aur/PKGBUILD packaging/aur/pirostats.install
)
for path in "${required[@]}"; do
	[[ -f "$path" ]] || fail "missing required product/evidence file: $path"
done

mapfile -t python_files < <(
	find . -type f -name '*.py' \
		-not -path './.git/*' -not -path './.venv/*' \
		-not -path './rust/target/*' -not -path './.agents/*' \
		-printf '%P\n' | sort
)
expected_python=$'tools/p6_png_diff.py\ntools/qt_shot.py'
[[ "${python_files[*]}" == "${expected_python//$'\n'/ }" ]] || {
	printf 'repository gate: unexpected Python files:\n%s\n' "${python_files[*]:-(none)}" >&2
	exit 1
}

retired=(src tests rust/tests/parity_runner.sh scripts/capture-baseline.sh \
	tools/python_oracle.py tools/inventory_ast_reporter.py \
	tools/demo_shot.py tools/manual_tooltip_preview.py)
for path in "${retired[@]}"; do
	[[ ! -e "$path" ]] || fail "retired migration path remains: $path"
done

production_surfaces=(pirostats install.sh uninstall.sh packaging service plasmoid .github)
if rg -n '(python[0-9]*|src/[^ ]*\.py|python_oracle|parity_runner)' \
	"${production_surfaces[@]}"; then
	fail "Python runtime path remains on a production surface"
fi

grep -Fqx 'ExecStart=/usr/bin/pirostats daemon' service/pirostats.service \
	|| fail "system service launcher drift"
grep -Fqx 'ExecStart=%h/.local/bin/pirostats daemon' service/pirostats-user.service \
	|| fail "user service launcher drift"
grep -Fq 'exec /usr/lib/pirostats/pirostats "$@"' packaging/pirostats-launcher \
	|| fail "package launcher drift"
grep -Fq "makedepends=('cargo' 'git')" packaging/aur/PKGBUILD \
	|| fail "AUR Rust build dependencies drift"
grep -Fq 'canonical_width_covers_every_tooltip_item' rust/src/render/formatter.rs \
	|| fail "canonical-width closure test missing"
grep -Fq 'html.contains("<table")' rust/src/render/mono.rs \
	|| fail "table-free render assertion missing"

echo "repository gate: ok"
