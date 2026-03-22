#!/bin/bash
# Deep dive into specific slow operations

set -e

cd "$(dirname "$0")/.."
cargo build --release 2>/dev/null

JQR="./target/release/jqr"
JQ_C="jq"

echo "Deep Dive: Slow Operations Analysis"
echo "===================================="

benchmark() {
    local name="$1"
    local input="$2"
    local filter="$3"
    local iterations="${4:-20}"

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

# Generate test data
LARGE_OBJECTS=$($JQ_C -n '[range(1000) | {id: ., name: "item\(.)", value: . * 2}]')

echo ""
echo "### LARGE OBJECT MAP TRANSFORM VARIANTS ###"

# Break down the map transform
benchmark "Simple field access" "$LARGE_OBJECTS" '[.[].id]' 20
benchmark "Simple field access 2" "$LARGE_OBJECTS" '[.[].value]' 20
benchmark "Map with identity" "$LARGE_OBJECTS" 'map(.)' 20
benchmark "Map with field" "$LARGE_OBJECTS" 'map(.id)' 20
benchmark "Map with two fields" "$LARGE_OBJECTS" 'map(.id, .value)' 10
benchmark "Map with object {a:.id}" "$LARGE_OBJECTS" 'map({a: .id})' 20
benchmark "Map with object {a:.id,b:.value}" "$LARGE_OBJECTS" 'map({a: .id, b: .value})' 20
benchmark "Map with object {x:.id,y:.value}" "$LARGE_OBJECTS" 'map({x: .id, y: .value})' 20

echo ""
echo "### MULTIPLE OUTPUTS VARIANTS ###"

ARRAY_1000=$($JQ_C -n '[range(1000)]')

benchmark "Simple iteration .[]" "$ARRAY_1000" '.[]' 10
benchmark "Iteration with multiply" "$ARRAY_1000" '.[] | . * 2' 10
benchmark "Iteration with add" "$ARRAY_1000" '.[] | . + 1' 10
benchmark "Iteration to array [.[]]" "$ARRAY_1000" '[.[]]' 10
benchmark "Map equivalent" "$ARRAY_1000" 'map(. * 2)' 10

echo ""
echo "### OBJECT FIELD ACCESS PATTERNS ###"

benchmark "Single field" "$LARGE_OBJECTS" '.[500].id' 50
benchmark "Multiple fields same object" "$LARGE_OBJECTS" '.[500] | .id, .name, .value' 50
benchmark "has() check" "$LARGE_OBJECTS" '[.[] | has("id")]' 10
benchmark "keys check" "$LARGE_OBJECTS" '.[0] | keys' 50
benchmark "getpath" "$LARGE_OBJECTS" '.[500] | getpath(["id"])' 50

echo ""
echo "### ITERATOR OVERHEAD ###"

benchmark "Empty filter" 'null' '.' 100
benchmark "Collect 1000 from range" 'null' '[range(1000)]' 20
benchmark "Generate and collect" 'null' '[range(1000) | . * 2]' 20

echo ""
echo "===================================="
echo "Done"
