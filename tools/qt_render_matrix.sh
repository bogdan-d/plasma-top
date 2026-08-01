#!/usr/bin/env bash
# Rasterize the Rust panel/tooltip matrix with Qt RichText in a fixed environment.
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: tools/qt_render_matrix.sh [--no-build]

Renders panel H/V, main tooltip, and all deep-dive pages in dark/light/overlay.
Fails on render/Qt errors, invalid PNGs, tables, or cross-theme layout drift.
Writes screenshots and an environment manifest under .test-artifacts/plasma/qt/.
EOF
}

build=true
while (($#)); do
    case "$1" in
    --no-build) build=false ;;
    -h | --help)
        usage
        exit 0
        ;;
    *)
        echo "unknown argument: $1" >&2
        usage >&2
        exit 2
        ;;
    esac
    shift
done

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary="$repo_dir/target/release/plasma-top"
python="${PYTHON:-python3}"
artifact_root="$repo_dir/.test-artifacts/plasma/qt"
tmp_root="$(mktemp -d /tmp/plasma-top-qt-render.XXXXXX)"
trap 'rm -rf "$tmp_root"' EXIT

"$python" -c 'import PyQt6.QtGui' >/dev/null 2>&1 || {
    echo "PyQt6 unavailable through: $python (set PYTHON to another interpreter)" >&2
    exit 1
}
if [[ "$build" == true ]]; then
    cargo build --manifest-path "$repo_dir/Cargo.toml" --release --locked
fi
[[ -x "$binary" ]] || {
    echo "Rust binary not found: $binary" >&2
    exit 1
}

rm -rf "$artifact_root"
mkdir -p "$artifact_root"/{html,png,logs}

# The deterministic Rust golden gate runs before Qt rasterization.
cargo test --manifest-path "$repo_dir/Cargo.toml" --all-features \
    golden -- --nocapture >"$artifact_root/logs/rust-golden.log" 2>&1

make_config() {
    local variant="$1" dir="$2"
    mkdir -p "$dir/home/.config"
    cp "$repo_dir/config/config.toml" "$dir/config.toml"
    if [[ "$variant" == light ]]; then
        printf '[Colors:Window]\nBackgroundNormal=238,238,238\n' >"$dir/home/.config/kdeglobals"
    elif [[ "$variant" == overlay ]]; then
        sed -i 's/^overlay[[:space:]]*=.*/overlay = true/' "$dir/config.toml"
    fi
}

cells=(panel-h panel-v tooltip processes cpu_cores connections fastfetch graphs)
variants=(dark light overlay)

for cell in "${cells[@]}"; do
    for variant in "${variants[@]}"; do
        env_dir="$tmp_root/$cell-$variant"
        make_config "$variant" "$env_dir"
        html="$artifact_root/html/$cell-$variant.html"
        png="$artifact_root/png/$cell-$variant.png"
        render_log="$artifact_root/logs/render-$cell-$variant.log"
        qt_log="$artifact_root/logs/qt-$cell-$variant.log"
        bg="#3b4a5a"
        [[ "$variant" == light ]] && bg="#eeeeee"

        case "$cell" in
        panel-h)
            args=(render --config "$env_dir/config.toml" --component panel --layout horizontal --format html)
            source=/tmp/plasma-top_render_panel.html
            qt=(--width 1400 --height 96 --point --size 8 --scale 2)
            ;;
        panel-v)
            args=(render --config "$env_dir/config.toml" --component panel --layout vertical --format html)
            source=/tmp/plasma-top_render_panel.html
            qt=(--width 80 --height 1200 --point --size 8 --scale 2)
            ;;
        tooltip)
            args=(render --config "$env_dir/config.toml" --component tooltip --format html)
            source=/tmp/plasma-top_render_tooltip.html
            qt=(--fit --point --size 11 --lineheight 1.05 --scale 2)
            ;;
        fastfetch)
            args=(render --config "$env_dir/config.toml" --page "$cell" --format html)
            source=/tmp/plasma-top_render_tooltip.html
            qt=(--fit --point --size 11 --lineheight 1.05 --scale 2 --plasmoid-output)
            ;;
        *)
            args=(render --config "$env_dir/config.toml" --page "$cell" --format html)
            source=/tmp/plasma-top_render_tooltip.html
            qt=(--fit --point --size 11 --lineheight 1.05 --scale 2)
            ;;
        esac

        HOME="$env_dir/home" "$binary" "${args[@]}" >"$render_log" 2>&1
        cp "$source" "$html"
        ! grep -qi '<table' "$html" || {
            echo "table found in $html" >&2
            exit 1
        }
        QT_QPA_PLATFORM=offscreen "$python" "$repo_dir/tools/qt_shot.py" \
            --html "$html" "$png" --bg "$bg" "${qt[@]}" >"$qt_log" 2>&1
    done
done

# Images must decode and be non-empty. The manifest and contact sheet make the
# fixed-host visual review reproducible; live sensor rows may change dimensions
# between sequential renders, so dimensions are evidence, not a false gate.
"$python" - "$artifact_root" <<'PY'
from pathlib import Path
import json
import sys
from PyQt6.QtCore import Qt
from PyQt6.QtGui import QColor, QFont, QGuiApplication, QImage, QPainter

root = Path(sys.argv[1])
app = QGuiApplication([])
cells = ("panel-h", "panel-v", "tooltip", "processes", "cpu_cores", "connections", "fastfetch", "graphs")
variants = ("dark", "light", "overlay")
manifest = {}
for cell in cells:
    sizes = {}
    for variant in variants:
        path = root / "png" / f"{cell}-{variant}.png"
        image = QImage(str(path))
        if image.isNull() or image.width() <= 0 or image.height() <= 0:
            raise SystemExit(f"invalid image: {path}")
        sizes[variant] = [image.width(), image.height()]
    manifest[cell] = sizes
(root / "images.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")

tile_w, tile_h, label_h = 420, 300, 24
sheet = QImage(tile_w * len(variants), (tile_h + label_h) * len(cells), QImage.Format.Format_RGB32)
sheet.fill(QColor("#202020"))
painter = QPainter(sheet)
painter.setPen(QColor("white"))
painter.setFont(QFont("Noto Sans", 10))
for row, cell in enumerate(cells):
    for col, variant in enumerate(variants):
        image = QImage(str(root / "png" / f"{cell}-{variant}.png"))
        scaled = image.scaled(tile_w, tile_h, Qt.AspectRatioMode.KeepAspectRatio,
                              Qt.TransformationMode.SmoothTransformation)
        x = col * tile_w + (tile_w - scaled.width()) // 2
        y0 = row * (tile_h + label_h)
        y = y0 + label_h + (tile_h - scaled.height()) // 2
        painter.drawText(col * tile_w + 6, y0 + 17, f"{cell} / {variant}")
        painter.drawImage(x, y, scaled)
painter.end()
if not sheet.save(str(root / "contact-sheet.png")):
    raise SystemExit("failed to save contact sheet")
PY

{
    echo "timestamp_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "kernel=$(uname -srmo)"
    echo "session=${XDG_SESSION_TYPE:-unknown}"
    echo "wayland=${WAYLAND_DISPLAY:-none}"
    echo "plasmawindowed=$(plasmawindowed --version 2>&1 | head -1)"
    "$python" - <<'PY'
from PyQt6.QtCore import QT_VERSION_STR, PYQT_VERSION_STR
print(f"qt={QT_VERSION_STR}")
print(f"pyqt={PYQT_VERSION_STR}")
PY
    fc-match 'NotoSansM Nerd Font Mono' | head -1 | sed 's/^/font=/'
    sha256sum "$artifact_root"/png/*.png
} >"$artifact_root/environment.txt"

echo "Qt render matrix passed: ${artifact_root#"$repo_dir"/}/images.json"
