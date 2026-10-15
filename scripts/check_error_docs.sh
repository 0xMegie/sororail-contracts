#!/usr/bin/env bash
# Checks that docs/ERRORS.md lists exactly the same (variant, code) pairs as
# contracts/common/src/errors.rs. Run in CI so the two cannot drift apart.
set -euo pipefail

cd "$(dirname "$0")/.."

ERRORS_RS="contracts/common/src/errors.rs"
ERRORS_MD="docs/ERRORS.md"

# Pairs from the Rust enum: "Name 123"
rs_pairs=$(grep -oE '^\s*[A-Za-z][A-Za-z0-9]* = [0-9]+,' "$ERRORS_RS" \
    | sed -E 's/^\s*([A-Za-z0-9]+) = ([0-9]+),/\1 \2/' \
    | sort)

# Pairs from the docs table: "| 123 | `Name` | ... |" -> "Name 123"
md_pairs=$(grep -oE '^\| [0-9]+ \| `[A-Za-z0-9]+`' "$ERRORS_MD" \
    | sed -E 's/^\| ([0-9]+) \| `([A-Za-z0-9]+)`/\2 \1/' \
    | sort)

if [ "$rs_pairs" != "$md_pairs" ]; then
  echo "error: $ERRORS_MD is out of sync with $ERRORS_RS" >&2
  echo "--- in errors.rs but not (or mismatched) in docs/ERRORS.md ---" >&2
  comm -23 <(echo "$rs_pairs") <(echo "$md_pairs") >&2
  echo "--- in docs/ERRORS.md but not (or mismatched) in errors.rs ---" >&2
  comm -13 <(echo "$rs_pairs") <(echo "$md_pairs") >&2
  exit 1
fi

echo "ok: $ERRORS_MD matches $ERRORS_RS ($(echo "$rs_pairs" | wc -l) codes)"
