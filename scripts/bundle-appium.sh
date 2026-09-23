#!/usr/bin/env bash
# 构建期：生成内置便携 Appium 运行时（离线分发用）。
# 产出 src-tauri/resources/appium-bundle.tar.gz，内含：
#   bin/node                      —— 可移植 node 运行时
#   node_modules/appium/…         —— Appium 本体（npm 安装）
#   .appium/                      —— UiAutomator2 驱动（APPIUM_HOME 隔离到包内）
#
# ⚠️ 需在“与目标一致的 OS/架构”上运行（mac 出 mac 包、Windows 用等价 PowerShell），
#    因为 node 与原生依赖不跨平台。运行一次即可（需联网 npm install，属构建期）。
#
# 用法： ./scripts/bundle-appium.sh          # 构建机执行
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$ROOT/src-tauri/resources/appium-bundle.tar.gz"
WORK="$(mktemp -d)"
BUNDLE="$WORK/appium-bundle"
APPIUM_VERSION="${APPIUM_VERSION:-3}"

mkdir -p "$BUNDLE/bin"
cd "$BUNDLE"

echo "[bundle-appium] npm install appium@$APPIUM_VERSION (构建期联网) …"
printf '{"name":"phonectrl-appium-runtime","private":true,"version":"1.0.0"}\n' > package.json
npm install --no-audit --no-fund --prefix . "appium@$APPIUM_VERSION"

echo "[bundle-appium] 安装 uiautomator2 驱动到包内 APPIUM_HOME …"
CLI="$BUNDLE/node_modules/appium/build/lib/main.js"
[ -f "$CLI" ] || CLI="$BUNDLE/node_modules/appium/index.js"
APPIUM_HOME="$BUNDLE/.appium" node "$CLI" driver install uiautomator2

echo "[bundle-appium] 复制 node 运行时 …"
cp "$(command -v node)" "$BUNDLE/bin/node"

echo "[bundle-appium] 打包 → $OUT"
tar -czf "$OUT" -C "$BUNDLE" bin node_modules .appium package.json

rm -rf "$WORK"
echo "[bundle-appium] 完成。大小：$(du -h "$OUT" | cut -f1)"
