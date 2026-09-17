// E5 -- does the same command give the same bytes on a different machine?
//
// E1 measured determinism by running each command eleven times in one process
// environment, and every engine was byte-stable. That result is real but weak:
// nothing varied except the clock. An agent's output is not re-read in the
// environment that produced it. It is cached, diffed against a previous run,
// compared across a fleet, or replayed in CI, and the environments differ in
// locale, timezone, terminal width and colour support.
//
// This runs one command per engine under six environments that a real fleet
// actually spans, and asks whether the bytes are identical. A shell whose
// output changes with LC_ALL cannot be diffed across two developers' machines;
// one whose table width follows COLUMNS cannot be diffed across two terminals.
//
//   node benches/agentic/environment.mjs <repo-root>

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

const repo = process.argv[2];
if (!repo) { console.error('usage: environment.mjs <repo-root>'); process.exit(2); }

// The listing task from E2: an array of records, the shape an agent re-reads
// most and the one with the most room to vary.
const ENGINES = {
    aethershell: { bin: 'ae', argv: ['-c', 'ls("src") | where(fn(f) => ends_with(f.name, ".rs")) | pick("name", "size")'] },
    'aethershell (agent)': { bin: 'ae', argv: ['--agent', '-c', 'ls("src") | where(fn(f) => ends_with(f.name, ".rs")) | pick("name", "size")'] },
    bash: { bin: 'bash', argv: ['-c', 'ls -l src/*.rs'] },
    pwsh: { bin: 'pwsh', argv: ['-NoProfile', '-Command', 'Get-ChildItem src/*.rs | Select-Object Name,Length'] },
    nushell: { bin: 'nu', argv: ['-n', '-c', 'ls src/*.rs | select name size'] },
};

// Each is a plausible machine in a fleet, not a contrived one.
const ENVIRONMENTS = {
    'baseline (C locale, UTC, 80 cols)':
        { LC_ALL: 'C', LANG: 'C', TZ: 'UTC', COLUMNS: '80' },
    'German locale':
        { LC_ALL: 'de_DE.UTF-8', LANG: 'de_DE.UTF-8', TZ: 'UTC', COLUMNS: '80' },
    'Japanese locale':
        { LC_ALL: 'ja_JP.UTF-8', LANG: 'ja_JP.UTF-8', TZ: 'UTC', COLUMNS: '80' },
    'Tokyo timezone':
        { LC_ALL: 'C', LANG: 'C', TZ: 'Asia/Tokyo', COLUMNS: '80' },
    'narrow terminal (60 cols)':
        { LC_ALL: 'C', LANG: 'C', TZ: 'UTC', COLUMNS: '60' },
    'wide terminal (200 cols)':
        { LC_ALL: 'C', LANG: 'C', TZ: 'UTC', COLUMNS: '200' },
};

const have = (bin) => {
    const r = spawnSync(bin, ['--version'], { shell: true, stdio: 'ignore' });
    return r.status === 0 || r.status === 1;
};

const rows = [];
for (const [engine, spec] of Object.entries(ENGINES)) {
    if (!have(spec.bin)) { console.error(`skip ${engine}: ${spec.bin} not on PATH`); continue; }

    const outputs = {};
    for (const [label, env] of Object.entries(ENVIRONMENTS)) {
        const r = spawnSync(spec.bin, spec.argv, {
            cwd: repo, encoding: 'utf8', maxBuffer: 64 << 20,
            // A fresh, minimal environment: inheriting the parent's would let
            // the parent's locale leak in and mask exactly what is being tested.
            env: { ...process.env, ...env },
        });
        outputs[label] = r.stdout ?? '';
    }

    const baseline = outputs['baseline (C locale, UTC, 80 cols)'];
    const differing = Object.entries(outputs)
        .filter(([label, text]) => label !== 'baseline (C locale, UTC, 80 cols)' && text !== baseline)
        .map(([label]) => label);

    rows.push({
        engine,
        distinct: new Set(Object.values(outputs)).size,
        differing,
        baseline_bytes: Buffer.byteLength(baseline),
        samples: Object.fromEntries(
            Object.entries(outputs).map(([k, v]) => [k, v.split('\n')[0].slice(0, 90)])
        ),
    });
}

fs.writeFileSync(path.join(repo, 'environment.json'), JSON.stringify(rows, null, 2));

const pad = (s, n) => String(s).padEnd(n);
console.log(pad('engine', 22) + pad('distinct outputs', 18) + 'varies with');
for (const r of rows) {
    console.log(
        pad(r.engine, 22) +
        pad(`${r.distinct} of ${Object.keys(ENVIRONMENTS).length}`, 18) +
        (r.differing.length ? r.differing.join(', ') : 'nothing')
    );
}

console.log('\nFirst line of output, per environment, for any engine that varied:');
for (const r of rows.filter((x) => x.distinct > 1)) {
    console.log(`\n  ${r.engine}`);
    for (const [label, line] of Object.entries(r.samples)) {
        console.log(`    ${pad(label, 34)} ${line}`);
    }
}
