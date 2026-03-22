#!/bin/bash
# Performance benchmark suite for jqr (jq-rust) vs jq C

set -e

# Build release version
echo "Building release..."
cd "$(dirname "$0")/.."
cargo build --release 2>/dev/null

JQR="./target/release/jqr"
JQ_C="jq"

# Check if jq C is available
if ! command -v jq &> /dev/null; then
    echo "Warning: jq C not found, benchmarks will be jq-rust only"
    JQ_C=""
fi

# Benchmark function
benchmark() {
    local name="$1"
    local input="$2"
    local filter="$3"
    local iterations="${4:-100}"

    echo ""
    echo "=== $name ==="
    echo "Filter: $filter"

    # Time jq-rust
    local start_rust=$(date +%s%N)
    for i in $(seq 1 $iterations); do
        echo "$input" | $JQR "$filter" > /dev/null 2>&1
    done
    local end_rust=$(date +%s%N)
    local time_rust=$(( (end_rust - start_rust) / 1000000 ))
    echo "jq-rust: ${time_rust}ms (${iterations} iterations)"

    # Time jq C if available
    if [ -n "$JQ_C" ]; then
        local start_c=$(date +%s%N)
        for i in $(seq 1 $iterations); do
            echo "$input" | $JQ_C "$filter" > /dev/null 2>&1
        done
        local end_c=$(date +%s%N)
        local time_c=$(( (end_c - start_c) / 1000000 ))
        echo "jq C:    ${time_c}ms (${iterations} iterations)"

        if [ $time_c -gt 0 ]; then
            local ratio=$(echo "scale=2; $time_rust / $time_c" | bc)
            echo "Ratio:   ${ratio}x slower"
        fi
    fi
}

echo "jq-rust Performance Benchmarks"
echo "=============================="

# Benchmark 1: Identity filter (startup overhead)
benchmark "Identity Filter" '{"a": 1}' '.'

# Benchmark 2: Field access
benchmark "Field Access" '{"a": {"b": {"c": 1}}}' '.a.b.c'

# Benchmark 3: Array iteration
INPUT_ARRAY='[1,2,3,4,5,6,7,8,9,10]'
benchmark "Array Iteration" "$INPUT_ARRAY" '.[]'

# Benchmark 4: Map operation
benchmark "Map Operation" "$INPUT_ARRAY" 'map(. * 2)'

# Benchmark 5: Select filter
benchmark "Select Filter" "$INPUT_ARRAY" '[.[] | select(. > 5)]'

# Benchmark 6: Object construction
benchmark "Object Construction" '{"name": "test", "value": 42}' '{n: .name, v: .value}'

# Benchmark 7: String operations
benchmark "String Split" '"hello world foo bar"' 'split(" ")'

# Benchmark 8: Arithmetic
benchmark "Arithmetic" '{"a": 10, "b": 20}' '.a + .b * 2'

# Benchmark 9: Recursive descent
benchmark "Recursive Descent" '{"a": {"b": {"c": {"d": 1}}}}' '.. | numbers' 50

# Benchmark 10: Group by (complex)
LARGE_ARRAY='[{"type":"a","v":1},{"type":"b","v":2},{"type":"a","v":3},{"type":"b","v":4},{"type":"c","v":5}]'
benchmark "Group By" "$LARGE_ARRAY" 'group_by(.type)' 50

# Benchmark 11: Keys and values
benchmark "Keys" '{"a":1,"b":2,"c":3}' 'keys'

# Benchmark 12: Add arrays
benchmark "Add Arrays" '[[1,2],[3,4],[5,6]]' 'add'

# Benchmark 13: Sort
benchmark "Sort" '[5,3,8,1,9,2,7,4,6]' 'sort'

# Benchmark 14: Unique
benchmark "Unique" '[1,2,1,3,2,4,3,5]' 'unique'

# Benchmark 15: Has key
benchmark "Has Key" '{"a":1,"b":2}' 'has("a")'

echo ""
echo "=============================="
echo "Benchmarks complete"
