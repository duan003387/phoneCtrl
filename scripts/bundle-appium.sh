#!/usr/bin/env bash
# 构建期：生成内置便携 Appium 运行时（离线分发用）。
# 产出 src-tauri/resources/appium-bundle.tar.gz，内含：
#   bin/node | bin/node.exe        —— 可移植 node 运行时（按目标 OS 命名，见下）
#   node_modules/appium/…         —— Appium 本体（npm 安装）
#   .appium/                      —— UiAutomator2 驱动（APPIUM_HOME 隔离到包内）
#
# ⚠️ 需在“与目标一致的 OS”上运行（mac 出 mac 包、Windows 在 Git Bash 里出 win 包），
#    因为 node 与原生依赖不跨平台。架构无关：node 二进制本身的命名才是关键，
#    通用 bin/node 在 Windows 上找不到（运行时按 bin/node.exe 查找）。运行一次即可
#    （需联网 npm install，属构建期）。
#
# 用法： ./scripts/bundle-appium.sh                       # mac / linux
#        bash scripts/bundle-appium.sh                   # Windows（Git Bash / MSYS）
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$ROOT/src-tauri/resources/appium-bundle.tar.gz"
WORK="$(mktemp -d)"
BUNDLE="$WORK/appium-bundle"
APPIUM_VERSION="${APPIUM_VERSION:-3}"

# node 落盘名只看 OS（不看架构）：Appium 官方只分发 mac/linux/windows，
# 运行端 automate.rs 在 Windows 上查找 bin/node.exe，两边必须一致。
case "$(uname -s)" in
  Darwin|Linux)          NODE_BIN="node" ;;
  MINGW*|MSYS*|CYGWIN*)  NODE_BIN="node.exe" ;;
  *) echo "[bundle-appium] 不支持的系统: $(uname -s)" >&2; exit 1 ;;
esac

mkdir -p "$BUNDLE/bin"
cd "$BUNDLE"

echo "[bundle-appium] npm install appium@$APPIUM_VERSION (构建期联网) …"
printf '{"name":"phonectrl-appium-runtime","private":true,"version":"1.0.0"}\n' > package.json
npm install --no-audit --no-fund --prefix . "appium@$APPIUM_VERSION"

echo "[bundle-appium] 安装 uiautomator2 驱动到包内 APPIUM_HOME …"
# CLI 入口优先级须与 automate.rs::bundled_launch 一致（index.js 优先），
# 否则构建期与运行期可能调到不同的入口上。
CLI="$BUNDLE/node_modules/appium/index.js"
[ -f "$CLI" ] || CLI="$BUNDLE/node_modules/appium/build/lib/main.js"
APPIUM_HOME="$BUNDLE/.appium" node "$CLI" driver install uiautomator2

echo "[bundle-appium] 复制 node 运行时 → bin/$NODE_BIN …"
# 用 execPath 取 node 真身：Git Bash 的 `command -v node` 会省掉 .exe 而拷不到文件。
NODE_SRC="$(node -e 'process.stdout.write(process.execPath)')"
if command -v cygpath >/dev/null 2>&1; then
  NODE_SRC="$(cygpath -u "$NODE_SRC")"
fi
cp "$NODE_SRC" "$BUNDLE/bin/$NODE_BIN"

echo "[bundle-appium] 打包 → $OUT"
tar -czf "$OUT" -C "$BUNDLE" bin node_modules .appium package.json

rm -rf "$WORK"
echo "[bundle-appium] 完成。大小：$(du -h "$OUT" | cut -f1)"
