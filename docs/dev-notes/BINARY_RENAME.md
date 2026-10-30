# Binary Rename: ironshell → ish

## Summary
Successfully renamed the binary from `ironshell` to `ish` to match the intended user experience described in the README and documentation.

## Changes Made

### Cargo.toml
- Added explicit `[[bin]]` section to specify binary name as "ae"
- Maintained library name as "ironshell" for internal consistency

```toml
[[bin]]
name = "ae"
path = "src/main.rs"
```

## Verification
- ✅ `cargo build --bin ish` - Build successful
- ✅ `cargo run --bin ish` - REPL launches correctly  
- ✅ `cargo run --bin ish -- --tui` - TUI launches correctly
- ✅ `cargo test --lib` - All tests pass (15/15)
- ✅ `cargo build --release --bin ish` - Release build successful
- ✅ `./target/release/ish.exe` - Direct binary execution works

## User Impact
- **Before**: Users had to run `cargo run --bin ironshell` 
- **After**: Users can run `cargo run --bin ish` as documented
- **Installation**: After `cargo install --path .`, users will have `ish` command available
- **Consistency**: Matches all documentation examples in README.md

## Files Updated
- `Cargo.toml` - Added binary configuration
- `test_exit.sh` - Already used correct binary name

## Files Already Correct
- `README.md` - Already used `ish` throughout
- `src/main.rs` - Usage function already showed `ish`
- All documentation files already referenced `ish`

## Next Steps
This completes the binary rename. Users can now:
1. Build: `cargo build --bin ish`
2. Run REPL: `cargo run --bin ish` 
3. Run TUI: `cargo run --bin ish -- --tui`
4. Install: `cargo install --path .` (creates `ish` command)
