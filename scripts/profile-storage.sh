#!/usr/bin/env bash
set -euo pipefail

# Profile storage key counts per contract by counting DataKey variants in source.
# Compares against .storage-baseline.txt and fails if any contract exceeds its baseline.

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BASELINE="$REPO_ROOT/.storage-baseline.txt"
PROFILE_OUT="$REPO_ROOT/storage-profile.txt"

if [ ! -f "$BASELINE" ]; then
    echo "ERROR: Baseline file not found at $BASELINE"
    exit 1
fi

echo "# Storage profile — $(date -u +%Y-%m-%dT%H:%M:%SZ)" > "$PROFILE_OUT"
echo "# contract_name storage_key_count" >> "$PROFILE_OUT"

EXIT_CODE=0

for contract_dir in "$REPO_ROOT"/contracts/*/; do
    contract_name=$(basename "$contract_dir")
    lib_file="$contract_dir/src/lib.rs"

    if [ ! -f "$lib_file" ]; then
        continue
    fi

    # Count DataKey variants (lines between "enum DataKey {" and the closing "}")
    key_count=$(sed -n '/enum DataKey/,/^}/p' "$lib_file" | grep -cE '^\s+\w+' || true)

    echo "$contract_name $key_count" >> "$PROFILE_OUT"

    # Check against baseline
    baseline_count=$(grep "^$contract_name " "$BASELINE" | awk '{print $2}' || true)
    if [ -n "$baseline_count" ]; then
        if [ "$key_count" -gt "$baseline_count" ]; then
            echo "REGRESSION: $contract_name storage keys increased from $baseline_count to $key_count"
            EXIT_CODE=1
        else
            echo "OK: $contract_name storage keys: $key_count (baseline: $baseline_count)"
        fi
    else
        echo "NEW: $contract_name storage keys: $key_count (no baseline entry)"
    fi
done

echo ""
echo "Storage profile written to $PROFILE_OUT"

if [ "$EXIT_CODE" -ne 0 ]; then
    echo ""
    echo "FAILED: Storage regression detected. Update .storage-baseline.txt if the increase is intentional."
fi

exit $EXIT_CODE
