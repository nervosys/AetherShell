// Execute the corpus. Every number this writes comes from a process that
// actually ran: no representative outputs, no estimated timings.
//
//   node benches/agentic/run.mjs <datadir> [repeats]
//
// For each (engine, query) it records wall-clock time over N repeats, the exact
// bytes of stdout and stderr, the exit code, whether the answer matches the
// oracle, and whether the output was byte-identical across all N runs. Command
// and output bytes are written to <datadir>/tok/ so that the repository's own
// `tokens_of` example can count them with real cl100k/o200k BPE -- the token
// figures are never estimated here.

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { ENGINES, COMMANDS, QUESTIONS, ATTEMPTS, normalise } from './corpus.mjs';
import { corpusFacts, oracleFor } from './oracle.mjs';

const data = process.argv[2];
const REPEATS = Number(process.argv[3] ?? 7);
const ORACLE = oracleFor(data);
if (!data) { console.error('usage: run.mjs <datadir> [repeats]'); process.exit(2); }

const tokDir = path.join(data, 'tok');
// A filtered run tops up an existing result set rather than discarding the
// engines it is not re-measuring.
if (!process.env.BENCH_ENGINES) fs.rmSync(tokDir, { recursive: true, force: true });
fs.mkdirSync(tokDir, { recursive: true });

const have = (bin) => {
    const r = spawnSync(bin, ['--version'], { shell: true, stdio: 'ignore' });
    return r.status === 0 || r.status === 1; // sqlite3 --version exits 0; be lenient
};

// BENCH_ENGINES=aethershell,bash+jq restricts the run. `bash+coreutils` spawns
// roughly 2,000 processes per query and dominates the wall clock, so a timing
// re-run of the fast engines is worth being able to ask for on its own.
const only = process.env.BENCH_ENGINES?.split(',').map((s) => s.trim()).filter(Boolean);

const results = [];
for (const [engine, spec] of Object.entries(ENGINES)) {
    if (only && !only.includes(engine)) continue;
    if (!have(spec.bin)) {
        console.error(`skip ${engine}: ${spec.bin} not on PATH`);
        continue;
    }
    for (const [q, cmd] of Object.entries(COMMANDS[engine])) {
        const argv = spec.argv(cmd);
        const times = [];
        let out = null, err = null, code = null, stable = true;

        // One warm-up run, discarded: the first invocation pays for page cache
        // and, on Windows, for Defender's first look at the binary.
        spawnSync(spec.bin, argv, { cwd: data, encoding: 'utf8', shell: false, maxBuffer: 64 << 20 });

        for (let i = 0; i < REPEATS; i++) {
            const t0 = process.hrtime.bigint();
            const r = spawnSync(spec.bin, argv, {
                cwd: data, encoding: 'utf8', shell: false, maxBuffer: 64 << 20,
            });
            const t1 = process.hrtime.bigint();
            times.push(Number(t1 - t0) / 1e6);
            if (i === 0) { out = r.stdout ?? ''; err = r.stderr ?? ''; code = r.status; }
            else if ((r.stdout ?? '') !== out) stable = false;
        }

        times.sort((a, b) => a - b);
        const median = times[Math.floor(times.length / 2)];
        const correct = normalise(out) === normalise(ORACLE[q]);

        const base = `${engine.replace(/[^a-z0-9]/gi, '_')}.${q}`;
        fs.writeFileSync(path.join(tokDir, `${base}.cmd`), cmd);
        fs.writeFileSync(path.join(tokDir, `${base}.out`), out);

        results.push({
            engine, q, question: QUESTIONS[q], command: cmd,
            exit: code, correct, stable,
            answer: normalise(out).slice(0, 120),
            expected: normalise(ORACLE[q]),
            stdout_bytes: Buffer.byteLength(out),
            stderr_bytes: Buffer.byteLength(err),
            ms_median: +median.toFixed(2),
            ms_min: +times[0].toFixed(2),
            ms_max: +times[times.length - 1].toFixed(2),
            attempts: ATTEMPTS[engine]?.[q] ?? null,
            tok_base: base,
        });
        process.stderr.write(
            `${correct ? 'ok  ' : 'FAIL'} ${engine.padEnd(15)} ${q.padEnd(4)} ` +
            `${median.toFixed(1).padStart(8)} ms  ${String(Buffer.byteLength(out)).padStart(6)} B` +
            `${stable ? '' : '  UNSTABLE'}\n`
        );
    }
}

let merged = results;
const resultsPath = path.join(data, 'results.json');
if (only && fs.existsSync(resultsPath)) {
    const prev = JSON.parse(fs.readFileSync(resultsPath, 'utf8')).results ?? [];
    merged = prev.filter((r) => !only.includes(r.engine)).concat(results);
}

fs.writeFileSync(resultsPath, JSON.stringify({
    generated: new Date().toISOString(),
    repeats: REPEATS,
    platform: `${process.platform} ${process.arch}`,
    corpus: corpusFacts(data),
    results: merged,
}, null, 2));

const bad = results.filter((r) => !r.correct);
console.error(`\n${results.length} measurements, ${bad.length} incorrect`);
for (const b of bad) console.error(`  ${b.engine} ${b.q}: got "${b.answer}" want "${b.expected}"`);
