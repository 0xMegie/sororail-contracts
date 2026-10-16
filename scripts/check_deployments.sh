#!/usr/bin/env bash
set -euo pipefail

deployment_file="${1:-DEPLOYMENTS.md}"
fail=0

for contract_dir in contracts/*/; do
  contract_file="${contract_dir}src/contract.rs"
  [ -f "$contract_file" ] || continue

  contract="${contract_dir%/}"
  contract="${contract##*/}"
  if ! grep -Fq "| \`$contract\`" "$deployment_file"; then
    echo "::error file=${deployment_file}::deployable contract '${contract}' is missing from DEPLOYMENTS.md"
    fail=1
  fi
done

exit "$fail"
