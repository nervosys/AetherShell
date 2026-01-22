# AetherShell Standard Library

The standard library provides commonly used functions and utilities organized into logical modules.

## Modules

| Module       | Description                                          |
| ------------ | ---------------------------------------------------- |
| `prelude`    | Most commonly used functions (auto-imported in REPL) |
| `math`       | Mathematical functions and constants                 |
| `string`     | String manipulation functions                        |
| `collection` | Collection manipulation utilities                    |
| `functional` | Functional programming utilities                     |
| `io`         | File I/O and data serialization                      |

## Usage

```ae
# Import modules
import "lib/prelude.ae"
import "lib/math.ae"
import "lib/string.ae"
import "lib/collection.ae"
import "lib/functional.ae"
import "lib/io.ae"

# Use imported functions
PI                          # 3.14159...
is_even(4)                  # true
words("hello world")        # ["hello", "world"]
union([1,2], [2,3])         # [1,2,3]
find([1,2,3,4], fn(x) => x > 2)  # 3
```

## Module Overview

### prelude.ae
Core utilities that most programs need:
- `id` - Identity function
- `compose` - Function composition (right-to-left)*
- `pipe_fn` - Function composition (left-to-right)*
- `flip` - Flip function arguments*
- `not`, `both`, `either` - Boolean utilities
- `is_some`, `is_none`, `get_or` - Option utilities
- `clamp` - Clamp value to range

*Note: Higher-order functions that return lambdas may have closure issues when imported.

### math.ae
Mathematical utilities:
- `PI`, `E`, `PHI`, `TAU` - Mathematical constants
- `is_even`, `is_odd` - Parity checking
- `is_positive`, `is_negative`, `is_zero` - Sign checking
- `sign` - Sign of a number (-1, 0, or 1)
- `lerp`, `inv_lerp` - Linear interpolation
- `square`, `cube` - Power functions
- `gcd`, `lcm` - Number theory
- `factorial`, `fib` - Sequences
- `deg_to_rad`, `rad_to_deg` - Angle conversion
- `mean` - Statistics

### string.ae
String manipulation utilities:
- `words`, `unwords` - Split/join by spaces
- `lines`, `unlines` - Split/join by newlines
- `capitalize`, `title_case` - Case transformation
- `snake_case`, `kebab_case`, `screaming_snake` - Case styles
- `is_blank`, `is_not_blank` - Blank checking
- `is_upper`, `is_lower` - Case checking
- `repeat`, `reverse_str`, `count_substr` - Utilities

### collection.ae
Collection manipulation:
- `partition` - Split by predicate
- `union`, `intersect`, `difference` - Set operations
- `is_subset` - Subset checking
- `sort_desc` - Reverse sort

### functional.ae
Advanced functional programming utilities:
- `curry`, `uncurry` - Currying functions*
- `partial`, `partial_right` - Partial application*
- `twice` - Apply function twice*
- `complement`, `and_p`, `or_p` - Predicate combinators*
- `find` - Find first element matching predicate
- `drop` - Drop first n elements

*Note: Functions that return lambdas may have closure issues when imported.

### io.ae
Input/output utilities:
- `read_json` - Read JSON file
- `read_lines`, `read_trimmed` - Read text files
- `file_ext`, `file_name`, `file_dir` - Path utilities
- `path_join` - Join path components
- `to_json_pretty` - JSON serialization
- `parse_yaml`, `parse_csv` - Data parsing

## Testing

Run the standard library tests:
```bash
./target/debug/ae lib/test_stdlib.ae
```
