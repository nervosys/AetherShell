// E3d -- the failures that exit 0.
//
// `errors.mjs` reports, for every engine, "failures that exited 0 -- an agent
// checking status alone sees success", because a wrong answer at exit 0 is the
// most expensive thing a shell can do to an agent: there is nothing to branch
// on, nothing to retry, and the wrong value propagates.
//
// We apply that test to other shells and had never applied it to ourselves.
// Both uncoded sweeps report a number they pass over without comment:
//
//     461  accepted it and answered      (uncoded.mjs)
//     501  accepted it and answered      (uncoded-paths.mjs)
//
// Those builtins were handed an argument no builtin can accept, and returned a
// value. Some are right to: `mkdir(path)` on a missing path is exactly what
// mkdir is for, and `len({unexpected: true})` is legitimately 1. But the
// original motivating defects for `src/signature.rs` were all of this shape --
//
//     round(4.966, 2)        -> 5       the digits argument was discarded
//     env(name, default)     -> the default was discarded
//     db_json_to_sqlite("x") -> false   at exit 0
//
// -- so the class is known to be real here, and unmeasured.
//
//   node benches/agentic/silent-success.mjs
//
// **What this can and cannot decide.** There is no oracle for "should this
// have failed?", so this does not report defects. It reports the builtins that
// answered a nonsense argument with a value carrying no information -- null,
// false, an empty string, an empty collection -- which is where a silent
// failure would hide if there were one. Every hit needs reading. A count of
// suspects is not a count of bugs, and saying otherwise would be the same
// error as quoting the edit count instead of the sweep.

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

const JAIL = fs.mkdtempSync(path.join(os.tmpdir(), 'ae-silent-'));
// A throwaway HOME, outside the workspace jail as a real one is. Thousands of
// calls include builtins that keep state under ~ (platform_db_store, config,
// history); run against the real HOME, a probe writes into the user's shell
// state -- which is how "a", "hello" and "definitely-missing-94117.txt" came
// to be saved platform snapshots on the machine these probes were built on.
const SCRATCH = fs.mkdtempSync(path.join(os.tmpdir(), 'ae-home-'));
process.on('exit', () => fs.rmSync(SCRATCH, { recursive: true, force: true }));
const SCRATCH_HOME = {
    HOME: SCRATCH,
    USERPROFILE: SCRATCH,
    XDG_CONFIG_HOME: path.join(SCRATCH, '.config'),
    XDG_DATA_HOME: path.join(SCRATCH, '.local', 'share'),
    XDG_CACHE_HOME: path.join(SCRATCH, '.cache'),
};
process.on('exit', () => fs.rmSync(JAIL, { recursive: true, force: true }));

const run = (code) => {
    const r = spawnSync('ae', ['--agent', '--policy', 'strict', '--workspace', JAIL, '-c', code], {
        cwd: JAIL,
        encoding: 'utf8',
        timeout: 10000,
        env: { ...process.env, ...SCRATCH_HOME, AETHER_MAX_NET: '0' },
    });
    const out = ((r.stdout ?? '') + (r.stderr ?? '')).trim();
    return r.signal === 'SIGTERM' && r.status === null
        ? { out: '\u0000TIMEOUT', status: null }
        : { out, status: r.status };
};

const probe = run('1 + 1');
if (probe.out !== '2') {
    console.error(`! \`ae\` is not answering (got ${JSON.stringify(probe.out)}); is it on PATH?`);
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

// A value that carries no information. This is the shape a silent failure
// takes: the builtin could not do the work, and said so with a value instead
// of an error.
const VACUOUS = new Set(['null', 'false', '""', "''", '[]', '{}', '0', '-']);
const isVacuous = (s) => VACUOUS.has(s.trim());

const ARG = '{unexpected: true}';
const suspects = [];
const answered = [];
let done = 0;
for (const name of [...names].sort()) {
    const { out, status } = run(`${name}(${ARG})`);
    if (out === '\u0000TIMEOUT') continue;
    const failed = out.startsWith('error') || out.startsWith('{"error"');
    if (failed || status !== 0) continue;
    answered.push(name);
    if (isVacuous(out)) suspects.push(`${name}\t-> ${JSON.stringify(out)}`);
    if (++done % 200 === 0) process.stderr.write(`  ${done}/${names.size}\n`);
}

// Non-vacuity: if nothing answered, this measured nothing. If EVERYTHING
// answered, the probe is not being rejected anywhere and says nothing either.
if (answered.length < 50 || answered.length > names.size - 50) {
    console.error(
        `! ${answered.length} of ${names.size} answered; that is not a usable split`,
    );
    process.exit(2);
}

const pct = ((suspects.length / answered.length) * 100).toFixed(1);
console.log(`\n${answered.length} builtins answered a nonsense argument.`);
console.log(`${suspects.length} of those (${pct}%) answered with a value carrying no`);
console.log('information -- null, false, empty. That is where a silent failure hides.');
console.log('\nEach needs reading; this is a list of suspects, not of defects.\n');
for (const s of suspects) console.log(`  ${s}`);
fs.writeFileSync('silent-success.txt', `${suspects.join('\n')}\n`);
console.log('\nlist written to silent-success.txt');
