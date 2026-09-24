#!/usr/bin/env bash
# 一键：签名 + 打包 + 公证 + staple，产出可对外分发的 mac dmg。
#
# 前置（只需一次）：
#  1) 在 https://developer.apple.com/account → Certificates 创建 “Developer ID Application” 证书
#     （用你的组织账号，Team ID: X88VRL9YR4），下载并双击导入“登录”钥匙串。
#     校验：security find-identity -v -p codesigning 里应出现 “Developer ID Application: …(X88VRL9YR4)”
#  2) 生成 App 专用密码：https://appleid.apple.com → 登录 → 密码安全性 → App 专用密码。
#
# 用法（在本机 shell）：
#   export APPLE_ID="你的@apple.com"
#   export APPLE_APP_PWD="abcd-efgh-ijkl-mnop"     # App 专用密码（不是 Apple ID 登录密码）
#   export APPLE_TEAM_ID="X88VRL9YR4"
#   ./scripts/sign-notarize-mac.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TAURI="$ROOT/src-tauri"
IDENTITY="${APPLE_SIGNING_IDENTITY:-Developer ID Application: Guangzhou Xiao Koi Culture Technology Co., Ltd. (X88VRL9YR4)}"

: "${APPLE_ID:?请先 export APPLE_ID}"
: "${APPLE_APP_PWD:?请先 export APPLE_APP_PWD（App 专用密码）}"
: "${APPLE_TEAM_ID:?请先 export APPLE_TEAM_ID}"

echo "[sign] 校验签名身份存在…"
if ! security find-identity -v -p codesigning | grep -q "Developer ID Application"; then
  echo "钥匙串里没有 'Developer ID Application' 证书。请先按脚本顶部注释创建并导入。" >&2
  exit 1
fi

echo "[sign] 预签内置 adb（它是 Mach-O，公证要求 bundle 内每个可执行文件都被我们签）…"
if [ -f "$TAURI/resources/adb/adb" ]; then
  codesign --force --timestamp --options runtime \
    --entitlements "$TAURI/entitlements.plist" \
    --sign "$IDENTITY" "$TAURI/resources/adb/adb"
fi

echo "[sign] 构建并签名（Tauri 会用 APPLE_SIGNING_IDENTITY 自动 codesign .app/dmg）…"
export APPLE_SIGNING_IDENTITY="$IDENTITY"
cd "$ROOT"
npm run tauri build

TARGET="$TAURI/target"
# dmg 文件名里带的版本取自 tauri.conf.json；写死版本号会让改了版本的发布直接失败。
VERSION="$(sed -n 's/.*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$TAURI/tauri.conf.json" | head -1)"
if [ -z "$VERSION" ]; then
  echo "无法从 src-tauri/tauri.conf.json 解析 version" >&2
  exit 1
fi

# 本机架构产物在 target/release/bundle/dmg，交叉 --target 构建在 target/<triple>/release/bundle/dmg。
shopt -s nullglob
DMGS=(
  "$TARGET/release/bundle/dmg/PhoneCtrl_${VERSION}"_*.dmg
  "$TARGET"/*/release/bundle/dmg/"PhoneCtrl_${VERSION}"_*.dmg
)
shopt -u nullglob
if [ "${#DMGS[@]}" -eq 0 ]; then
  echo "找不到 PhoneCtrl_${VERSION}_*.dmg：构建是否成功？是否换了 --target？" >&2
  exit 1
fi

for DMG in "${DMGS[@]}"; do
  # 每个 dmg 旁边就是同一次构建的 .app（bundle/dmg 与 bundle/macos 同级）
  APP="$(dirname "$(dirname "$DMG")")/macos/PhoneCtrl.app"
  if [ ! -d "$APP" ]; then
    echo "缺少已签名的 $APP（tauri build 是否带了签名身份？）" >&2
    exit 1
  fi

  echo "[sign] 校验签名：$(basename "$APP") ($APP)"
  codesign --verify --deep --strict --verbose=2 "$APP"
  spctl -a -vv -t install "$APP" || true

  echo "[notarize] 提交公证：$(basename "$DMG")（可能需要 1-5 分钟）…"
  xcrun notarytool submit "$DMG" \
    --apple-id "$APPLE_ID" \
    --password "$APPLE_APP_PWD" \
    --team-id "$APPLE_TEAM_ID" \
    --wait

  echo "[staple] 把公证票据钉到 dmg 上…"
  xcrun stapler staple "$DMG"
  xcrun stapler validate "$DMG"
  # 对 .app 也一并 staple，便于拷盘分发
  xcrun stapler staple "$APP" 2>/dev/null || true

  echo "完成 ✅  可分发：$DMG"
done
