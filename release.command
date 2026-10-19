#!/bin/bash
# Finder double-click entry point for macOS.
set -u
release_directory="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
release_status=0
"$release_directory/release.sh" "$@" || release_status=$?
if [[ -t 0 ]]; then
  printf '\n'
  read -r -p '按回车关闭窗口…' || true
fi
exit "$release_status"
