// E3e -- arguments accepted and thrown away.
//
// The table at the top of `src/signature.rs` opens with these:
//
//     round(4.966, 2)     -> 5       the digits argument was discarded
//     env(name, default)  -> the default was discarded
//
// A call the builtin could not honour, answered with a value instead of a
// refusal. Declaring a signature fixes it, because `validate` checks arity
// before the body runs -- but only 53 of 1,052 builtins are declared. Nothing
// has ever checked the other 999.
//
//   node benches/agentic/discarded-args.mjs
//
// **The oracle is differential, so this reports defects rather than suspects.**
// If `f(x)` and `f(x, junk, junk)` return the same bytes, the extra arguments
// changed nothing: they were accepted and discarded. No judgement is needed
// about what the builtin "should" do.
//
// Two things have to be ruled out first, and both are ruled out by measuring
// rather than by reasoning:
//
//   * **Nondeterminism.** `crypto.uuid()` differs from itself, so it would
//     look like the extras mattered. Every builtin is run twice with the SAME
//     arguments first; one that disagrees with itself is excluded, and the
//     count of exclusions is reported -- a sweep that silently dropped half
//     the catalogue would otherwise look like a clean result.
//
//   * **Failure.** If `f(x)` already fails, `f(x, junk)` fails identically and
//     proves nothing. Only calls that SUCCEED are compared.
//
// A variadic builtin legitimately consumes extra arguments -- and that is not
// a false positive here, because consuming them changes the output. `pick`
// taking three field names produces a different record than `pick` taking one.

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

const JAIL = fs.mkdtempSync(path.join(os.tmpdir(), 'ae-discard-'));
process.on('exit', () => fs.rmSync(JAIL, { recursive: true, force: true }));

const run = (code) => {
    const r = spawnSync('ae', ['--agent', '--policy', 'strict', '--workspace', JAIL, '-c', code], {
        cwd: JAIL,
        encoding: 'utf8',
        timeout: 10000,
        env: { ...process.env, AETHER_MAX_NET: '0' },
    });
    const out = ((r.stdout ?? '') + (r.stderr ?? '')).trim();
    return r.signal === 'SIGTERM' && r.status === null
        ? { out: '\u0000TIMEOUT', status: null }
        : { out, status: r.status };
};

if (run('1 + 1').out !== '2') {
    console.error('! `ae` is not answering; is it on PATH?');
    process.exit(2);
}

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

const cats = JSON.parse(run('json.stringify(ontology_manifest())').out).categories.map((c) =>
    typeof c === 'string' ? c : c.category,
);
const names = new Set();
for (const cat of cats) {
    try {
        for (const b of JSON.parse(
            run(`json.stringify(ontology_describe(${JSON.stringify(cat)}))`).out,
        ).builtins ?? []) {
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

// A plausible first argument (many builtins take a string), and extras that
// no builtin has a use for.
const BASE = '"a"';
const EXTRA = '"zzz-unused-94117", 424242, false';

const discarded = [];
const nondet = [];
let compared = 0;
let failedBase = 0;
let done = 0;

for (const name of [...names].sort()) {
    const a = run(`${name}(${BASE})`);
    if (a.out === '\u0000TIMEOUT') continue;
    const failed = a.out.startsWith('error') || a.out.startsWith('{"error"') || a.status !== 0;
    if (failed) {
        failedBase += 1;
        continue;
    }
    // Same call again: anything that disagrees with itself cannot be compared.
    const b = run(`${name}(${BASE})`);
    if (b.out !== a.out) {
        nondet.push(name);
        continue;
    }
    const c = run(`${name}(${BASE}, ${EXTRA})`);
    if (c.out === '\u0000TIMEOUT') continue;
    compared += 1;
    const cFailed = c.out.startsWith('error') || c.out.startsWith('{"error"') || c.status !== 0;
    if (!cFailed && c.out === a.out) {
        discarded.push(`${name}\t${a.out.replace(/\s+/g, ' ').slice(0, 80)}`);
    }
    if (++done % 100 === 0) process.stderr.write(`  ${done}/${names.size}\n`);
}

// Non-vacuity. If almost nothing could be compared, the result says nothing
// about the catalogue, and a small `discarded` number would look like health.
if (compared < 100) {
    console.error(
        `! only ${compared} builtins were comparable (${failedBase} refused the base ` +
            `call, ${nondet.length} disagreed with themselves); that is not a usable sample`,
    );
    process.exit(2);
}

const pct = ((discarded.length / compared) * 100).toFixed(1);
console.log(`\n${compared} builtins accepted \`${BASE}\` and were comparable.`);
console.log(`${failedBase} refused it (nothing to compare), ${nondet.length} disagreed with`);
console.log('themselves and were excluded as nondeterministic.\n');
console.log(`${discarded.length} of ${compared} (${pct}%) returned the SAME value with three`);
console.log('extra arguments as without them. Those arguments were discarded.\n');
for (const d of discarded.slice(0, 40)) console.log(`  ${d}`);
if (discarded.length > 40) console.log(`  ... and ${discarded.length - 40} more`);
fs.writeFileSync('discarded-args.txt', `${discarded.join('\n')}\n`);
if (nondet.length) {
    fs.writeFileSync('nondeterministic.txt', `${nondet.join('\n')}\n`);
}
console.log('\nlists written to discarded-args.txt and nondeterministic.txt');
