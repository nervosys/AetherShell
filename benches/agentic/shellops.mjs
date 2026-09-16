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
    // The mode an agent actually runs in: AECON output, effect gating on. It is
    // listed separately rather than replacing the default, because the two
    // render differently and a reader is entitled to both numbers.
    'aethershell (agent)': {
        bin: 'ae', argv: (c) => ['--agent', '-c', c], label: 'AetherShell 12.0.2 --agent',
    },
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

// What every engine's output must contain for the measurement to count.
//
// E2 originally had no oracle: `correct` was `exit === 0 && output non-empty`.
// That let AetherShell's default renderer -- which printed an array of records
// as `[{…}, {…}, …]` -- score 316 bytes against bash's 2,650 and be reported
// as a 7.9x win, while containing none of the requested data. An output-size
// comparison is meaningless unless something checks that both outputs contain
// the answer. These are deliberately loose: a filename that must appear, and a
// number where a number belongs. They are not equality checks, because the
// engines legitimately format differently -- they exist to catch an output
// that is not an answer at all.
// A file size, however the engine chose to write it. Engines legitimately
// differ here and the oracle must not mistake a different encoding for a
// missing answer:
//
//   bash                27260
//   AetherShell --agent 27260, under a factored `@suffix name: .rs` header
//   nushell             27.2 kB
//
// That third form is worth noticing rather than merely tolerating: nushell's
// default table rounds to three significant figures, so an agent that asks for
// file sizes cannot sum, diff or compare them exactly. It is the only engine
// here whose cheapest path to the answer loses information, and it is also the
// most expensive.
const SIZE = /\d{3,}|\d+(\.\d+)?\s*[kKMGT]i?B/;

// `\blib\b` rather than `lib\.rs`, because AetherShell's agent mode factors the
// common suffix out of the column (`@suffix name: .rs` then bare stems). The
// data is there and is reconstructible; only the spelling differs.
export const EXPECT = {
    t1: [/\blib\b/, SIZE],
    t2: [/\d{4,}/],
    t3: [/builtins/, SIZE],
    // A bare `\w` accepted a header row with no data in it; the branch this
    // repository is on is a known fact, exactly as `builtins` is in t3.
    t4: [/master|main/],
    t5: [/\d{2,}/],
    t6: [/12\.\d+\.\d+/],
    t7: [/\d{2,}/],
    t8: [/\.rs|@suffix/],
};

// A vacuity guard on the guard itself. The first version of this table was
// written through a script whose escaping ate every backslash, turning
// `/\d{4,}/` into `/d{4,}/` -- a pattern matching four literal letter d's,
// which nothing would ever match, in a table whose entire job is to catch
// output that is not an answer. A broken oracle is worse than none, because it
// reads as rigour. This asserts the patterns match text of the shape they are
// meant to accept and reject text of the shape they are meant to catch.
export function selfCheck() {
    // One sample per engine's real encoding of the same facts. All three must
    // be accepted, or the oracle is scoring spelling instead of content.
    const accept = {
        'plain (bash, AetherShell default)':
            'lib.rs 10991 builtins.rs 1770246 master 190 12.0.2 88 parser.rs',
        'AECON (AetherShell --agent)':
            '@suffix name: .rs\nlib\t10991\nbuiltins\t1770246\nmaster 190 12.0.2 88 parser.rs',
        'rounded units (nushell)':
            '│ src/lib.rs │ 11.0 kB │ src/builtins.rs │ 1.8 MB │ master 190 12.0.2 88 parser.rs 109968',
    };
    // Things that are not answers and must never score.
    const reject = {
        'elided records': '[{…}, {…}, {…}]',
        'empty': '',
        'headers only': 'name\tsize',
    };

    const problems = [];
    for (const [task, pats] of Object.entries(EXPECT)) {
        for (const re of pats) {
            for (const [label, text] of Object.entries(accept)) {
                if (!re.test(text)) problems.push(`${task}: ${re} rejects ${label}`);
            }
        }
        // A rejection only has to fail one of the task's patterns.
        for (const [label, text] of Object.entries(reject)) {
            if (pats.every((re) => re.test(text))) {
                problems.push(`${task}: accepts ${label}, which is not an answer`);
            }
        }
    }
    return problems;
}

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

// Agent mode runs the identical programs; only the renderer and the effect
// gate differ. Sharing the map rather than copying it keeps the two rows a
// comparison of output modes and not, accidentally, of two different corpora.
COMMANDS['aethershell (agent)'] = COMMANDS.aethershell;
