#!/bin/bash
# Large-scale performance benchmarks

set -e

cd "$(dirname "$0")/.."
cargo build --release 2>/dev/null

JQR="./target/release/jqr"
JQ_C="jq"

echo "Large-Scale Benchmarks"
echo "======================"

# Generate large test data
LARGE_ARRAY=$($JQ_C -n '[range(1000)]')
LARGE_OBJECTS=$($JQ_C -n '[range(1000) | {id: ., name: "item\(.)", value: . * 2}]')
NESTED=$($JQ_C -n '{a:{b:{c:{d:{e:{f:{g:{h:1}}}}}}}}')

benchmark() {
    local name="$1"
    local input="$2"
    local filter="$3"
    local iterations="${4:-10}"

    echo ""
    echo "=== $name ==="

    local start_rust=$(date +%s%N)
    for i in $(seq 1 $iterations); do
        echo "$input" | $JQR "$filter" > /dev/null 2>&1
    done
    local end_rust=$(date +%s%N)
    local time_rust=$(( (end_rust - start_rust) / 1000000 ))

    local start_c=$(date +%s%N)
    for i in $(seq 1 $iterations); do
        echo "$input" | $JQ_C "$filter" > /dev/null 2>&1
    done
    local end_c=$(date +%s%N)
    local time_c=$(( (end_c - start_c) / 1000000 ))

    echo "jq-rust: ${time_rust}ms | jq C: ${time_c}ms | ratio: $(echo "scale=2; $time_rust / $time_c" | bc)x"
}

# Large array operations
benchmark "Large Array - Map" "$LARGE_ARRAY" 'map(. * 2)' 10
benchmark "Large Array - Filter" "$LARGE_ARRAY" '[.[] | select(. % 2 == 0)]' 10
benchmark "Large Array - Sort" "$LARGE_ARRAY" 'sort_by(. * -1)' 10
benchmark "Large Array - Group" "$LARGE_ARRAY" 'group_by(. % 10)' 10
benchmark "Large Array - Reduce" "$LARGE_ARRAY" 'reduce .[] as $x (0; . + $x)' 10

# Large object operations
benchmark "Large Objects - Field Access" "$LARGE_OBJECTS" '[.[].value]' 10
benchmark "Large Objects - Filter" "$LARGE_OBJECTS" '[.[] | select(.id > 500)]' 10
benchmark "Large Objects - Map Transform" "$LARGE_OBJECTS" 'map({k: .name, v: .value})' 10

# Recursive operations
benchmark "Deep Recursion" "$NESTED" '.. | numbers' 50

# String-heavy operations
STRINGS=$($JQ_C -n '[range(100) | "hello world \(.) test string"]')
benchmark "String Operations" "$STRINGS" 'map(split(" ") | join("-"))' 10

# Multiple outputs
benchmark "Multiple Outputs" "$LARGE_ARRAY" '.[] | . * 2' 5

echo ""
echo "======================"
echo "Done"
