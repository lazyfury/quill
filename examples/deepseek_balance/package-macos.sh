#!/usr/bin/env bash
#
# Packages the release binary as a macOS .app bundle.
#
#     ./package-macos.sh            # build --release, then assemble dist/
#     ./package-macos.sh --open     # ... and launch it afterwards
#
# The bundle is written to `dist/` (git-ignored) and is made of three things:
# the executable, `Info.plist`, and — since a menu-bar app has no Dock icon —
# no icon at all. `codesign` runs ad-hoc (`-`): enough for a locally built app
# to launch without the "damaged" Gatekeeper error, not a distribution signature.

set -euo pipefail

APP_NAME="DeepSeek Balance"
BINARY="deepseek_balance"

# Paths are resolved from this script, so it works from any cwd.
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MANIFEST="$HERE/Cargo.toml"
PLIST="$HERE/packaging/Info.plist"
DIST="$HERE/dist"
APP="$DIST/$APP_NAME.app"
BUILT="$HERE/target/release/$BINARY"

open_after=false
for arg in "$@"; do
	case "$arg" in
		--open) open_after=true ;;
		*)
			echo "未知参数: $arg" >&2
			exit 2
			;;
	esac
done

if [ "$(uname -s)" != "Darwin" ]; then
	echo "这个脚本只在 macOS 上有意义（.app bundle 是 macOS 的概念）" >&2
	exit 1
fi

echo "==> cargo build --release"
cargo build --release --manifest-path "$MANIFEST"

echo "==> 组装 $APP"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BUILT" "$APP/Contents/MacOS/$BINARY"
cp "$PLIST" "$APP/Contents/Info.plist"

# The executable name and the plist must agree, or the app launches nothing.
declared="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$APP/Contents/Info.plist")"
if [ "$declared" != "$BINARY" ]; then
	echo "Info.plist 的 CFBundleExecutable ($declared) 与二进制名 ($BINARY) 不一致" >&2
	exit 1
fi

/usr/bin/plutil -lint "$APP/Contents/Info.plist"

echo "==> codesign (ad-hoc)"
codesign --force --sign - "$APP"
codesign --verify --verbose=2 "$APP"

echo "==> 完成"
echo "$APP"
du -sh "$APP" | awk '{ print "  体积: " $1 }'
echo "  运行: open \"$APP\"    # 菜单栏模式，状态栏会出现余额"
echo "  删除: rm -rf \"$APP\""

if [ "$open_after" = true ]; then
	open "$APP"
fi
