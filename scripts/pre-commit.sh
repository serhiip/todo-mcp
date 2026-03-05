#!/usr/bin/env bash
# Optional pre-commit hook: run the quality gate so commits stay warning-free.
# Install: cp scripts/pre-commit.sh .git/hooks/pre-commit && chmod +x .git/hooks/pre-commit
set -e
cd "$(git rev-parse --show-toplevel)"
./scripts/quality.sh
