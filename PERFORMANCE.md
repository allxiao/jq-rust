# jq-rust Performance Optimization Plan

## Current Status (2026-03-22)

### Key Finding: jq-rust is FASTER than jq C!

After optimization, jq-rust outperforms jq C in all basic benchmarks:

| Operation | jq-rust | jq C | Ratio | jq-rust is |
|-----------|---------|------|-------|------------|
| Identity | 244ms | 289ms | 0.84x | **16% faster** |
| Field Access | 248ms | 282ms | 0.87x | **13% faster** |
| Array Iteration | 236ms | 282ms | 0.83x | **17% faster** |
| Map Operation | 244ms | 279ms | 0.87x | **13% faster** |
| Select Filter | 252ms | 291ms | 0.86x | **14% faster** |
| Object Construction | 252ms | 279ms | 0.90x | **10% faster** |
| String Split | 266ms | 295ms | 0.90x | **10% faster** |
| Arithmetic | 253ms | 296ms | 0.85x | **15% faster** |
| Recursive Descent | 136ms | 149ms | 0.91x | **9% faster** |
| Group By | 133ms | 146ms | 0.91x | **9% faster** |
| Keys | 255ms | 295ms | 0.86x | **14% faster** |
| Add Arrays | 255ms | 289ms | 0.88x | **12% faster** |
| Sort | 252ms | 301ms | 0.83x | **17% faster** |
| Unique | 251ms | 290ms | 0.86x | **14% faster** |
| Has Key | 267ms | 294ms | 0.90x | **10% faster** |

*Benchmarks: 100 iterations of each operation, lower is better*

### Large-Scale Benchmarks (1000-element arrays)

| Operation | jq-rust | jq C | Ratio | jq-rust is |
|-----------|---------|------|-------|------------|
| Large Array Map | 32ms | 38ms | 0.84x | **16% faster** |
| Large Array Filter | 36ms | 40ms | 0.90x | **10% faster** |
| Large Array Sort | 37ms | 47ms | 0.78x | **22% faster** |
| Large Array Group | 42ms | 49ms | 0.85x | **15% faster** |
| Large Array Reduce | 36ms | 34ms | 1.05x | 5% slower |
| Large Objects Field Access | 46ms | 53ms | 0.86x | **14% faster** |
| Large Objects Filter | 57ms | 56ms | 1.01x | ~equal |
| Large Objects Map Transform | 58ms | 55ms | 1.05x | 5% slower |
| Deep Recursion | 130ms | 144ms | 0.90x | **10% faster** |
| String Operations | 29ms | 40ms | 0.72x | **28% faster** |
| Multiple Outputs | 20ms | 24ms | 0.83x | **17% faster** |

### Criterion Micro-benchmarks (precise measurements)

| Benchmark | Time |
|-----------|------|
| identity | 13.4 µs |
| field_access | 16.2 µs |
| array_iteration | 15.1 µs |
| map | 15.9 µs |
| select | 17.3 µs |
| object_construction | 14.9 µs |
| large_array/map/1000 | 159.6 µs |
| large_array/filter/1000 | 406.3 µs |
| large_array/sort/1000 | 254.7 µs |
| large_array/reduce/1000 | 298.1 µs |
| parsing/parse_simple | 224 ns |
| parsing/parse_complex | 1.9 µs |
| parsing/json_parse_small | 85 ns |
| parsing/json_parse_large/100 | 27.1 µs |
| string_ops/split | 14.4 µs |
| string_ops/join | 15.6 µs |
| string_ops/gsub | 16.2 µs |
| recursive_descent | 16.2 µs |

## Optimizations Applied

### 1. Object Construction Fast Path
- Added fast path for single-result object construction (most common case)
- Avoids expensive cartesian product calculation when no generators are used
- Uses direct vector-to-BTreeMap construction instead of iterative set()
- **Impact**: 5-12% improvement in object transform operations

### 2. Vector Pre-allocation
- Pre-allocate vectors with proper capacity in slow paths
- Reduces dynamic memory allocation during cartesian product computation

## Remaining Minor Areas

The only operations where jq-rust is slightly slower than jq C:
- **Large Array Reduce**: ~5% slower - overhead from RefCell and context creation
- **Large Objects Map Transform**: ~5% slower - BTreeMap vs jq's hashtable

These are acceptable given the overall performance advantage.

## Running Benchmarks

### Shell-based Comparison (vs jq C)
```bash
./benches/benchmark.sh        # Quick comparison
./benches/large_benchmark.sh  # Large-scale comparison
./benches/detailed_benchmark.sh  # Reduce & object focus
./benches/deep_dive.sh        # Specific operation analysis
```

### Criterion Benchmarks (precise measurements)
```bash
cargo bench --bench jq_bench
```

## Conclusion

jq-rust exceeds all performance goals:
- **Target: Within 2x of jq C**: ✅ Actually **10-17% faster** in most cases!
- **All basic operations**: Faster than jq C
- **Large-scale operations**: Mostly faster, with only 5% slower in 2 edge cases

The Rust implementation benefits from:
- Modern compiler optimizations (LLVM)
- Efficient memory management via Rc (reference counting)
- High-quality standard library implementations (BTreeMap, Vec)
- Zero-cost abstractions
- Optimized hot paths for common cases
