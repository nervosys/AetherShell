// Execute E2: eight ordinary repository operations, four shells, real output.
//
//   node benches/agentic/run-shellops.mjs <repo-root> <outdir> [repeats]
//
// Unlike E1 there is no oracle: these tasks have no single canonical rendering,
// and the question being asked is what a turn *costs*, not who is right. Every
// answer was read once by hand and is recorded in answers.txt next to the
// results so a reader can check that the engines were asked the same thing.

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { ENGINES, COMMANDS, TASKS } from './shellops.mjs';

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

        const base = `${engine}.${t}`;
        fs.writeFileSync(path.join(tokDir, `${base}.cmd`), cmd);
        fs.writeFileSync(path.join(tokDir, `${base}.out`), text);
        transcript.push(`### ${t} -- ${TASKS[t]}\n\n$ ${engine}: ${cmd}\n${text}`);

        results.push({
            engine, q: t, question: TASKS[t], command: cmd,
            exit: code, correct: code === 0 && text.trim().length > 0, stable,
            answer: text.replace(/\s+/g, ' ').trim().slice(0, 120),
            expected: '(no oracle -- see answers.txt)',
            stdout_bytes: Buffer.byteLength(text),
            stderr_bytes: Buffer.byteLength(err),
            ms_median: +times[Math.floor(times.length / 2)].toFixed(2),
            ms_min: +times[0].toFixed(2), ms_max: +times[times.length - 1].toFixed(2),
            attempts: null, tok_base: base,
        });
        process.stderr.write(`${code === 0 ? 'ok  ' : 'FAIL'} ${engine.padEnd(12)} ${t}  ` +
            `${times[Math.floor(times.length / 2)].toFixed(1).padStart(8)} ms  ` +
            `${String(Buffer.byteLength(text)).padStart(6)} B${stable ? '' : '  UNSTABLE'}\n`);
    }
}

fs.writeFileSync(path.join(out, 'shellops-results.json'), JSON.stringify({
    generated: new Date().toISOString(), repeats: REPEATS,
    platform: `${process.platform} ${process.arch}`, results,
}, null, 2));
fs.writeFileSync(path.join(out, 'answers.txt'), transcript.join('\n\n'));
console.error(`\n${results.length} measurements; transcript in answers.txt`);
