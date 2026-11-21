#!/usr/bin/env bash
# 独立临时库由 cargo 测试自己创建。不读取现用案件库。
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
echo "git_rev $(git rev-parse HEAD)"
echo "git_rev_parse_exit $?"

run() {
  local name="$1"
  shift
  echo "CMD $name: $*"
  "$@"
  local ec=$?
  echo "EXIT $name $ec"
  return "$ec"
}

ec=0
run core_lib cargo test -p core --lib -- --test-threads=1 || ec=$?
run desktop_lib cargo test -p lurus-desktop --lib -- --test-threads=1 || ec=$?
run legal_loop cargo test -p lurus-desktop --test legal_loop -- --test-threads=1 --nocapture || ec=$?
run npm_build npm --prefix apps/desktop run build || ec=$?
echo "git_rev_after $(git rev-parse HEAD)"
echo "SUMMARY core_and_desktop_and_loop_and_npm exit=$ec"
exit "$ec"
