#!/bin/bash
# Detailed benchmarks for reduce and object transform operations

set -e

cd "$(dirname "$0")/.."
cargo build --release 2>/dev/null

JQ_RUST="./target/release/jq"
JQ_C="jq"

echo "Detailed Reduce & Object Transform Benchmarks"
echo "=============================================="

benchmark() {
    local name="$1"
    local input="$2"
    local filter="$3"
    local iterations="${4:-50}"

    echo ""
    echo "=== $name ==="
    echo "Filter: $filter"

    local start_rust=$(date +%s%N)
    for i in $(seq 1 $iterations); do
        echo "$input" | $JQ_RUST "$filter" > /dev/null 2>&1
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

# Generate test data
SMALL_ARRAY='[1,2,3,4,5,6,7,8,9,10]'
MEDIUM_ARRAY=$($JQ_C -n '[range(100)]')
LARGE_ARRAY=$($JQ_C -n '[range(1000)]')

echo ""
echo "### REDUCE OPERATIONS ###"

# Simple reduce - add
benchmark "Reduce Add (10 items)" "$SMALL_ARRAY" 'reduce .[] as $x (0; . + $x)' 100
benchmark "Reduce Add (100 items)" "$MEDIUM_ARRAY" 'reduce .[] as $x (0; . + $x)' 50
benchmark "Reduce Add (1000 items)" "$LARGE_ARRAY" 'reduce .[] as $x (0; . + $x)' 20

# Reduce with object building
benchmark "Reduce Build Object (10)" "$SMALL_ARRAY" 'reduce .[] as $x ({}; .[$x|tostring] = $x)' 100
benchmark "Reduce Build Object (100)" "$MEDIUM_ARRAY" 'reduce .[] as $x ({}; .[$x|tostring] = $x)' 50

# Reduce with array building
benchmark "Reduce Build Array (10)" "$SMALL_ARRAY" 'reduce .[] as $x ([]; . + [$x * 2])' 100
benchmark "Reduce Build Array (100)" "$MEDIUM_ARRAY" 'reduce .[] as $x ([]; . + [$x * 2])' 50

# Complex reduce
benchmark "Reduce with Pattern" "$SMALL_ARRAY" 'reduce .[] as $x ({sum:0,count:0}; .sum += $x | .count += 1)' 100

echo ""
echo "### OBJECT TRANSFORM OPERATIONS ###"

SMALL_OBJECTS='[{"a":1,"b":2},{"a":3,"b":4},{"a":5,"b":6}]'
MEDIUM_OBJECTS=$($JQ_C -n '[range(50) | {id: ., name: "item\(.)", value: . * 2}]')
LARGE_OBJECTS=$($JQ_C -n '[range(200) | {id: ., name: "item\(.)", value: . * 2}]')

# Simple object construction
benchmark "Object {k:v} (small)" "$SMALL_OBJECTS" 'map({x: .a, y: .b})' 100
benchmark "Object {k:v} (medium)" "$MEDIUM_OBJECTS" 'map({x: .id, y: .value})' 50
benchmark "Object {k:v} (large)" "$LARGE_OBJECTS" 'map({x: .id, y: .value})' 20

# Object with computed keys
benchmark "Object computed key (small)" "$SMALL_OBJECTS" 'map({(.a|tostring): .b})' 100
benchmark "Object computed key (medium)" "$MEDIUM_OBJECTS" 'map({(.id|tostring): .value})' 50

# Object merge
benchmark "Object merge" '{"a":1,"b":2}' '. + {c:3,d:4}' 100
benchmark "Object merge (larger)" '{"a":1,"b":2,"c":3,"d":4,"e":5}' '. + {f:6,g:7,h:8,i:9,j:10}' 100

# Object with multiple entries
benchmark "Object multi-entry" '{"x":1}' '{a:.x, b:.x+1, c:.x+2, d:.x+3}' 100

# to_entries/from_entries
benchmark "to_entries" '{"a":1,"b":2,"c":3,"d":4,"e":5}' 'to_entries' 100
benchmark "from_entries" '[{"key":"a","value":1},{"key":"b","value":2},{"key":"c","value":3}]' 'from_entries' 100
benchmark "with_entries" '{"a":1,"b":2,"c":3,"d":4,"e":5}' 'with_entries(.value += 1)' 100

echo ""
echo "### CONTEXT CREATION OVERHEAD ###"

# These test the overhead of creating child contexts (used in reduce, binding)
benchmark "Simple binding" "$SMALL_ARRAY" '.[] as $x | $x * 2' 100
benchmark "Nested binding" "$SMALL_ARRAY" '.[] as $x | ($x * 2) as $y | $y + $x' 100
benchmark "Multiple bindings" "$SMALL_ARRAY" '.[] as $x | .[] as $y | $x + $y' 20

echo ""
echo "=============================================="
echo "Done"
