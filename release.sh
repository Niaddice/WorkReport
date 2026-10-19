#!/usr/bin/env bash
# Publish the current host's WorkReport build. Compatible with macOS Bash 3.2.
set -euo pipefail

REPOSITORY='Niaddice/WorkReport'
REMOTE_URL="https://github.com/${REPOSITORY}.git"
PROJECT_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
OUTPUT_ROOT="$PROJECT_ROOT/.release"
VERSION=''
VERSION_EXPLICIT=false
TAG=''
BRANCH=''
COMMIT=''
MESSAGE=''
NOTES_FILE=''
DRAFT=false
DRY_RUN=false
RESUME=false
RESUME_READY=false
LOCK_HELD=false
STEP='读取参数'
ASSETS=()

usage() {
  cat <<'HELP'
用法：
  ./release.sh                         自动选择版本、提交、构建、推送并发布
  ./release.sh 0.1.1                   指定发布版本（也接受 v0.1.1）
  ./release.sh --dry-run               仅预览，不修改文件、不联网、不发布
  ./release.sh 0.1.1 -m "修复日报保存"  指定提交说明
  ./release.sh --notes-file notes.md   使用 Markdown 发布说明
  ./release.sh --draft                 上传为草稿 Release
  ./release.sh --resume v0.1.1          使用已构建产物重试推送或发布

默认发布到 Niaddice/WorkReport。未指定版本时沿用 package.json 版本；
若标签或 Release 已存在，自动递增补丁版本。脚本会提交所有未被
.gitignore 忽略的更改。构建失败时不会推送；不会强制推送或覆盖正式 Release。
HELP
}
log() { printf '\n[WorkReport] %s\n' "$*"; }
die() { printf '\n错误：%s\n' "$*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || die "缺少命令 $1，请先安装。"; }
git_remote() {
  GIT_TERMINAL_PROMPT=0 git -c credential.helper= -c 'credential.helper=!gh auth git-credential' "$@"
}
cleanup() {
  local code=$?
  if $LOCK_HELD; then rmdir "$OUTPUT_ROOT/.publish.lock" 2>/dev/null || true; fi
  return "$code"
}
failed() {
  local code=$?
  trap - ERR
  printf '\n失败阶段：%s（退出码 %s）。已完成的本地提交、标签和安装包会保留。\n' "$STEP" "$code" >&2
  if $RESUME_READY; then
    printf '排除问题后重试：%q --resume %q\n' "$PROJECT_ROOT/release.sh" "$TAG" >&2
  else
    printf '排除问题后重新执行 release.sh；脚本不会自动回滚或强制推送。\n' >&2
  fi
  exit "$code"
}
trap cleanup EXIT
trap failed ERR

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) usage; exit 0 ;;
    --dry-run) DRY_RUN=true; shift ;;
    --draft) DRAFT=true; shift ;;
    --resume) [[ $# -ge 2 && -n "$2" && -z "$VERSION" ]] || die '--resume 需要一个标签，且不能同时指定其他版本'; RESUME=true; VERSION="${2#v}"; shift 2 ;;
    -m|--message) [[ $# -ge 2 && -n "$2" ]] || die '-m 需要提交说明'; MESSAGE="$2"; shift 2 ;;
    --notes-file) [[ $# -ge 2 && -f "$2" ]] || die '--notes-file 需要一个存在的文件'; NOTES_FILE="$(cd -- "$(dirname -- "$2")" && pwd -P)/$(basename -- "$2")"; shift 2 ;;
    -*) die "未知选项：$1" ;;
    *) [[ -z "$VERSION" ]] || die '只能指定一个版本'; VERSION="${1#v}"; VERSION_EXPLICIT=true; shift ;;
  esac
done
cd "$PROJECT_ROOT"
need node
CURRENT_VERSION="$(node -p "require('./package.json').version")"
VERSION="${VERSION:-$CURRENT_VERSION}"
node -e 'const v=process.argv[1]; if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(v) || !v.split(".").every(n=>Number.isSafeInteger(Number(n)))) process.exit(1)' "$VERSION" || die '版本格式应为 0.1.1（不含前导零）'
TAG="v$VERSION"

if $DRY_RUN; then
  printf '仓库：%s\n候选版本：%s（实际执行时检查标签和 Release）\n' "$REMOTE_URL" "$TAG"
  if $RESUME; then
    printf '重试流程：核对本地提交和安装包校验和 → 推送 → 创建或恢复 Release。\n'
  else
    printf '流程：检查工具和登录 → 连接仓库并保留历史 → 同步版本 → npm ci\n'
    printf '      → 提交更改 → 前端/Rust 测试 → Tauri 构建 → 打包/校验和\n'
    printf '      → 推送当前分支和标签 → 上传安装包 → %s。\n' "$(if $DRAFT; then printf '保留草稿'; else printf '正式发布'; fi)"
  fi
  printf 'macOS 产物：.app.zip；Linux：.deb / .AppImage；Windows Git Bash：.msi / .exe。\n'
  printf '本次仅预览，未初始化 Git、修改版本、构建、提交、推送或发布。\n'
  exit 0
fi

STEP='检查发布环境'
for tool in git gh npm cargo rustc; do need "$tool"; done
case "$(uname -s)" in
  Darwin) PLATFORM=macos; BUNDLES=app; need ditto ;;
  Linux) PLATFORM=linux; BUNDLES=deb,appimage ;;
  MINGW*|MSYS*|CYGWIN*) PLATFORM=windows; BUNDLES=nsis,msi ;;
  *) die '暂不支持当前构建平台。' ;;
esac
if ! command -v shasum >/dev/null 2>&1; then need sha256sum; fi
gh auth status --hostname github.com >/dev/null 2>&1 || die '请先执行 gh auth login --hostname github.com'
INFO="$(gh repo view "$REPOSITORY" --json nameWithOwner,defaultBranchRef,viewerPermission)"
PERMISSION="$(printf '%s' "$INFO" | node -pe 'JSON.parse(require("fs").readFileSync(0,"utf8")).viewerPermission')"
case "$PERMISSION" in ADMIN|MAINTAIN|WRITE) ;; *) die "当前 GitHub 账号没有 $REPOSITORY 的写入权限" ;; esac
DEFAULT_BRANCH="$(printf '%s' "$INFO" | node -pe 'JSON.parse(require("fs").readFileSync(0,"utf8")).defaultBranchRef?.name || "main"')"
git config user.name >/dev/null || die '请先配置 git config --global user.name "你的姓名"'
git config user.email >/dev/null || die '请先配置 git config --global user.email "你的 GitHub 邮箱"'
mkdir -p "$OUTPUT_ROOT"
mkdir "$OUTPUT_ROOT/.publish.lock" 2>/dev/null || die '另一个发布正在执行；确认没有发布进程后可移除 .release/.publish.lock'
LOCK_HELD=true

STEP='连接 Git 仓库'
if git rev-parse --show-toplevel >/dev/null 2>&1; then
  CHECKOUT_ROOT="$(cd -- "$(git rev-parse --show-toplevel)" && pwd -P)"
  [[ "$CHECKOUT_ROOT" == "$PROJECT_ROOT" ]] || die '项目位于其他 Git 仓库内部，请在独立的 WorkReport 检出目录执行。'
else
  $RESUME && die '无法重试：当前目录还没有 Git 仓库。'
  git init --initial-branch="$DEFAULT_BRANCH"
fi
if ! git remote get-url origin >/dev/null 2>&1; then git remote add origin "$REMOTE_URL"; fi
# Validate both fetch and push destinations, including explicit pushurl settings.
for direction in fetch push; do
  REMOTE_ARGS=(remote get-url --all origin)
  [[ "$direction" != push ]] || REMOTE_ARGS=(remote get-url --push --all origin)
  while IFS= read -r destination; do
    case "$destination" in
      "https://github.com/$REPOSITORY"|"https://github.com/$REPOSITORY.git"|"git@github.com:$REPOSITORY"|"git@github.com:$REPOSITORY.git"|"ssh://git@github.com/$REPOSITORY"|"ssh://git@github.com/$REPOSITORY.git") ;;
      *) die "origin 的 $direction 地址不是 ${REPOSITORY}，请自行核对 remote 配置。" ;;
    esac
  done < <(git "${REMOTE_ARGS[@]}")
done
git_remote fetch --prune origin
if ! git rev-parse --verify HEAD >/dev/null 2>&1; then
  BRANCH="$DEFAULT_BRANCH"
  git symbolic-ref HEAD "refs/heads/$BRANCH"
  if git show-ref --verify --quiet "refs/remotes/origin/$BRANCH"; then
    # Attach the existing history while preserving every existing local file.
    git reset --mixed "refs/remotes/origin/$BRANCH"
    while IFS= read -r -d '' tracked_file; do
      if [[ ! -e "$tracked_file" && ! -L "$tracked_file" ]]; then git checkout-index -- "$tracked_file"; fi
    done < <(git ls-files -z)
  fi
fi
BRANCH="$(git symbolic-ref --quiet --short HEAD)" || die '当前为 detached HEAD，请先切换到一个分支。'
git check-ignore --quiet .release/ || die '请将 .release/ 加入 .gitignore 后再发布。'
if ! $RESUME && git show-ref --verify --quiet "refs/remotes/origin/$BRANCH"; then
  git merge-base --is-ancestor "origin/$BRANCH" HEAD || die '远端分支包含尚未合并的提交，请先同步或解决分歧后再执行。'
fi

release_exists() { printf '%s\n' "$RELEASE_TAGS" | grep -Fqx -- "$1"; }
tag_exists() {
  git show-ref --verify --quiet "refs/tags/$1" || release_exists "$1" ||
    printf '%s\n' "$REMOTE_TAGS" | awk -v "ref=refs/tags/$1" '$2 == ref {found=1} END {exit !found}'
}
RELEASE_TAGS="$(gh api "repos/$REPOSITORY/releases?per_page=100" --paginate --jq '.[].tag_name')"
REMOTE_TAGS="$(git_remote ls-remote --tags --refs origin)"

if $RESUME; then
  STEP='校验重试产物'
  RELEASE_DIR="$OUTPUT_ROOT/$TAG"
  [[ -f "$RELEASE_DIR/manifest.json" ]] || die "缺少 $TAG 的本地产物清单，无法重试。"
  COMMIT="$(node -e 'const m=require(process.argv[1]); if(m.repository!==process.argv[2] || m.tag!==process.argv[3] || !/^[0-9a-f]{40,64}$/.test(m.commit)) process.exit(1); console.log(m.commit)' "$RELEASE_DIR/manifest.json" "$REPOSITORY" "$TAG")"
  [[ "$(node -pe 'require(process.argv[1]).branch' "$RELEASE_DIR/manifest.json")" == "$BRANCH" ]] || die '请切回发布时使用的分支后重试。'
  if [[ "$(node -pe 'require(process.argv[1]).draft === true' "$RELEASE_DIR/manifest.json")" == true ]]; then DRAFT=true; fi
  if [[ -z "$NOTES_FILE" && -f "$RELEASE_DIR/RELEASE_NOTES.md" ]]; then NOTES_FILE="$RELEASE_DIR/RELEASE_NOTES.md"; fi
  [[ "$(git rev-parse HEAD)" == "$COMMIT" ]] || die '当前 HEAD 与该安装包的提交不同，请切回对应提交所属分支后重试。'
  [[ -z "$(git status --porcelain --untracked-files=normal)" ]] || die '重试前请先处理工作区更改。'
  while IFS= read -r asset; do
    [[ "$asset" != */* && "$asset" != .* && -f "$RELEASE_DIR/$asset" ]] || die '产物清单包含无效文件。'
    ASSETS+=("$RELEASE_DIR/$asset")
  done < <(node -e 'for (const a of require(process.argv[1]).assets) console.log(a)' "$RELEASE_DIR/manifest.json")
  [[ ${#ASSETS[@]} -gt 0 ]] || die '没有可上传的安装包。'
  if command -v shasum >/dev/null 2>&1; then
    (cd "$RELEASE_DIR" && shasum -a 256 -c SHA256SUMS)
  else
    (cd "$RELEASE_DIR" && sha256sum -c SHA256SUMS)
  fi
else
  STEP='选择并同步版本'
  # An explicitly requested version must never silently become a different release.
  if $VERSION_EXPLICIT; then
    if tag_exists "$TAG"; then die "$TAG 已存在，请选择新版本，或使用 --resume $TAG 重试。"; fi
  else
    while tag_exists "$TAG"; do
      VERSION="$(node -e 'const v=process.argv[1].split("."); v[2]=String(Number(v[2])+1); console.log(v.join("."))' "$VERSION")"
      TAG="v$VERSION"
    done
  fi
  log "发布 $REPOSITORY / $BRANCH / $TAG"
  node - "$VERSION" <<'NODE'
const fs = require('fs');
const version = process.argv[2];
const changes = [];
function json(file, update) {
  const value = JSON.parse(fs.readFileSync(file, 'utf8'));
  update(value);
  changes.push([file, JSON.stringify(value, null, 2) + '\n']);
}
json('package.json', v => { v.version = version; });
json('package-lock.json', v => { v.version = version; v.packages[''].version = version; });
json('src-tauri/tauri.conf.json', v => { v.version = version; });
const cargoPath = 'src-tauri/Cargo.toml';
let cargo = fs.readFileSync(cargoPath, 'utf8');
let packageFound = false;
cargo = cargo.replace(/(\[package\][\s\S]*?)(?=\n\[|$)/, block => {
  if (!/^version = "[^"]+"$/m.test(block)) throw new Error('Cargo.toml 缺少 package.version');
  packageFound = true;
  return block.replace(/^version = "[^"]+"$/m, `version = "${version}"`);
});
if (!packageFound) throw new Error('Cargo.toml 缺少 [package]');
changes.push([cargoPath, cargo]);
const lockPath = 'src-tauri/Cargo.lock';
const lock = fs.readFileSync(lockPath, 'utf8');
const pattern = /(\[\[package\]\]\r?\nname = "work-report"\r?\nversion = ")[^"]+("[\s\S]*?)(?=\n\[\[package\]\]|$)/;
if (!pattern.test(lock)) throw new Error('Cargo.lock 缺少 work-report 包');
changes.push([lockPath, lock.replace(pattern, (_, prefix, rest) => prefix + version + rest)]);
for (const [file, text] of changes) fs.writeFileSync(file, text);
NODE
  STEP='安装依赖并提交代码'
  npm ci
  git add --all
  if ! git diff --cached --quiet; then git commit -m "${MESSAGE:-chore: release $TAG}"; fi
  COMMIT="$(git rev-parse HEAD)"
  [[ -z "$(git status --porcelain --untracked-files=normal)" ]] || die '提交后仍有未提交更改，请检查 Git hooks 或并发编辑。'
  STEP='测试'
  npm test
  cargo test --locked --manifest-path src-tauri/Cargo.toml
  RELEASE_DIR="$OUTPUT_ROOT/$TAG"
  mkdir -p "$RELEASE_DIR"
  BUILD_MARKER="$RELEASE_DIR/.build-start"
  touch "$BUILD_MARKER"
  STEP='构建并打包'
  # Use a known native-host output path, regardless of caller Cargo overrides.
  env -u CARGO_TARGET_DIR -u CARGO_BUILD_TARGET npm run tauri -- build --ci --bundles "$BUNDLES" -- --locked
  BUNDLE_DIR="$PROJECT_ROOT/src-tauri/target/release/bundle"
  if [[ "$PLATFORM" == macos ]]; then
    APP="$BUNDLE_DIR/macos/WorkReport.app"
    [[ -d "$APP" ]] || die '没有生成 WorkReport.app。'
    APP_VERSION="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP/Contents/Info.plist")"
    [[ "$APP_VERSION" == "$VERSION" ]] || die '安装包版本与发布版本不一致。'
    ASSET="$RELEASE_DIR/WorkReport_${TAG}_macos_$(uname -m).app.zip"
    ditto -c -k --sequesterRsrc --keepParent "$APP" "$ASSET"
    ASSETS+=("$ASSET")
  else
    shopt -s nullglob
    for artifact in "$BUNDLE_DIR"/deb/*.deb "$BUNDLE_DIR"/appimage/*.AppImage "$BUNDLE_DIR"/nsis/*.exe "$BUNDLE_DIR"/msi/*.msi; do
      # Bash 3.2 compares mtimes at whole-second precision; use Node for short builds.
      if node -e 'const fs=require("fs"); process.exit(fs.statSync(process.argv[1]).mtimeMs > fs.statSync(process.argv[2]).mtimeMs ? 0 : 1)' "$artifact" "$BUILD_MARKER"; then
        cp "$artifact" "$RELEASE_DIR/"
        ASSETS+=("$RELEASE_DIR/$(basename -- "$artifact")")
      fi
    done
    [[ ${#ASSETS[@]} -gt 0 ]] || die '本次构建没有生成可上传的安装包。'
  fi
  STEP='生成校验和与发布清单'
  ASSET_NAMES=()
  for asset in "${ASSETS[@]}"; do ASSET_NAMES+=("$(basename -- "$asset")"); done
  if command -v shasum >/dev/null 2>&1; then
    (cd "$RELEASE_DIR" && shasum -a 256 "${ASSET_NAMES[@]}") > "$RELEASE_DIR/SHA256SUMS"
  else
    (cd "$RELEASE_DIR" && sha256sum "${ASSET_NAMES[@]}") > "$RELEASE_DIR/SHA256SUMS"
  fi
  [[ -z "$(git status --porcelain --untracked-files=normal)" ]] || die '测试或构建修改了源码/锁文件，请检查后重新执行；尚未推送。'
  if [[ -n "$NOTES_FILE" && "$NOTES_FILE" != "$RELEASE_DIR/RELEASE_NOTES.md" ]]; then cp "$NOTES_FILE" "$RELEASE_DIR/RELEASE_NOTES.md"; NOTES_FILE="$RELEASE_DIR/RELEASE_NOTES.md"; fi
  node - "$RELEASE_DIR/manifest.json" "$REPOSITORY" "$TAG" "$COMMIT" "$BRANCH" "$DRAFT" "${ASSET_NAMES[@]}" <<'NODE'
const fs = require('fs');
const [file, repository, tag, commit, branch, draft, ...assets] = process.argv.slice(2);
fs.writeFileSync(file, JSON.stringify({ repository, tag, commit, branch, draft: draft === 'true', assets }, null, 2) + '\n');
NODE
fi
RESUME_READY=true

STEP='推送提交和标签'
if git show-ref --verify --quiet "refs/tags/$TAG"; then
  [[ "$(git rev-parse "$TAG^{commit}")" == "$COMMIT" ]] || die '本地标签指向其他提交，拒绝覆盖。'
else
  git tag -a "$TAG" -m "WorkReport $TAG" "$COMMIT"
fi
if $RESUME && git show-ref --verify --quiet "refs/remotes/origin/$BRANCH" && git merge-base --is-ancestor "$COMMIT" "origin/$BRANCH"; then
  git_remote push origin "refs/tags/$TAG:refs/tags/$TAG"
else
  git_remote push --atomic --set-upstream origin "HEAD:refs/heads/$BRANCH" "refs/tags/$TAG:refs/tags/$TAG"
fi

STEP='发布 GitHub Release'
if release_exists "$TAG"; then
  $RESUME || die "$TAG 的 Release 已被其他操作创建；可检查后使用 --resume。"
  IS_DRAFT="$(gh release view "$TAG" --repo "$REPOSITORY" --json isDraft --jq .isDraft)"
  if [[ "$IS_DRAFT" == true ]]; then
    gh release upload "$TAG" "${ASSETS[@]}" "$RELEASE_DIR/SHA256SUMS" --repo "$REPOSITORY" --clobber
    if ! $DRAFT; then gh release edit "$TAG" --repo "$REPOSITORY" --draft=false --verify-tag; fi
  else
    log '该 Release 已正式发布，保留现有附件。'
  fi
else
  RELEASE_ARGS=(release create "$TAG" "${ASSETS[@]}" "$RELEASE_DIR/SHA256SUMS" --repo "$REPOSITORY" --verify-tag --target "$COMMIT" --title "WorkReport $TAG")
  if [[ -n "$NOTES_FILE" ]]; then RELEASE_ARGS+=(--notes-file "$NOTES_FILE"); else RELEASE_ARGS+=(--generate-notes); fi
  if $DRAFT; then RELEASE_ARGS+=(--draft); fi
  gh "${RELEASE_ARGS[@]}"
fi
log "完成：https://github.com/$REPOSITORY/releases/tag/$TAG"
printf '本地产物：%s\n提交：%s\n' "$RELEASE_DIR" "$COMMIT"
