#!/usr/bin/env bash
set -euo pipefail

# Validate cross-contract dependency graph.
# Parses Cargo.toml for workspace crate deps and scans for env.invoke_contract
# calls to build a runtime dependency graph. Fails on circular dependencies.

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
EXIT_CODE=0

echo "=== Cross-Contract Dependency Graph ==="
echo ""

declare -A CARGO_DEPS
declare -A RUNTIME_DEPS
ALL_CONTRACTS=()

# Collect all contract names
for contract_dir in "$REPO_ROOT"/contracts/*/; do
    name=$(basename "$contract_dir")
    ALL_CONTRACTS+=("$name")
done

# Parse Cargo.toml for workspace crate dependencies
for contract_dir in "$REPO_ROOT"/contracts/*/; do
    name=$(basename "$contract_dir")
    cargo_file="$contract_dir/Cargo.toml"
    deps=""

    for other in "${ALL_CONTRACTS[@]}"; do
        if [ "$other" != "$name" ]; then
            if grep -q "\"$other\"" "$cargo_file" 2>/dev/null || \
               grep -q "path.*$other" "$cargo_file" 2>/dev/null; then
                deps="$deps $other"
            fi
        fi
    done

    CARGO_DEPS[$name]="${deps# }"
done

# Scan for runtime cross-contract invocations (invoke_contract / try_invoke_contract)
for contract_dir in "$REPO_ROOT"/contracts/*/; do
    name=$(basename "$contract_dir")
    lib_file="$contract_dir/src/lib.rs"
    runtime_deps=""

    if [ -f "$lib_file" ]; then
        if grep -qE 'invoke_contract|try_invoke_contract' "$lib_file" 2>/dev/null; then
            runtime_deps="(uses cross-contract invocation)"
        fi
    fi

    RUNTIME_DEPS[$name]="${runtime_deps}"
done

# Print dependency graph
echo "Cargo Dependencies:"
for name in "${ALL_CONTRACTS[@]}"; do
    deps="${CARGO_DEPS[$name]:-none}"
    if [ -z "$deps" ]; then
        deps="none"
    fi
    echo "  $name -> $deps"
done

echo ""
echo "Runtime Cross-Contract Calls:"
for name in "${ALL_CONTRACTS[@]}"; do
    runtime="${RUNTIME_DEPS[$name]:-none}"
    if [ -z "$runtime" ]; then
        runtime="none"
    fi
    echo "  $name: $runtime"
done

# Check for circular dependencies using simple DFS via topological sort
# Build adjacency list from Cargo deps
echo ""
echo "=== Circular Dependency Check ==="

# Use tsort to detect cycles
EDGES_FILE=$(mktemp)
HAS_EDGES=false

for name in "${ALL_CONTRACTS[@]}"; do
    deps="${CARGO_DEPS[$name]:-}"
    if [ -n "$deps" ]; then
        for dep in $deps; do
            echo "$name $dep" >> "$EDGES_FILE"
            HAS_EDGES=true
        done
    fi
done

if [ "$HAS_EDGES" = true ]; then
    if ! tsort "$EDGES_FILE" > /dev/null 2>&1; then
        echo "FAILED: Circular dependency detected!"
        echo "Cycle details:"
        tsort "$EDGES_FILE" 2>&1 || true
        EXIT_CODE=1
    else
        echo "No circular dependencies detected."
        echo ""
        echo "Valid build order:"
        tsort "$EDGES_FILE" | tac | while read -r line; do
            echo "  $line"
        done
    fi
else
    echo "No inter-crate dependencies found — no cycles possible."
fi

rm -f "$EDGES_FILE"

# Print documented deployment order for reference
echo ""
echo "=== Documented Deployment Order ==="
echo "  1. router-registry"
echo "  2. router-access"
echo "  3. router-middleware"
echo "  4. router-timelock"
echo "  5. router-multicall"
echo "  6. router-core"

if [ "$EXIT_CODE" -ne 0 ]; then
    echo ""
    echo "FAILED: Dependency validation failed."
fi

exit $EXIT_CODE
