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
//     507  accepted it and answered
//     285  E_BAD_ARG
//     142  E_NEEDS_APPROVAL   (the gate, correctly)
//      56  E_UNKNOWN
//      42  E_TOOL_MISSING
//      12  E_UNKNOWN_BUILTIN
//       4  E_POLICY_DENY
//       3  E_PARSE
//       1  varies between runs -- a builtin whose answer depends on the clock
//            or the filesystem. Worth knowing before reading a one-builtin
//            change as signal.
//
// The 56 still-uncoded split two ways, and the split is the point:
//
//      41  the shell's own argument and type errors -- 3.9% of the catalogue
//      15  an external tool absent on THIS host, still uncoded because the
//            builtin propagates a bare io::Error with no context -- not even
//            the tool's name survives, so there is nothing to convert.
//
// How it got here, and why the edit count is the wrong number to quote:
//
//   157  before any of this
//   156  declaring `cat`                              1 builtin moved
//   140   54 ad-hoc type errors -> bad_arg           16 builtins moved
//    98  108 tool-not-found sites -> E_TOOL_MISSING  42 builtins moved
//    56  171 argument errors -> arg_err              42 builtins moved
//
// 334 edits, 101 builtins moved: about 30%, consistently, because most
// converted sites sit behind an earlier failure path this probe never
// reaches. Quote what the sweep reports.
//
// Three regex passes were each quietly incomplete, and none of the gaps was
// visible from the edit side:
//   * `a2a.register` -- a dot in the builtin label
//   * `must be integer` -- no article, where the pattern assumed one
//   * a multi-line `anyhow!` call -- rustfmt had wrapped the message onto its
//     own line, which hid 74 sites, nearly as many as the single-line pass
//     had found
// Each surfaced only by asking the running shell about a builtin that was
// supposed to be fixed and was not.
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
        env: { ...process.env, AETHER_MAX_NET: '0' },
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
const TOOL_ABSENT = /not found: No such file|No such file or directory \(os error 2\)/;

const tally = new Map();
const uncoded = [];
const toolAbsent = [];
const hung = [];
let done = 0;
for (const name of [...names].sort()) {
    const out = run(`${name}({unexpected: true})`);
    const m = out.match(/E_[A-Z_0-9]+/);
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
