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
// Result on 2026-09-22 (1,052 builtins across 54 categories):
//
//     461  accepted it and answered
//     285  E_BAD_ARG
//     142  E_NEEDS_APPROVAL   (the gate, correctly)
//      96  E_BUDGET_EXCEEDED  (AETHER_MAX_NET=0, correctly)
//      31  E_TOOL_MISSING
//      12  E_UNKNOWN_BUILTIN
//       5  E_TOOL_FAILED
//       4  E_NO_UI
//       4  E_NOT_FOUND
//       4  E_POLICY_DENY
//       3  E_PARSE
//       3  E_BAD_STATE
//       2  E_UNIMPLEMENTED
//
//       0  E_UNKNOWN
//
// **E_UNKNOWN does not appear.** Every one of 1,052 builtins, handed an
// argument none of them can accept, answers with a code an agent can branch
// on. The code still exists -- it is the boundary's guarantee that nothing
// escapes as bare prose -- but nothing in the catalogue reaches it.
//
// How it got here, and why the edit count is the wrong number to quote:
//
//   157  before any of this
//   156  declaring `cat`                              1 builtin moved
//   140   54 ad-hoc type errors -> bad_arg           16 builtins moved
//    98  108 tool-not-found sites -> E_TOOL_MISSING  42 builtins moved
//    56  171 argument errors -> arg_err              42 builtins moved
//    29   38 tail sites + a census false positive    27 builtins moved
//    13   the last 16, read one at a time            16 builtins moved
//     2  346 bare `Command::new(p).output()?` sites  11 builtins moved
//     0   2 that wrapped the io error in prose        2 builtins moved
//
// ~750 edits, ~157 builtins moved. The 346-site pass is the clearest case of
// why the diff is the wrong number: it moved eleven builtins out of the
// uncoded column. Quote what the sweep reports.
//
// **The last sixteen were not what the label said.** Five passes of regex
// conversion had left a residue described as "the shell's own argument and
// type errors". Read individually -- which is what a sweep is for -- they
// were six different conditions:
//
//   crypto.verify_signature  the shell does not implement this  E_UNIMPLEMENTED
//   tx_commit                no transaction is open             E_BAD_STATE
//   finetune_status          that job does not exist            E_NOT_FOUND
//   docker_ps                docker ran and exited non-zero     E_TOOL_FAILED
//   eza                      conflated absent with failed       split in two
//   head/tail/wc             a genuine argument error           E_BAD_ARG
//
// One of six was the argument error the label claimed. Two were worse than
// uncoded: crypto.cert_parse and crypto.verify_cert reported E_BAD_ARG, which
// tells an agent to retry with different arguments when no arguments will
// ever work. The taxonomy went from ten codes to fourteen -- not by design
// review, but because the sweep kept naming conditions the shell could
// identify perfectly well and was declining to.
//
// **And the biggest single class was invisible to all of it.** 346 sites read
// `Command::new(prog).output()?`, propagating a bare io::Error: what reached
// an agent was `No such file or directory (os error 2)` -- no code, no
// builtin, not even the name of the program that was missing. Thirteen
// builtins still answered that *after* E_TOOL_MISSING shipped, because the
// code had been added only where the tool was already named. The sweep could
// not tell those apart from a missing *file*, which is the next entry.
//
// Seven error shapes had to be found, and each was invisible from the edit
// side until a builtin that was supposed to be fixed still was not:
//   * a dot in the builtin label (`a2a.register`)
//   * no article (`must be integer`)
//   * a multi-line `anyhow!` rustfmt had wrapped -- 74 sites, nearly as many
//     as the single-line pass that preceded it
//   * `.context("...")?` on an Option, which anyhow leaves uncoded
//   * a multi-line `anyhow!` with no format args at all
//   * prose that named a code the taxonomy did not have (`E_UNIMPLEMENTED:`),
//     so the message said one thing and the `code` field said another
//   * `.output()?` with no message at all -- the largest class, and the one
//     no grep for error text could ever have found
//
// And three false positives of its own, all flattering:
//   * `diagnose({unexpected: true})` SUCCEEDS, returning `{code: E_UNKNOWN}`
//     about the record it was handed. Grepping output for a code without
//     checking that the call failed counted the shell's own diagnostic
//     machinery as a defect.
//   * one run used a `release/ae` five days older than `src/` and reported
//     `head` and `uniq` -- both long fixed -- as still broken. Hence the
//     staleness guard below.
//   * the tool-absent classifier matched `No such file or directory` alone,
//     which is a missing *file* as much as a missing *program*. It filed
//     `make_targets` (no Makefile) and `ssh_config` (no ~/.ssh/config) under
//     the bucket this sweep does not score, excusing two real defects. A
//     classifier that quietly moves defects into its own unscored bucket is
//     the most flattering bug a benchmark can have.
//// `tests/uncoded_failure_census.rs` holds the line at zero for the
// data-transformation categories, in-process and fast enough for CI. This
// sweep is the wide, slow version: 1,052 subprocesses, far too slow for the
// suite, and the way to check whether the number is falling.

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

const JAIL = fs.mkdtempSync(path.join(os.tmpdir(), 'ae-uncoded-'));
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

// A call that times out must not look like a call that succeeded.
// `a2ui_confirm` waits for user confirmation and never returns without a TTY;
// spawnSync then yields empty output, which the classifier below read as
// "accepted it and answered". Found by sweeping the same table in-process,
// where there is no timeout and the run simply hung on the 15th builtin.
const run = (code) => {
    const r = spawnSync('ae', ['--agent', '--policy', 'strict', '--workspace', JAIL, '-c', code], {
        cwd: JAIL,
        encoding: 'utf8',
        timeout: 10000,
        env: { ...process.env, ...SCRATCH_HOME, AETHER_MAX_NET: '0' },
    });
    const out = ((r.stdout ?? '') + (r.stderr ?? '')).trim();
    // Node reports a timeout kill as SIGTERM with a null status.
    return r.signal === 'SIGTERM' && r.status === null ? '\u0000TIMEOUT' : out;
};

const probe = run('1 + 1');
if (probe !== '2') {
    console.error(`! \`ae\` is not answering (got ${JSON.stringify(probe)}); is it on PATH?`);
    process.exit(2);
}

// The `ae` on PATH must be newer than the sources, or this measures a shell
// that no longer exists. Found the hard way: a stale `release/ae` five days
// behind `src/` reported `head` and `uniq` -- the two the census docstring
// cites as fixed -- still uncoded, and would have restated a solved problem
// as an open one. Quoting a number from the wrong binary is the same error
// as quoting the edit count instead of the sweep, one level further out.
const AE = spawnSync(process.platform === 'win32' ? 'where' : 'which', ['ae'], {
    encoding: 'utf8',
})
    .stdout?.split('\n')[0]
    ?.trim();
const SRC = new URL('../../src/builtins.rs', import.meta.url);
try {
    const bin = fs.statSync(AE).mtimeMs;
    const src = fs.statSync(SRC).mtimeMs;
    if (bin < src) {
        const days = ((src - bin) / 86400000).toFixed(1);
        console.error(`! \`${AE}\` is ${days} day(s) older than src/builtins.rs.`);
        console.error('  Rebuild before sweeping; a stale binary reports fixed defects as open.');
        process.exit(2);
    }
} catch (e) {
    // Not being able to check is not the same as being stale, but say so.
    console.error(`! could not compare \`ae\` against the sources (${e.message});`);
    console.error('  the numbers below are only as fresh as whatever is on PATH.');
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

// An uncoded failure because the *external tool* is absent says nothing about
// the builtin's argument handling -- the probe never got that far. It is still
// worth coding (a missing tool is a knowable, branchable condition, not an
// unknown one), but it is a different defect from a type error, and counting
// them together inflates a claim about the shell with a fact about this host.
//
// This test used to be `/No such file or directory \(os error 2\)/` alone,
// which is the message for a missing *file* as much as a missing *program*.
// It filed `make_targets` (no Makefile here) and `ssh_config` (no ~/.ssh/config)
// under "external tool absent" and so excused them from the count -- a
// classifier that quietly moves defects into the bucket it does not score is
// the most flattering bug a benchmark can have. Both were real, both are now
// E_NOT_FOUND. The pattern now requires evidence that a *process* failed to
// start.
const TOOL_ABSENT =
    /not found: No such file|is not installed|could not start `|program not found/;

const tally = new Map();
const uncoded = [];
const toolAbsent = [];
const hung = [];
// What each uncoded builtin actually printed, so a failing CI run names the
// offender and its message instead of only a count. The first CI failure of
// this gate reported "2 uncoded" and nothing else, which costs a round trip.
const said = {};
let done = 0;
for (const name of [...names].sort()) {
    // A call that times out gets one more attempt before it counts as HUNG.
    // A real hang hangs again; a load spike does not. Without this the CI
    // gate on `hung == 0` would trip on the runner being briefly busy --
    // one run here reported a hang that three clean runs never reproduced --
    // and a gate that flakes is a gate people learn to ignore.
    let out = run(`${name}({unexpected: true})`);
    if (out === '\u0000TIMEOUT') out = run(`${name}({unexpected: true})`);
    // Only a *failed* call can be uncoded. `diagnose({unexpected: true})`
    // succeeds and returns `{code: E_UNKNOWN}` describing the record it was
    // handed -- grepping the output for a code without checking that the call
    // failed counted the shell's own diagnostic machinery as a defect.
    const failed = out.startsWith('error') || out.startsWith('{"error"');
    const m = failed ? out.match(/E_[A-Z_0-9]+/) : null;
    const code =
        out === '\u0000TIMEOUT'
            ? 'HUNG'
            : m
              ? m[0]
              : out.startsWith('error')
                ? 'NO_CODE'
                : 'OK';
    tally.set(code, (tally.get(code) ?? 0) + 1);
    if (code === 'HUNG') hung.push(name);
    if (code === 'E_UNKNOWN' || code === 'NO_CODE') {
        (TOOL_ABSENT.test(out) ? toolAbsent : uncoded).push(name);
        said[name] = out.replace(/\s+/g, ' ').slice(0, 240);
    }
    if (++done % 200 === 0) process.stderr.write(`  ${done}/${names.size}\n`);
}

console.log('\nby code:');
for (const [k, v] of [...tally].sort((a, b) => b[1] - a[1])) {
    console.log(`  ${String(v).padStart(5)}  ${k}`);
}
const all = uncoded.length + toolAbsent.length;
const pct = ((uncoded.length / names.size) * 100).toFixed(1);
console.log(`\nuncoded: ${all} of ${names.size}`);
console.log(`  ${String(uncoded.length).padStart(5)}  the shell's own: argument and type errors (${pct}%)`);
console.log(`  ${String(toolAbsent.length).padStart(5)}  external tool absent on this host`);
console.log('           -- says nothing about their argument handling, though');
console.log('           "tool not found" is itself a knowable condition');
console.log('           currently reported as E_UNKNOWN');
fs.writeFileSync('uncoded.txt', `${uncoded.join('\n')}\n`);
fs.writeFileSync('uncoded-tool-absent.txt', `${toolAbsent.join('\n')}\n`);
if (hung.length) {
    console.log(`\n${hung.length} builtin(s) never returned and were killed at the timeout.`);
    console.log('A call that hangs is worse for an agent than one that fails:');
    console.log(`  ${hung.join(', ')}`);
}
console.log('\nlists written to uncoded.txt and uncoded-tool-absent.txt');

// CI mode. Every count this probe reports as a problem is 0 today, so any of
// them rising is a regression -- including a builtin that hangs, which is
// worse for an agent than one that fails. Exit 1 so the job fails; the
// non-vacuity checks above already exit 2 when the probe measured nothing.
if (process.env.AE_PROBE_ASSERT === '1') {
    const bad = uncoded.length + toolAbsent.length + hung.length;
    if (bad > 0) {
        console.error(
            `\n! ${uncoded.length} uncoded, ${toolAbsent.length} uncoded tool-absent, ` +
                `${hung.length} hung -- all three were 0`,
        );
        for (const n of [...uncoded, ...toolAbsent]) console.error(`    ${n}: ${said[n]}`);
        for (const n of hung) console.error(`    ${n}: never returned`);
        process.exit(1);
    }
    console.log('\nassert: 0 uncoded, 0 hung');
}
