#!/usr/bin/env bash
#
# 把 MarkView 打包成 macOS 的 .app。
#
# 为什么需要这一步：macOS 忽略窗口级别的图标设置，Dock 和 Finder 只认 bundle 里
# CFBundleIconFile 指定的 .icns。所以 `cargo run` 出来的程序在 Dock 里是通用图标，
# 只有装进 bundle 才会显示我们画的图标。
#
#   用法：packaging/build-app.sh
#   产物：dist/MarkView.app
#
# 可用环境变量覆盖：BUNDLE_ID（默认是个占位值，发布前请改成自己的反域名）。

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# 应用名和可执行文件名必须和 Cargo.toml 里的 package 名对得上，
# 否则下面 cp 的时候会找不到文件（这里以前就踩过一次）。
APP_NAME="MarkView"
BIN_NAME="markview"
BUNDLE_ID="${BUNDLE_ID:-com.example.markview}"

# 版本号只认 Cargo.toml，免得两处各写一份然后慢慢对不上
VERSION="$(sed -n 's/^version *= *"\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -n 1)"
DIST="$ROOT/dist"
APP="$DIST/$APP_NAME.app"

if [ ! -x "$ROOT/target/release/$BIN_NAME" ]; then
	echo "==> 构建 release"
	cargo build --release --manifest-path "$ROOT/Cargo.toml"
fi

if [ ! -f "$ROOT/assets/AppIcon.icns" ]; then
	echo "==> 生成图标"
	cargo run --release --quiet --example make_icon --manifest-path "$ROOT/Cargo.toml"
fi

echo "==> 组装 $APP"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$ROOT/target/release/$BIN_NAME" "$APP/Contents/MacOS/$BIN_NAME"
cp "$ROOT/assets/AppIcon.icns" "$APP/Contents/Resources/AppIcon.icns"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleName</key>
	<string>$APP_NAME</string>
	<key>CFBundleDisplayName</key>
	<string>$APP_NAME</string>
	<key>CFBundleExecutable</key>
	<string>$BIN_NAME</string>
	<key>CFBundleIdentifier</key>
	<string>$BUNDLE_ID</string>
	<key>CFBundleIconFile</key>
	<string>AppIcon</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleDevelopmentRegion</key>
	<string>en</string>
	<key>LSApplicationCategoryType</key>
	<string>public.app-category.productivity</string>
	<key>CFBundleShortVersionString</key>
	<string>$VERSION</string>
	<key>CFBundleVersion</key>
	<string>$VERSION</string>
	<key>LSMinimumSystemVersion</key>
	<string>11.0</string>
	<key>NSHighResolutionCapable</key>
	<true/>
	<key>CFBundleDocumentTypes</key>
	<array>
		<dict>
			<key>CFBundleTypeName</key>
			<string>Markdown document</string>
			<key>CFBundleTypeRole</key>
			<string>Editor</string>
			<key>LSHandlerRank</key>
			<string>Alternate</string>
			<key>LSItemContentTypes</key>
			<array>
				<string>net.daringfireball.markdown</string>
				<string>public.plain-text</string>
			</array>
		</dict>
	</array>
</dict>
</plist>
PLIST

# 临时签名。自己编译的程序不签也能跑，但签一下能避免部分 Gatekeeper 提示。
if command -v codesign >/dev/null 2>&1; then
	echo "==> 临时签名"
	codesign --force --sign - "$APP" >/dev/null 2>&1 || echo "    （签名失败，不影响本地运行）"
fi

# 让 Finder / Dock 立刻刷新图标缓存，不然可能还显示旧图标
touch "$APP"

echo
echo "==> 完成：$APP"
echo "    运行：open \"$APP\""
echo "    换图标后如果 Dock 还显示旧的，把图标拖出去再拖回来，或 killall Dock"
