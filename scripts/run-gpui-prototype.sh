#!/usr/bin/env bash
set -euo pipefail

usage() {
  printf '用法：%s [--release] [--bundle-only | --workspace <目录>]\n' "$0"
  printf '默认以 debug 配置构建并启动。--bundle-only 只生成本地 .app。\n'
}

profile=debug
launch=true
workspace_path=
while [[ $# -gt 0 ]]; do
  case "$1" in
    --release) profile=release ;;
    --bundle-only) launch=false ;;
    --workspace)
      if [[ $# -lt 2 || -z "$2" || -n "$workspace_path" ]]; then
        usage >&2
        exit 2
      fi
      workspace_path="$2"
      shift
      ;;
    -h|--help) usage; exit 0 ;;
    *) usage >&2; exit 2 ;;
  esac
  shift
done

if [[ -n "$workspace_path" ]]; then
  if [[ "$launch" != true ]]; then
    usage >&2
    exit 2
  fi
  if [[ "$workspace_path" != /* ]]; then
    workspace_path="$PWD/$workspace_path"
  fi
fi

if [[ "$(uname -s)" != Darwin ]]; then
  printf '此启动脚本用于 macOS 原型。\n' >&2
  exit 1
fi

export PATH="$HOME/.cargo/bin:$PATH"
if ! command -v cargo >/dev/null 2>&1; then
  printf '找不到 Cargo。请安装原型 Cargo.toml 要求的 Rust 工具链。\n' >&2
  exit 1
fi

project_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/../src-gpui" && pwd -P)"
target_dir="$project_dir/target"
bundle_dir="$target_dir/bundle/macos"
app_name='WorkReport GPUI Prototype'
executable_name=workreport-gpui-prototype
bundle="$bundle_dir/$app_name.app"
bundle_executable="$bundle/Contents/MacOS/$executable_name"

if [[ "$launch" == true ]]; then
  pattern="$(printf '%s' "$bundle_executable" | /usr/bin/sed 's/[][\\.^$*+?(){}|]/\\&/g')( .*)?"
  if /usr/bin/pgrep -f -x "$pattern" >/dev/null; then
    printf '原型正在运行。请从应用内保存或放弃草稿并退出，再重新启动。\n' >&2
    exit 1
  else
    status=$?
    if [[ "$status" != 1 ]]; then
      printf '无法检查当前原型进程。可用 --bundle-only 生成本地应用包。\n' >&2
      exit "$status"
    fi
  fi
fi

build_flags=(--locked --target-dir "$target_dir")
if [[ "$profile" == release ]]; then
  build_flags+=(--release)
fi
(cd "$project_dir" && env -u CARGO_BUILD_TARGET cargo build "${build_flags[@]}")

mkdir -p "$bundle_dir"
stage_dir="$(mktemp -d "$bundle_dir/.prototype.XXXXXX")"
trap 'rm -rf "$stage_dir"' EXIT
stage_bundle="$stage_dir/$app_name.app"
mkdir -p "$stage_bundle/Contents/MacOS"
/usr/bin/install -m 755 "$target_dir/$profile/$executable_name" \
  "$stage_bundle/Contents/MacOS/$executable_name"

cat >"$stage_bundle/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key>
  <string>com.workreport.gpui-prototype</string>
  <key>CFBundleExecutable</key>
  <string>workreport-gpui-prototype</string>
  <key>CFBundleName</key>
  <string>WorkReport GPUI Prototype</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.0</string>
  <key>CFBundleVersion</key>
  <string>1</string>
  <key>NSPrincipalClass</key>
  <string>NSApplication</string>
</dict>
</plist>
PLIST

/usr/bin/plutil -lint "$stage_bundle/Contents/Info.plist"
/usr/bin/codesign --force --sign - "$stage_bundle"
/usr/bin/codesign --verify --strict "$stage_bundle"

rm -rf "$bundle"
mv "$stage_bundle" "$bundle"
printf '%s\n' "$bundle"

if [[ "$launch" == true ]]; then
  if [[ -n "$workspace_path" ]]; then
    /usr/bin/open -n "$bundle" --args --workspace "$workspace_path"
  else
    /usr/bin/open -n "$bundle"
  fi
fi
