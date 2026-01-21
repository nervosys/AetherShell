# AetherShell Performance Guide

This document describes how to benchmark and optimize AetherShell performance.

## Benchmark Suite

AetherShell includes comprehensive benchmarks using [Criterion](https://bheisler.github.io/criterion.rs/book/).

### Available Benchmarks

| Benchmark              | Description                                    |
| ---------------------- | ---------------------------------------------- |
| `parser_performance`   | Parser speed for various syntax constructs     |
| `eval_performance`     | Evaluator speed for expressions and statements |
| `pipeline_performance` | Pipeline execution with various operations     |
| `builtin_performance`  | Builtin function dispatch and execution        |
| `mcp_performance`      | MCP protocol operations                        |

### Running Benchmarks

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark
cargo bench --bench parser_performance

# Run with specific filter
cargo bench --bench pipeline_performance -- "pipeline_map"

# Quick test with small sample
cargo bench --bench parser_performance -- --sample-size 10
```

### Benchmark Results

Results are saved to `target/criterion/` with HTML reports. Open `target/criterion/report/index.html` in a browser.

## Parser Benchmarks

Tests parsing performance for:

| Group            | Tests                                                                                                 |
| ---------------- | ----------------------------------------------------------------------------------------------------- |
| `parser_simple`  | literal_int, literal_string, literal_array, arithmetic, comparison, assignment, function_call, lambda |
| `parser_complex` | pipeline_simple, pipeline_chain, record, match_simple, if_else, nested_lambdas                        |
| `parser_scaling` | array (10-500 elements), statements (5-50)                                                            |

## Evaluator Benchmarks

Tests execution performance for:

| Group               | Tests                                                                 |
| ------------------- | --------------------------------------------------------------------- |
| `eval_arithmetic`   | add_ints, multiply_ints, complex_arithmetic, float_arithmetic, modulo |
| `eval_strings`      | concat_short, string_repeat, string_builtin                           |
| `eval_arrays`       | array_create_small, array_index, array_len, range                     |
| `eval_records`      | record_create, record_field_access                                    |
| `eval_lambdas`      | lambda_create, lambda_call, lambda_nested                             |
| `eval_conditionals` | if_true, if_comparison, match_simple                                  |
| `eval_scaling`      | sum_chain (10-100), array_create (10-500)                             |

## Pipeline Benchmarks

Tests pipeline operation performance:

| Group                | Tests                                                      |
| -------------------- | ---------------------------------------------------------- |
| `pipeline_map`       | double with 10-1000 elements                               |
| `pipeline_filter`    | even filter with 10-1000 elements                          |
| `pipeline_reduce`    | sum with 10-1000 elements                                  |
| `pipeline_chain`     | map_filter, full_chain (4 ops) with 10-500 elements        |
| `pipeline_array_ops` | first, last, flatten, reverse, unique with 10-500 elements |
| `pipeline_sort`      | integers, sorted, reversed with 10-1000 elements           |
| `pipeline_any_all`   | any_early, any_late, any_none, all_true, all_false_early   |

## Optimization Tips

### Hot Paths

1. **Builtin Dispatch**: The `BUILTIN_DISPATCH` array uses direct indexing for O(1) lookup
2. **Parser**: Uses recursive descent with minimal backtracking
3. **Evaluator**: Direct pattern matching on AST nodes

### Memory Optimization

1. **Value Cloning**: Consider `Cow<T>` for large values passed through pipelines
2. **String Interning**: Frequently used strings (builtin names) could be interned
3. **Arena Allocation**: AST nodes could use arena allocation for batch parsing

### Startup Time

1. **Lazy Initialization**: Defer AI client creation until first use
2. **Config Caching**: Cache parsed config after first load
3. **Plugin Discovery**: Lazy-load plugin manifests

## Profiling

### Using flamegraph

```bash
# Install flamegraph
cargo install flamegraph

# Generate flamegraph
cargo flamegraph --bench pipeline_performance -- --bench
```

### Using perf (Linux)

```bash
cargo build --release
perf record target/release/ae -c "[1,2,3] | map(fn(x) => x * 2)"
perf report
```

### Using Instruments (macOS)

```bash
cargo build --release
instruments -t "Time Profiler" target/release/ae -c "..."
```

## Performance Targets

| Operation          | Target | Current   |
| ------------------ | ------ | --------- |
| Simple parse       | <1μs   | ~175ns ✅  |
| Integer add eval   | <500ns | ~300ns ✅  |
| Float arithmetic   | <1μs   | ~486ns ✅  |
| Complex arithmetic | <2μs   | ~1.05μs ✅ |
| 100-element map    | <100μs | ~64μs ✅   |
| 1000-element map   | <5ms   | ~3.1ms ✅  |
| Cold start         | <100ms | ~15ms ✅   |

## Contributing

When adding new features, please:

1. Add benchmarks for any new hot paths
2. Run `cargo bench` before and after changes
3. Document any performance regressions in PR description
4. Use `#[inline]` judiciously for small, frequently-called functions
