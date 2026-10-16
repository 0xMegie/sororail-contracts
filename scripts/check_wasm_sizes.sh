#!/usr/bin/env bash
set -euo pipefail

wasm_dir="${1:-target/wasm32v1-none/release}"
max_bytes="${MAX_WASM_BYTES:-262144}"
shopt -s nullglob
files=("$wasm_dir"/sororail_*.wasm)

if [ "${#files[@]}" -eq 0 ]; then
  echo "error: no contract WASM files found in ${wasm_dir}" >&2
  exit 1
fi

fail=0
for file in "${files[@]}"; do
  size=$(stat -c%s "$file")
  name=$(basename "$file")
  echo "$name: $size bytes (limit: $max_bytes)"
  if [ "$size" -gt "$max_bytes" ]; then
    echo "::error file=$file::${name} is ${size} bytes, exceeds ${max_bytes} byte limit"
    fail=1
  fi
done

exit "$fail"
