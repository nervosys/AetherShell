# AetherShell Testing Strategy

This document outlines the comprehensive testing approach for AetherShell's syntax and function library.

## Overview

AetherShell testing is organized into **four layers**:

1. **Unit Tests** - Rust tests for individual components (`tests/*.rs`)
2. **Syntax Tests** - `.ae` scripts testing language constructs (`tests/coverage/*.ae`)
3. **Integration Tests** - End-to-end tests combining multiple features
4. **Coverage Tests** - Systematic tests ensuring all builtins and features are covered

---

## Quick Start

```bash
# Run all core tests (excludes AI/network tests)
cargo test --test builtins_coverage --test theme_coverage --test builtins --test eval --test pipeline --test smoke --test typecheck

# Run just the coverage tests
cargo test --test builtins_coverage --test theme_coverage

# Run a single .ae test file
.\target\debug\ae.exe tests/coverage/syntax_comprehensive.ae

# Run all .ae coverage tests (PowerShell)
Get-ChildItem tests/coverage/*.ae | ForEach-Object { .\target\debug\ae.exe $_.FullName }
```

---

## Known Parser Quirks

When writing `.ae` test files, be aware of these parser behaviors:

### Pipeline Assignment Followed by Another Assignment
Pipeline expressions assigned to variables must be parenthesized if followed by another assignment:

```aethershell
# ❌ FAILS - Parser error
x = [1,2,3] | reverse
y = first(x)

# ✅ WORKS - Parenthesized pipeline
x = ([1,2,3] | reverse)
y = first(x)
```

### Pipeline-Only Builtins
Some builtins only accept pipeline input, not function-call syntax:

```aethershell
# ❌ FAILS
flatten([[1],[2]])
any([1,2,3], fn(x) => x > 1)

# ✅ WORKS
[[1],[2]] | flatten
[1,2,3] | any(fn(x) => x > 1)
```

Pipeline-only builtins: `flatten`, `reverse`, `slice`, `any`, `all`

### Lambda Parameters
AetherShell requires at least one parameter for lambdas (no zero-param lambdas):

```aethershell
# ❌ FAILS
constant = fn() => 42

# ✅ WORKS - Use placeholder parameter
constant = fn(_) => 42
```

---

## Test Categories

### 1. Language Syntax Coverage

All AST constructs from `src/ast.rs` must be tested:

| Category          | Constructs                                                                    | Test File                                |
| ----------------- | ----------------------------------------------------------------------------- | ---------------------------------------- |
| **Literals**      | `Int`, `Float`, `String`, `Bool`, `Null`                                      | `tests/coverage/syntax_comprehensive.ae` |
| **Collections**   | `Array`, `Record`                                                             | `tests/coverage/syntax_comprehensive.ae` |
| **Operators**     | `+`, `-`, `*`, `/`, `%`, `**`, `==`, `!=`, `<`, `<=`, `>`, `>=`, `&&`, `\|\|` | `tests/coverage/syntax_comprehensive.ae` |
| **Unary**         | `-x`, `!x`                                                                    | `tests/coverage/syntax_comprehensive.ae` |
| **Lambdas**       | `fn(x) => expr`, multi-param                                                  | `tests/coverage/syntax_comprehensive.ae` |
| **Pipelines**     | `x \| f`, `x \| f(args)`                                                      | `tests/coverage/syntax_comprehensive.ae` |
| **Match**         | `match x { ... }`, guards, patterns                                           | `tests/coverage/syntax_comprehensive.ae` |
| **Variables**     | `let x = ...`, `let mut x = ...`                                              | `tests/coverage/syntax_comprehensive.ae` |
| **Member Access** | `record.field`                                                                | `tests/coverage/syntax_comprehensive.ae` |

### 2. Builtin Function Coverage

All **143+ builtins** organized by category:

#### Core (0-7)
| Index | Name       | Status |
| ----- | ---------- | ------ |
| 0     | `help`     | ✅      |
| 1     | `call`     | ✅      |
| 2     | `clear`    | ✅      |
| 3     | `echo`     | ✅      |
| 4     | `print`    | ✅      |
| 5     | `http_get` | ⬜      |
| 6     | `Some`     | ✅      |
| 7     | `None`     | ✅      |

#### File System (8-18)
| Index | Name          | Status |
| ----- | ------------- | ------ |
| 8     | `ls` / `list` | ✅      |
| 9     | `pwd`         | ✅      |
| 10    | `cat`         | ✅      |
| 11    | `read_text`   | ⬜      |
| 12    | `head`        | ✅      |
| 13    | `tail`        | ✅      |
| 14    | `find`        | ✅      |
| 15    | `sort`        | ✅      |
| 16    | `uniq`        | ✅      |
| 17    | `wc`          | ✅      |
| 18    | `grep`        | ✅      |

#### Functional (19-29)
| Index | Name                 | Status |
| ----- | -------------------- | ------ |
| 19    | `map`                | ✅      |
| 20    | `where`              | ✅      |
| 21    | `reduce`             | ✅      |
| 22    | `take`               | ✅      |
| 23    | `keys`               | ⬜      |
| 24    | `len` / `length`     | ✅      |
| 25    | `type_of` / `typeof` | ✅      |
| 26    | `first`              | ⬜      |
| 27    | `last`               | ⬜      |
| 28    | `any`                | ⬜      |
| 29    | `all`                | ⬜      |

#### MCP Detection (30-33)
| Index | Name               | Status |
| ----- | ------------------ | ------ |
| 30    | `mcp_servers`      | ⬜      |
| 31    | `mcp_detect`       | ⬜      |
| 32    | `mcp_cache_clear`  | ⬜      |
| 33    | `mcp_cache_status` | ⬜      |

#### String Operations (34-42)
| Index | Name          | Status |
| ----- | ------------- | ------ |
| 34    | `split`       | ⬜      |
| 35    | `join`        | ⬜      |
| 36    | `trim`        | ⬜      |
| 37    | `upper`       | ⬜      |
| 38    | `lower`       | ⬜      |
| 39    | `replace`     | ⬜      |
| 40    | `contains`    | ⬜      |
| 41    | `starts_with` | ⬜      |
| 42    | `ends_with`   | ⬜      |

#### Array Operations (43-49)
| Index | Name      | Status |
| ----- | --------- | ------ |
| 43    | `flatten` | ⬜      |
| 44    | `reverse` | ⬜      |
| 45    | `slice`   | ⬜      |
| 46    | `range`   | ⬜      |
| 47    | `zip`     | ⬜      |
| 48    | `push`    | ⬜      |
| 49    | `concat`  | ⬜      |

#### Math Operations (50-57)
| Index | Name    | Status |
| ----- | ------- | ------ |
| 50    | `abs`   | ⬜      |
| 51    | `min`   | ⬜      |
| 52    | `max`   | ⬜      |
| 53    | `floor` | ⬜      |
| 54    | `ceil`  | ⬜      |
| 55    | `round` | ⬜      |
| 56    | `sqrt`  | ⬜      |
| 57    | `pow`   | ⬜      |

#### System (58-64)
| Index | Name             | Status |
| ----- | ---------------- | ------ |
| 58    | `exit`           | ⬜      |
| 59    | `env`            | ⬜      |
| 60    | `set_env`        | ⬜      |
| 61    | `sleep`          | ⬜      |
| 62    | `time`           | ⬜      |
| 63    | `json_parse`     | ⬜      |
| 64    | `json_stringify` | ⬜      |

#### Syntax KB (65-71)
| Index | Name                | Status |
| ----- | ------------------- | ------ |
| 65    | `syntax_get`        | ⬜      |
| 66    | `syntax_list`       | ⬜      |
| 67    | `syntax_search`     | ⬜      |
| 68    | `syntax_add`        | ⬜      |
| 69    | `syntax_categories` | ⬜      |
| 70    | `ab_encode`         | ⬜      |
| 71    | `ab_decode`         | ⬜      |

#### Neural Network (72-82)
| Index | Name             | Status |
| ----- | ---------------- | ------ |
| 72    | `nn_create`      | ⬜      |
| 73    | `nn_forward`     | ⬜      |
| 74    | `nn_mutate`      | ⬜      |
| 75    | `nn_crossover`   | ⬜      |
| 76    | `nn_params`      | ⬜      |
| 77    | `nn_set_params`  | ⬜      |
| 78    | `nn_layers`      | ⬜      |
| 79    | `nn_info`        | ⬜      |
| 80    | `consensus_net`  | ⬜      |
| 81    | `consensus_vote` | ⬜      |
| 82    | `activation`     | ⬜      |

#### Evolution (83-92)
| Index | Name                 | Status |
| ----- | -------------------- | ------ |
| 83    | `population`         | ⬜      |
| 84    | `evolve`             | ⬜      |
| 85    | `evolve_step`        | ⬜      |
| 86    | `fitness`            | ⬜      |
| 87    | `best_individual`    | ⬜      |
| 88    | `evolution_stats`    | ⬜      |
| 89    | `selection_strategy` | ⬜      |
| 90    | `crossover_strategy` | ⬜      |
| 91    | `evolution_config`   | ⬜      |
| 92    | `coevolve`           | ⬜      |

#### Reinforcement Learning (93-107)
| Index | Name                | Status |
| ----- | ------------------- | ------ |
| 93    | `rl_agent`          | ⬜      |
| 94    | `rl_action`         | ⬜      |
| 95    | `rl_update`         | ⬜      |
| 96    | `rl_sarsa_agent`    | ⬜      |
| 97    | `rl_sarsa_update`   | ⬜      |
| 98    | `rl_pg_agent`       | ⬜      |
| 99    | `rl_pg_step`        | ⬜      |
| 100   | `rl_pg_episode_end` | ⬜      |
| 101   | `rl_ac_agent`       | ⬜      |
| 102   | `rl_ac_update`      | ⬜      |
| 103   | `rl_dqn_agent`      | ⬜      |
| 104   | `rl_dqn_step`       | ⬜      |
| 105   | `rl_replay_buffer`  | ⬜      |
| 106   | `rl_gridworld`      | ⬜      |
| 107   | `rl_env_step`       | ⬜      |

#### OS Tools (108-112)
| Index | Name                               | Status |
| ----- | ---------------------------------- | ------ |
| 108   | `tools` / `tool_list` / `os_tools` | ⬜      |
| 109   | `tool_info`                        | ⬜      |
| 110   | `tool_schema` / `tool_schemas`     | ⬜      |
| 111   | `tool_exec` / `tool_execute`       | ⬜      |
| 112   | `tool_search`                      | ⬜      |

#### RLM Agents (113-116)
| Index | Name                            | Status |
| ----- | ------------------------------- | ------ |
| 113   | `rlm_agent` / `recursive_agent` | ⬜      |
| 114   | `rlm_config`                    | ⬜      |
| 115   | `rlm_stats`                     | ⬜      |
| 116   | `rlm_spawn` / `spawn_agent`     | ⬜      |

#### MCP Client (117-122)
| Index | Name                                   | Status |
| ----- | -------------------------------------- | ------ |
| 117   | `mcp_server`                           | ⬜      |
| 118   | `mcp_tools` / `mcp_list_tools`         | ⬜      |
| 119   | `mcp_call` / `mcp_call_tool`           | ⬜      |
| 120   | `mcp_resources` / `mcp_list_resources` | ⬜      |
| 121   | `mcp_prompts` / `mcp_list_prompts`     | ⬜      |
| 122   | `mcp_connect` / `mcp_client`           | ⬜      |

#### Utilities (123-136)
| Index | Name                       | Status |
| ----- | -------------------------- | ------ |
| 123   | `sh` / `shell`             | ⬜      |
| 124   | `now` / `timestamp`        | ⬜      |
| 125   | `sort_by`                  | ⬜      |
| 126   | `save_json` / `write_json` | ⬜      |
| 127   | `ai_backends`              | ⬜      |
| 128   | `mcp_server_start`         | ⬜      |
| 129   | `agent_with_mcp`           | ⬜      |
| 130   | `each`                     | ⬜      |
| 131   | `in`                       | ⬜      |
| 132   | `values`                   | ⬜      |
| 133   | `sum`                      | ⬜      |
| 134   | `unique`                   | ⬜      |
| 135   | `avg` / `mean`             | ⬜      |
| 136   | `product`                  | ⬜      |

#### Configuration (137-143)
| Index | Name            | Status |
| ----- | --------------- | ------ |
| 137   | `config`        | ⬜      |
| 138   | `config_get`    | ⬜      |
| 139   | `config_set`    | ⬜      |
| 140   | `config_path`   | ⬜      |
| 141   | `config_init`   | ⬜      |
| 142   | `config_reload` | ⬜      |
| 143   | `themes`        | ⬜      |

---

## Test Infrastructure

### File Structure

```
tests/
├── *.rs                    # Rust integration tests
├── feature_*.ae            # Language feature tests (syntax)
├── scripts/
│   ├── builtins/          # Builtin function tests
│   │   └── test_*.ae      # Individual builtin tests
│   └── integration/       # Complex integration tests
│       └── test_*.ae      # End-to-end scenarios
└── coverage/              # NEW: Systematic coverage tests
    ├── builtins_*.ae      # Category-based builtin tests
    └── syntax_*.ae        # Complete syntax coverage
```

### Running Tests

```bash
# Run all Rust tests
cargo test

# Run specific test file
cargo test --test smoke

# Run feature tests via ae binary
./target/debug/ae tests/feature_00_comprehensive.ae

# Run all .ae tests (via test runner)
./scripts/run_ae_tests.sh
```

---

## Test Assertions

AetherShell tests use `print()` with visual checkmarks for manual verification.

For automated testing, use the `assert` pattern:

```aethershell
# Manual assertion pattern
result = 1 + 1
match result == 2 {
    true => print("✓ Addition works"),
    false => print("✗ FAILED: Addition")
}

# Or use type checks
match type_of(result) {
    "Int" => print("✓ Type correct"),
    _ => print("✗ FAILED: Wrong type")
}
```

---

## Coverage Goals

| Category               | Current | Target |
| ---------------------- | ------- | ------ |
| Language Syntax        | ~90%    | 100%   |
| Core Builtins (0-29)   | ~60%    | 100%   |
| File System (8-18)     | ~80%    | 100%   |
| String Ops (34-42)     | ~30%    | 100%   |
| Math Ops (50-57)       | ~20%    | 100%   |
| Neural Network (72-82) | ~0%     | 100%   |
| Evolution (83-92)      | ~0%     | 100%   |
| RL (93-107)            | ~0%     | 100%   |
| MCP/Tools (108-129)    | ~30%    | 100%   |
| Config (137-143)       | ~0%     | 100%   |

---

## Implementation Plan

### Phase 1: Syntax Coverage (Priority: High)
1. ✅ `feature_00_comprehensive.ae` - All features overview
2. ✅ `feature_01_literals.ae` through `feature_12_builtins.ae`
3. ⬜ Add edge cases and error handling tests

### Phase 2: Core Builtins (Priority: High)
1. ⬜ Create `tests/coverage/builtins_core.ae` (0-29)
2. ⬜ Create `tests/coverage/builtins_string.ae` (34-42)
3. ⬜ Create `tests/coverage/builtins_array.ae` (43-49)
4. ⬜ Create `tests/coverage/builtins_math.ae` (50-57)

### Phase 3: Domain Builtins (Priority: Medium)
1. ⬜ Create `tests/coverage/builtins_nn.ae` (72-82)
2. ⬜ Create `tests/coverage/builtins_evolution.ae` (83-92)
3. ⬜ Create `tests/coverage/builtins_rl.ae` (93-107)

### Phase 4: Integration Builtins (Priority: Medium)
1. ⬜ Create `tests/coverage/builtins_mcp.ae` (117-122)
2. ⬜ Create `tests/coverage/builtins_tools.ae` (108-116)
3. ⬜ Create `tests/coverage/builtins_config.ae` (137-143)

### Phase 5: Automated Test Runner (Priority: High)
1. ⬜ Create `scripts/run_tests.ps1` for Windows
2. ⬜ Create `scripts/run_tests.sh` for Unix
3. ⬜ Add CI integration with GitHub Actions

---

## Running Full Test Suite

```bash
# Complete test run
cargo test --all

# With verbose output
cargo test --all -- --nocapture

# Run specific category
cargo test ai_      # All AI tests
cargo test tui_     # All TUI tests
cargo test builtin  # All builtin tests
```

---

## Contributing Tests

When adding new features:

1. Add syntax test to appropriate `feature_*.ae` file
2. Add Rust unit test if function is in library
3. Add integration test for complex interactions
4. Update this document with coverage status

---

## Theme Testing

All 38 themes should be tested for:

1. ✅ Valid color definitions (compile-time)
2. ⬜ Correct ANSI output (manual/visual)
3. ⬜ Theme switching via `config_set`

---

*Last Updated: 2025*
