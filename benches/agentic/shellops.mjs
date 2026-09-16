// E2 -- the shell substrate itself.
//
// E1 asks how cheaply each engine answers a question about a dataset. E2 asks
// the prior question: what does an agent pay for one ordinary shell turn? The
// corpus is eight things an agent does constantly while working in a
// repository, run against this repository, with the output captured as the
// agent would receive it.
//
// The existing examples/shell_bench.rs compares the same idea against
// *representative* outputs written by hand. This one executes.

export const ENGINES = {
    aethershell: { bin: 'ae', argv: (c) => ['-c', c], label: 'AetherShell 12.0.2' },
    bash: { bin: 'bash', argv: (c) => ['-c', c], label: 'bash 5.2 + coreutils' },
    pwsh: { bin: 'pwsh', argv: (c) => ['-NoProfile', '-Command', c], label: 'PowerShell 7.6.6' },
    nushell: { bin: 'nu', argv: (c) => ['-n', '-c', c], label: 'nushell 0.115.1' },
};

export const TASKS = {
    t1: 'List the .rs files in src/ with their sizes.',
    t2: 'Total line count of the Rust sources in src/.',
    t3: 'The five largest files in src/, with sizes.',
    t4: 'The current git branch.',
    t5: 'How many files are under tests/.',
    t6: 'The package version declared in Cargo.toml.',
    t7: 'Count the .rs files under src/ recursively.',
    t8: 'The newest three files in src/, by modification time.',
};

export const COMMANDS = {
    aethershell: {
        t1: 'ls("src") | where(fn(f) => ends_with(f.name, ".rs")) | pick("name", "size")',
        t2: 'ls("src") | where(fn(f) => ends_with(f.name, ".rs")) | map(fn(f) => len(split(cat(f.path), "\\n"))) | sum',
        t3: 'ls("src") | sort_by("size") | last(5) | pick("name", "size")',
        t4: 'git_branch()',
        t5: 'fs_walk("tests") | len',
        t6: 'cat("Cargo.toml") | split("\\n") | where(fn(l) => starts_with(l, "version")) | first',
        t7: 'fs_walk("src") | where(fn(f) => ends_with(f, ".rs")) | len',
        t8: 'ls("src") | sort_by("modified") | last(3) | pick("name", "modified")',
    },
    bash: {
        t1: 'ls -l src/*.rs',
        t2: 'cat src/*.rs | wc -l',
        t3: 'ls -lS src | head -6',
        t4: 'git rev-parse --abbrev-ref HEAD',
        t5: 'find tests -type f | wc -l',
        t6: 'grep -m1 "^version" Cargo.toml',
        t7: 'find src -name "*.rs" | wc -l',
        t8: 'ls -lt src | head -4',
    },
    pwsh: {
        t1: 'Get-ChildItem src/*.rs | Select-Object Name,Length',
        t2: '(Get-Content src/*.rs | Measure-Object -Line).Lines',
        t3: 'Get-ChildItem src -File | Sort-Object Length -Descending | Select-Object -First 5 Name,Length',
        t4: 'git rev-parse --abbrev-ref HEAD',
        t5: '(Get-ChildItem tests -Recurse -File).Count',
        t6: 'Select-String -Path Cargo.toml -Pattern "^version" | Select-Object -First 1 -ExpandProperty Line',
        t7: '(Get-ChildItem src -Recurse -Filter *.rs).Count',
        t8: 'Get-ChildItem src -File | Sort-Object LastWriteTime -Descending | Select-Object -First 3 Name,LastWriteTime',
    },
    nushell: {
        t1: 'ls src/*.rs | select name size',
        t2: 'ls src/*.rs | get name | each {|f| open $f | lines | length} | math sum',
        t3: 'ls src | sort-by size | last 5 | select name size',
        t4: 'git rev-parse --abbrev-ref HEAD',
        t5: 'ls tests/**/* | where type == file | length',
        t6: 'open Cargo.toml | get package.version',
        t7: 'ls src/**/*.rs | length',
        t8: 'ls src | sort-by modified | last 3 | select name modified',
    },
};
