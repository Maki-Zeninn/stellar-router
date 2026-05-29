#!/bin/bash
# Helper script to run integration tests

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}=== Stellar Router Integration Tests ===${NC}\n"

# Check if stellar CLI is installed
if ! command -v stellar &> /dev/null; then
    echo -e "${RED}Error: stellar CLI not found${NC}"
    echo "Install with: cargo install --locked stellar-cli"
    exit 1
fi

echo -e "${GREEN}✓ Stellar CLI found:${NC} $(stellar --version)"

# Check if WASM contracts are built
WASM_DIR="target/wasm32-unknown-unknown/release"
CONTRACTS=(
    "router_core.wasm"
    "router_registry.wasm"
    "router_access.wasm"
    "router_middleware.wasm"
    "router_timelock.wasm"
    "router_multicall.wasm"
)

MISSING_CONTRACTS=()
for contract in "${CONTRACTS[@]}"; do
    if [ ! -f "$WASM_DIR/$contract" ]; then
        MISSING_CONTRACTS+=("$contract")
    fi
done

if [ ${#MISSING_CONTRACTS[@]} -ne 0 ]; then
    echo -e "${YELLOW}⚠ Missing WASM contracts:${NC}"
    for contract in "${MISSING_CONTRACTS[@]}"; do
        echo "  - $contract"
    done
    echo ""
    echo -e "${BLUE}Building WASM contracts...${NC}"
    cargo build --target wasm32-unknown-unknown --release
    echo -e "${GREEN}✓ WASM contracts built${NC}\n"
else
    echo -e "${GREEN}✓ All WASM contracts found${NC}\n"
fi

# Parse command line arguments
TEST_FILTER=""
VERBOSE="--nocapture"
THREADS="--test-threads=1"

while [[ $# -gt 0 ]]; do
    case $1 in
        --filter)
            TEST_FILTER="$2"
            shift 2
            ;;
        --quiet)
            VERBOSE=""
            shift
            ;;
        --parallel)
            THREADS=""
            shift
            ;;
        --help)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --filter <pattern>   Run only tests matching pattern"
            echo "  --quiet              Suppress test output"
            echo "  --parallel           Run tests in parallel (may hit rate limits)"
            echo "  --help               Show this help message"
            echo ""
            echo "Examples:"
            echo "  $0                                    # Run all tests"
            echo "  $0 --filter full_flow                 # Run only full_flow tests"
            echo "  $0 --filter test_full_router_core_flow # Run specific test"
            echo "  $0 --quiet                            # Run without output"
            exit 0
            ;;
        *)
            echo -e "${RED}Unknown option: $1${NC}"
            echo "Use --help for usage information"
            exit 1
            ;;
    esac
done

# Build test command
CMD="cargo test --test integration_tests"

if [ -n "$TEST_FILTER" ]; then
    CMD="$CMD $TEST_FILTER"
    echo -e "${BLUE}Running tests matching: ${YELLOW}$TEST_FILTER${NC}\n"
else
    echo -e "${BLUE}Running all integration tests${NC}\n"
fi

CMD="$CMD -- --ignored $THREADS $VERBOSE"

# Run tests
echo -e "${BLUE}Command:${NC} $CMD\n"
echo -e "${YELLOW}Note: Tests run against Stellar testnet and may take several minutes${NC}\n"

if eval $CMD; then
    echo -e "\n${GREEN}=== All tests passed! ===${NC}"
    exit 0
else
    echo -e "\n${RED}=== Some tests failed ===${NC}"
    exit 1
fi
