#!/usr/bin/env bash
set -euo pipefail

# ==============================================================================
# macos-trackpad-companion 全景诊断与日志采集脚本
# ==============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DIAG_DIR="$REPO_ROOT/diagnostics"
mkdir -p "$DIAG_DIR"

SETTINGS_FILE="$DIAG_DIR/mac-settings.txt"
LOG_FILE="$DIAG_DIR/companion-live.log"

echo "========================================================"
echo "🔍 1. 采集 macOS 系统触控板默认设置与系统环境..."
echo "========================================================"

{
    echo "=== 系统信息 (System Info) ==="
    sw_vers 2>/dev/null || true
    uname -a
    echo ""

    echo "=== 触控板设置: com.apple.AppleMultitouchTrackpad ==="
    defaults read com.apple.AppleMultitouchTrackpad 2>/dev/null || echo "(未找到或默认)"
    echo ""

    echo "=== 触控板设置: com.apple.driver.AppleBluetoothMultitouch.trackpad ==="
    defaults read com.apple.driver.AppleBluetoothMultitouch.trackpad 2>/dev/null || echo "(未找到或默认)"
    echo ""

    echo "=== 全局手势设置 (Global Defaults) ==="
    echo "com.apple.swipescrolldirection: $(defaults read -g com.apple.swipescrolldirection 2>/dev/null || echo '未设置')"
    echo "com.apple.trackpad.scaling: $(defaults read -g com.apple.trackpad.scaling 2>/dev/null || echo '未设置')"
    echo "com.apple.scrollwheel.scaling: $(defaults read -g com.apple.scrollwheel.scaling 2>/dev/null || echo '未设置')"
    echo "(The companion reads these global keys via CFPreferences and applies bounded compatibility mappings; missing keys are normal.)"
    echo ""

    echo "=== Dock 手势与 Space 设置 (com.apple.dock) ==="
    defaults read com.apple.dock showMissionControlGestureEnabled 2>/dev/null || echo "showMissionControlGestureEnabled: (默认)"
    defaults read com.apple.dock showAppExposeGestureEnabled 2>/dev/null || echo "showAppExposeGestureEnabled: (默认)"
    defaults read com.apple.dock showDesktopGestureEnabled 2>/dev/null || echo "showDesktopGestureEnabled: (默认)"
    echo ""

} > "$SETTINGS_FILE"

echo "✅ 系统与触控板设置已导出至: $SETTINGS_FILE"
echo ""

echo "========================================================"
echo "🚀 2. 启动 companion 并开启全景 Trace 追踪日志..."
echo "========================================================"
echo "实时日志将同步写入文件: $LOG_FILE"
echo "您可以现在在手机上进行操作（测试双指双击、三指查词、旋转等）"
echo "按 Ctrl+C 即可退出并保存日志。"
echo "========================================================"
echo ""

# 清空旧日志
> "$LOG_FILE"

# 以 RUST_LOG=macos_trackpad_companion=trace 运行 companion_net 并同时输出到终端和文件
export RUST_LOG="macos_trackpad_companion=trace"
export RUST_BACKTRACE=1

echo "📦 正在编译最新代码..."
cargo build --release --bin companion-net
"$REPO_ROOT/target/release/companion-net" 2>&1 | tee -a "$LOG_FILE"
