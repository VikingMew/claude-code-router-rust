#!/usr/bin/env sh
set -eu

test -f AGENTS.md
test -f ARCHITECTURE.md
test -f docs/index.md
test -f docs/exec-plans/index.md
test -d docs/exec-plans/active
test -d docs/exec-plans/completed
