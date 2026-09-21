#!/usr/bin/env bash
# 编译 sendevent 注入桥并打包成 inputbridge.jar。
# 用法：./bridge/build.sh
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
SDK="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
BUILD_TOOLS="$(ls -1 "$SDK/build-tools" | sort -V | tail -1)"
D8="$SDK/build-tools/$BUILD_TOOLS/d8"

OUT="$HERE/build"
rm -rf "$OUT"
mkdir -p "$OUT/classes"

echo "[build.sh] javac InputBridge.java"
javac -source 8 -target 8 \
  -d "$OUT/classes" \
  "$HERE/InputBridge.java" 2>/dev/null || \
javac -d "$OUT/classes" "$HERE/InputBridge.java"

echo "[build.sh] d8 -> classes.dex"
"$D8" --output "$OUT" --release --min-api 21 \
  $(find "$OUT/classes" -name '*.class')

echo "[build.sh] package jar"
jar cf "$ROOT/resources/inputbridge.jar" -C "$OUT" classes.dex

echo "[build.sh] OK -> resources/inputbridge.jar"
