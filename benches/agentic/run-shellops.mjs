// Execute E2: eight ordinary repository operations, four shells, real output.
//
//   node benches/agentic/run-shellops.mjs <repo-root> <outdir> [repeats]
//
// These tasks have no single canonical rendering, so the oracle is a content
// check rather than an equality check: each task names things its answer must
// contain (see EXPECT in shellops.mjs). That is weaker than E1's oracle and it
// is not optional -- without it, an engine that printed a placeholder instead
// of the data scored as the cheapest, which is exactly what happened the first
// time this ran. The full transcript is written to answers.txt so a reader can
// check that the engines were asked the same thing and answered it.

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { ENGINES, COMMANDS, TASKS, EXPECT, selfCheck } from './shellops.mjs';

const repo = process.argv[2];
const out = process.argv[3];
const REPEATS = Number(process.argv[4] ?? 7);
if (!repo || !out) { console.error('usage: run-shellops.mjs <repo-root> <outdir> [repeats]'); process.exit(2); }

const tokDir = path.join(out, 'tok-shellops');
fs.rmSync(tokDir, { recursive: true, force: true });
fs.mkdirSync(tokDir, { recursive: true });

const have = (bin) => {
    const r = spawnSync(bin, ['--version'], { shell: true, stdio: 'ignore' });
    return r.status === 0 || r.status === 1;
};

// The oracle is only worth having if it discriminates; check that before
// trusting anything it says. (The first version of EXPECT was written through
// a script whose escaping ate every backslash, leaving patterns that could
// never match — rigour-shaped and useless.)
const oracleProblems = selfCheck();
if (oracleProblems.length) {
    console.error('the content oracle is broken, refusing to report:');
    for (const problem of oracleProblems) console.error('  ' + problem);
    process.exit(3);
}

const results = [];
const transcript = [];
for (const [engine, spec] of Object.entries(ENGINES)) {
    if (!have(spec.bin)) { console.error(`skip ${engine}: ${spec.bin} not on PATH`); continue; }
    for (const [t, cmd] of Object.entries(COMMANDS[engine])) {
        const argv = spec.argv(cmd);
        spawnSync(spec.bin, argv, { cwd: repo, encoding: 'utf8', maxBuffer: 64 << 20 });

        const times = [];
        let text = null, err = '', code = null, stable = true;
        for (let i = 0; i < REPEATS; i++) {
            const t0 = process.hrtime.bigint();
            const r = spawnSync(spec.bin, argv, { cwd: repo, encoding: 'utf8', maxBuffer: 64 << 20 });
            times.push(Number(process.hrtime.bigint() - t0) / 1e6);
            if (i === 0) { text = r.stdout ?? ''; err = r.stderr ?? ''; code = r.status; }
            else if ((r.stdout ?? '') !== text) stable = false;
        }
        times.sort((a, b) => a - b);

        // Sanitised: the `aethershell (agent)` engine name has a space and
        // parentheses in it, and these basenames are passed on a command line.
        const base = `${engine.replace(/[^a-z0-9]/gi, '_')}.${t}`;
        fs.writeFileSync(path.join(tokDir, `${base}.cmd`), cmd);
        fs.writeFileSync(path.join(tokDir, `${base}.out`), text);
        transcript.push(`### ${t} -- ${TASKS[t]}\n\n$ ${engine}: ${cmd}\n${text}`);

        results.push({
            engine, q: t, question: TASKS[t], command: cmd,
            exit: code,
            correct: code === 0 && (EXPECT[t] ?? []).every((re) => re.test(text)),
            missing: (EXPECT[t] ?? []).filter((re) => !re.test(text)).map(String),
            stable,
            answer: text.replace(/\s+/g, ' ').trim().slice(0, 120),
            expected: (EXPECT[t] ?? []).map(String).join(' + '),
            stdout_bytes: Buffer.byteLength(text),
            stderr_bytes: Buffer.byteLength(err),
            ms_median: +times[Math.floor(times.length / 2)].toFixed(2),
            ms_min: +times[0].toFixed(2), ms_max: +times[times.length - 1].toFixed(2),
            attempts: null, tok_base: base,
        });
        const ok = code === 0 && (EXPECT[t] ?? []).every((re) => re.test(text));
        process.stderr.write(`${ok ? 'ok  ' : 'FAIL'} ${engine.padEnd(20)} ${t}  ` +
            `${times[Math.floor(times.length / 2)].toFixed(1).padStart(8)} ms  ` +
            `${String(Buffer.byteLength(text)).padStart(6)} B${stable ? '' : '  UNSTABLE'}\n`);
    }
}

fs.writeFileSync(path.join(out, 'shellops-results.json'), JSON.stringify({
    generated: new Date().toISOString(), repeats: REPEATS,
    platform: `${process.platform} ${process.arch}`, results,
}, null, 2));
fs.writeFileSync(path.join(out, 'answers.txt'), transcript.join('\n\n'));
const bad = results.filter((r) => !r.correct);
console.error(`\n${results.length} measurements, ${bad.length} whose output does not contain the answer`);
for (const b of bad) {
    console.error(`  ${b.engine} ${b.q}: no match for ${b.missing.join(', ')}`);
    console.error(`    got: ${b.answer.slice(0, 100)}`);
}
console.error('transcript in answers.txt');
