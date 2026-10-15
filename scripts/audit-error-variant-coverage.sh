#!/usr/bin/env bash
# Fail if a workspace Error::* variant is never named in tests/ or contract
# unit tests. Complements cargo-llvm-cov region/function gates (issue #52):
# line coverage can stay high while an entire error arm is never taken.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

# Collect variant identifiers from `pub enum Error` blocks in contracts/.
mapfile -t variants < <(
  awk '
    /pub enum Error/ { in_enum=1; next }
    in_enum && /^}/ { in_enum=0; next }
    in_enum && $0 ~ /^[[:space:]]*[A-Z][A-Za-z0-9_]*[[:space:]]*(=|,|\{|$)/ {
      line=$0
      sub(/^[[:space:]]+/, "", line)
      sub(/[[:space:]].*$/, "", line)
      sub(/,$/, "", line)
      if (line != "" && line !~ /^\/\// && line !~ /^#/) print line
    }
  ' $(find contracts -name '*.rs' -print)
)

if [ "${#variants[@]}" -eq 0 ]; then
  echo "audit-error-variant-coverage: no Error variants found" >&2
  exit 1
fi

# Deduplicate while preserving order.
declare -A seen=()
unique=()
for v in "${variants[@]}"; do
  if [ -z "${seen[$v]:-}" ]; then
    seen[$v]=1
    unique+=("$v")
  fi
done

fail=0
# Only count references inside the test corpus so module docs naming a dead
# variant cannot satisfy the check (the *NotFound example in issue #52).
mapfile -t test_files < <(
  find tests contracts -type f \( -name '*test*.rs' -o -path '*/tests/*.rs' -o -name 'test.rs' \) -print
)
if [ "${#test_files[@]}" -eq 0 ]; then
  echo "audit-error-variant-coverage: no test files found" >&2
  exit 1
fi

for v in "${unique[@]}"; do
  if ! grep -n -F -- "$v" "${test_files[@]}" >/dev/null 2>&1; then
    echo "::error::Error variant '${v}' is never referenced in the test corpus — add a test that triggers it (issue #52)"
    fail=1
  fi
done

if [ "$fail" -ne 0 ]; then
  echo "audit-error-variant-coverage: missing test references" >&2
  exit 1
fi

echo "audit-error-variant-coverage: ${#unique[@]} Error variants each appear in the test corpus"
