#!/usr/bin/env sh
set -eu

failures=0

fail() {
  printf '%s\n' "docs check: $*" >&2
  failures=$((failures + 1))
}

require_file() {
  test -f "$1" || fail "missing required file: $1"
}

require_dir() {
  test -d "$1" || fail "missing required directory: $1"
}

require_file AGENTS.md
require_file ARCHITECTURE.md
require_file docs/index.md
require_file docs/long-term-roadmap.md
require_file docs/exec-plans/index.md
require_dir docs/exec-plans/active
require_dir docs/exec-plans/completed

grep -Fq 'exec-plans/index.md' docs/index.md ||
  fail "docs/index.md must link docs/exec-plans/index.md"
grep -Fq 'active/' docs/exec-plans/index.md ||
  fail "docs/exec-plans/index.md must link active execution plans"
grep -Fq 'completed/' docs/exec-plans/index.md ||
  fail "docs/exec-plans/index.md must link completed execution plans"

if find docs/exec-plans/active -mindepth 1 -maxdepth 1 -type d | grep -q .; then
  fail "active execution plans must be markdown files directly under docs/exec-plans/active"
fi

active_plan_count=0
for path in docs/exec-plans/active/*.md; do
  test -e "$path" || continue

  name=${path##*/}
  case "$name" in
    README.md)
      continue
      ;;
    *workpad* | *Workpad* | *generated* | *Generated*)
      fail "generated/workpad artifact is not allowed in active execution plans: $path"
      continue
      ;;
    plan-[0-9][0-9][0-9]*.md)
      active_plan_count=$((active_plan_count + 1))
      ;;
    *)
      fail "active execution plan must use plan-NNN naming: $path"
      ;;
  esac
done

if [ "$active_plan_count" -eq 0 ]; then
  if ! grep -Eq 'No active execution plans[.]' docs/exec-plans/active/README.md; then
    fail "active README must say 'No active execution plans.' when there are no active plans"
  fi
else
  if grep -Eq 'No active execution plans[.]' docs/exec-plans/active/README.md; then
    fail "active README says there are no active plans while active plan files exist"
  fi

  for path in docs/exec-plans/active/plan-[0-9][0-9][0-9]*.md; do
    test -e "$path" || continue
    name=${path##*/}
    if ! grep -Fq "$name" docs/exec-plans/active/README.md; then
      fail "active README does not list active plan: $name"
    fi
  done
fi

if find docs/exec-plans/completed -mindepth 1 -maxdepth 1 -type f ! -name '*.md' | grep -q .; then
  fail "completed execution-plan records must be markdown files"
fi

removed_tracker_refs=$(
  {
    find docs -path docs/exec-plans/completed -prune -o -type f -name '*.md' -print
    find scripts .github -type f \( -name '*.sh' -o -name '*.yml' -o -name '*.yaml' -o -name '*.md' \) -print
  } | xargs grep -nE 'docs/exec-plans/tech-debt-tracker[.]md' 2>/dev/null || true
)

if [ -n "$removed_tracker_refs" ]; then
  printf '%s\n' "$removed_tracker_refs" >&2
  fail "current docs/scripts/workflows must not reference the removed tech debt tracker path"
fi

if [ "$failures" -ne 0 ]; then
  exit 1
fi
