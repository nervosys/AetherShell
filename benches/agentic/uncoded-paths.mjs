// E3c -- the second failure path.
//
// `uncoded.mjs` hands every builtin one argument no builtin can accept and
// tallies the response. It reports 0 uncoded of 1,052, and that number is
// real, but it answers a narrower question than it looks: **no builtin's
// FIRST failure is uncoded**. A builtin that rejects the nonsense argument on
// type never reaches its path handling, its network handling, or anything
// else behind that first check.
//
// `cat("")` found the gap. The sweep probes `cat({unexpected: true})`, which
// fails on the type; the empty string gets past that and into
// `validate_safe_path`, which returned bare `anyhow!` for every refusal --
// including path traversal, on the shell's main security surface. An agent
// could not tell "outside the workspace" from "unknown failure".
//
// So this is the same idea aimed one layer deeper: every builtin, handed a
// *well-formed* string argument that cannot possibly work. A path that does
// not exist is the commonest failure any shell has.
//
//   node benches/agentic/uncoded-paths.mjs
//
// Safety is identical to the first sweep: `--agent --policy strict` in a
// throwaway workspace with `AETHER_MAX_NET=0`, so the effect gate refuses
// anything dangerous before a body runs.

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

const JAIL = fs.mkdtempSync(path.join(os.tmpdir(), 'ae-paths-'));
process.on('exit', () => fs.rmSync(JAIL, { recursive: true, force: true }));

const run = (code) => {
    const r = spawnSync('ae', ['--agent', '--policy', 'strict', '--workspace', JAIL, '-c', code], {
        cwd: JAIL,
        encoding: 'utf8',
        timeout: 10000,
        env: { ...process.env, AETHER_MAX_NET: '0' },
    });
    const out = ((r.stdout ?? '') + (r.stderr ?? '')).trim();
    return r.signal === 'SIGTERM' && r.status === null ? '\u0000TIMEOUT' : out;
};

const probe = run('1 + 1');
if (probe !== '2') {
    console.error(`! \`ae\` is not answering (got ${JSON.stringify(probe)}); is it on PATH?`);
    process.exit(2);
}

// Same staleness guard as the first sweep, and for the same reason: a number
// measured from a binary older than the sources is worse than no number.
const AE = spawnSync(process.platform === 'win32' ? 'where' : 'which', ['ae'], {
    encoding: 'utf8',
})
    .stdout?.split('\n')[0]
    ?.trim();
try {
    const bin = fs.statSync(AE).mtimeMs;
    const src = fs.statSync(new URL('../../src/builtins.rs', import.meta.url)).mtimeMs;
    if (bin < src) {
        console.error(`! \`${AE}\` is older than src/builtins.rs; rebuild before sweeping.`);
        process.exit(2);
    }
} catch (e) {
    console.error(`! could not check \`ae\` against the sources (${e.message}).`);
}

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
if (names.size < 500) {
    console.error(`! only ${names.size} builtins enumerated; the walk is broken`);
    process.exit(2);
}

// Well-formed, and cannot work: a relative path inside the workspace that is
// simply not there. Deliberately NOT a traversal — `../../etc/passwd` is a
// policy refusal, which is a different (and already coded) answer.
const ARG = 'definitely-missing-94117.txt';

const tally = new Map();
const uncoded = [];
const hung = [];
let done = 0;
for (const name of [...names].sort()) {
    const out = run(`${name}(${JSON.stringify(ARG)})`);
    const failed = out.startsWith('error') || out.startsWith('{"error"');
    const m = failed ? out.match(/E_[A-Z_0-9]+/) : null;
    const code =
        out === '\u0000TIMEOUT'
            ? 'HUNG'
            : m
              ? m[0]
              : failed
                ? 'NO_CODE'
                : 'OK';
    tally.set(code, (tally.get(code) ?? 0) + 1);
    if (code === 'HUNG') hung.push(name);
    if (code === 'E_UNKNOWN' || code === 'NO_CODE') uncoded.push(`${name}\t${out.slice(0, 120)}`);
    if (++done % 200 === 0) process.stderr.write(`  ${done}/${names.size}\n`);
}

console.log('\nby code:');
for (const [k, v] of [...tally].sort((a, b) => b[1] - a[1])) {
    console.log(`  ${String(v).padStart(5)}  ${k}`);
}
const pct = ((uncoded.length / names.size) * 100).toFixed(1);
console.log(`\nuncoded on the second failure path: ${uncoded.length} of ${names.size} (${pct}%)`);
fs.writeFileSync('uncoded-paths.txt', `${uncoded.join('\n')}\n`);
if (hung.length) {
    console.log(`\n${hung.length} hung: ${hung.join(', ')}`);
}
console.log('list written to uncoded-paths.txt');
