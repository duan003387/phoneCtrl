#!/usr/bin/env bash
# 在 Windows（Git Bash / MSYS）里准备随包分发的 adb 三件套，放进 src-tauri/resources/adb/。
# 构建 Windows 安装包前运行一次；tauri.conf 的 resources 用 glob "resources/adb/*" 自动打包存在的文件。
#
# 用法：
#   ./scripts/fetch-adb-windows.sh            # 自动下载官方 platform-tools 并抽取
#   ./scripts/fetch-adb-windows.sh /path/to/platform-tools   # 从本地已解压目录拷贝
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
DEST="$ROOT/src-tauri/resources/adb"
mkdir -p "$DEST"

SRC="${1:-}"
if [ -z "$SRC" ]; then
  echo "[adb-win] 下载 platform-tools (Windows) ..."
  TMP="$(mktemp -d)"
  URL="https://dl.google.com/android/repository/platform-tools-latest-windows.zip"
  curl -L -o "$TMP/pt.zip" "$URL"
  unzip -q -o "$TMP/pt.zip" -d "$TMP"
  SRC="$TMP/platform-tools"
fi

for f in adb.exe AdbWinApi.dll AdbWinUsbApi.dll; do
  if [ ! -f "$SRC/$f" ]; then
    echo "[adb-win] 缺少 $f（源目录：$SRC）" >&2
    exit 1
  fi
  cp -f "$SRC/$f" "$DEST/$f"
  echo "[adb-win] 已复制 $f"
done

# Windows 构建时不需要 mac 版 adb，删除以免打进安装包（可选）。
[ -f "$DEST/adb" ] && rm -f "$DEST/adb" && echo "[adb-win] 已移除 mac 版 adb"

echo "[adb-win] 完成 → $DEST"
ls -lh "$DEST"
