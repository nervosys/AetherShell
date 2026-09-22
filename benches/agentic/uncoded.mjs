// E3b -- does "every failure is branchable" hold across the whole catalogue?
//
// E3 (`errors.mjs`) measures ten failures an agent actually causes, chosen to
// be representative, and AetherShell codes all ten. That is a real result about
// ten cases and it is worth knowing how far it generalises, because a benchmark
// of hand-picked cases flattering the party who picked them is the criticism
// `docs/TYPED_SHELL_RESPONSE.md` makes of other people's benchmarks.
//
// So: every builtin in the catalogue, handed one argument no builtin can
// reasonably accept, and the response code tallied.
//
//   node benches/agentic/uncoded.mjs
//
// **Safety.** Every call runs through `ae --agent --policy strict --workspace
// <jail>` with `AETHER_MAX_NET=0`, in a throwaway directory. The effect gate
// refuses anything classified dangerous *before the body runs*, so this does
// not rest on anyone's judgement about which builtins are safe to poke -- it
// rests on the gate built for exactly that. A policy refusal is coded by
// construction and is counted separately: the question is which builtins ran
// and *then* failed badly.
//
// Result on 2026-09-21 (1,052 builtins across 54 categories):
//
//     507  accepted it and answered
//     227  E_BAD_ARG
//     156  E_UNKNOWN          <- 14.8% of the catalogue
//     142  E_NEEDS_APPROVAL   (the gate, correctly)
//      12  E_UNKNOWN_BUILTIN
//       4  E_POLICY_DENY
//       3  E_PARSE
//       1  E_FILE
//
// The last row varies by one between runs (E_REALTIME_TIMESTAMP on an
// earlier sweep): a builtin whose answer depends on the clock or the
// filesystem. Worth knowing before reading a one-builtin change as signal.
//
// The 156 are type errors -- `cat: path must be a string`, `file_append: path
// must be a string` -- built ad hoc instead of through the shared `expect_*`
// helpers, which produce `E_BAD_ARG`. `E_UNKNOWN` is the one code the taxonomy
// tells an agent *not* to reason about, so each is a knowable condition
// reported as unknowable.
//
// `tests/uncoded_failure_census.rs` holds the line at zero for the
// data-transformation categories, in-process and fast enough for CI. This
// sweep is the wide, slow version: 1,052 subprocesses, far too slow for the
// suite, and the way to check whether the number is falling.

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

const JAIL = fs.mkdtempSync(path.join(os.tmpdir(), 'ae-uncoded-'));
process.on('exit', () => fs.rmSync(JAIL, { recursive: true, force: true }));

const run = (code) => {
    const r = spawnSync('ae', ['--agent', '--policy', 'strict', '--workspace', JAIL, '-c', code], {
        cwd: JAIL,
        encoding: 'utf8',
        timeout: 10000,
        env: { ...process.env, AETHER_MAX_NET: '0' },
    });
    return ((r.stdout ?? '') + (r.stderr ?? '')).trim();
};

const probe = run('1 + 1');
if (probe !== '2') {
    console.error(`! \`ae\` is not answering (got ${JSON.stringify(probe)}); is it on PATH?`);
    process.exit(2);
}

// Walk the catalogue the way an agent would: manifest, then each category.
const cats = JSON.parse(run('json.stringify(ontology_manifest())')).categories.map((c) =>
    typeof c === 'string' ? c : c.category,
);
const names = new Set();
for (const cat of cats) {
    try {
        for (const b of JSON.parse(run(`json.stringify(ontology_describe(${JSON.stringify(cat)}))`))
            .builtins ?? []) {
            names.add(b.name);
        }
    } catch {
        // A category with no listing is not a failure of this sweep.
    }
}
console.log(`${cats.length} categories, ${names.size} builtins`);

// Non-vacuity: a sweep that reaches nothing proves nothing.
if (names.size < 500) {
    console.error(`! only ${names.size} builtins enumerated; the walk is broken`);
    process.exit(2);
}

const tally = new Map();
const uncoded = [];
let done = 0;
for (const name of [...names].sort()) {
    const out = run(`${name}({unexpected: true})`);
    const m = out.match(/E_[A-Z_0-9]+/);
    const code = m ? m[0] : out.startsWith('error') ? 'NO_CODE' : 'OK';
    tally.set(code, (tally.get(code) ?? 0) + 1);
    if (code === 'E_UNKNOWN' || code === 'NO_CODE') uncoded.push(name);
    if (++done % 200 === 0) process.stderr.write(`  ${done}/${names.size}\n`);
}

console.log('\nby code:');
for (const [k, v] of [...tally].sort((a, b) => b[1] - a[1])) {
    console.log(`  ${String(v).padStart(5)}  ${k}`);
}
const pct = ((uncoded.length / names.size) * 100).toFixed(1);
console.log(`\nuncoded: ${uncoded.length} of ${names.size} (${pct}%)`);
fs.writeFileSync('uncoded.txt', `${uncoded.join('\n')}\n`);
console.log('full list written to uncoded.txt');
