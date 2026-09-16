// Turn results.json into markdown, with exact token counts.
//
// Token counts are not computed here. The command and output bytes written by
// run.mjs are handed to this repository's own `tokens_of` example, built with
// `--features real-tokens`, which counts them with the real cl100k_base and
// o200k_base BPE. If that build is unavailable the report says so rather than
// substituting a heuristic and letting the reader assume otherwise.
//
//   node benches/agentic/report.mjs <datadir> [--no-tokens]

import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

const data = process.argv[2];
const noTokens = process.argv.includes('--no-tokens');
if (!data) { console.error('usage: report.mjs <datadir> [--no-tokens]'); process.exit(2); }

// E2 writes its results alongside E1's; `--shellops` reports on those instead.
const shellops = process.argv.includes('--shellops');
const db = JSON.parse(fs.readFileSync(
    path.join(data, shellops ? 'shellops-results.json' : 'results.json'), 'utf8'));
const tokDir = path.join(data, shellops ? 'tok-shellops' : 'tok');

let tokens = {};
let exact = false;
if (!noTokens) {
    // Absolute paths for 120 files overflow the Windows 32 KB command line, so
    // pass basenames and let the child resolve them from the token directory.
    const files = fs.readdirSync(tokDir);
    const repoRoot = path.resolve(path.dirname(new URL(import.meta.url).pathname.replace(/^\//, '')), '..', '..');
    for (let i = 0; i < files.length; i += 20) {
        const out = execFileSync('cargo', [
            'run', '-q', '--manifest-path', path.join(repoRoot, 'Cargo.toml'),
            '-p', 'agentic-eval', '--example', 'tokens_of',
            '--features', 'agentic-eval/real-tokens', '--', ...files.slice(i, i + 20),
        ], { encoding: 'utf8', maxBuffer: 64 << 20, cwd: tokDir });
        for (const line of out.split('\n')) {
            if (line.startsWith('tokenizer exact:')) exact = line.includes('cl100k=true');
            const m = line.match(/^\s*(\d+)\s+(\d+)\s+(.+?)\s*$/);
            if (m) tokens[path.basename(m[3])] = { cl100k: +m[1], o200k: +m[2] };
        }
    }
}

const tok = (base, ext) => tokens[`${base}.${ext}`]?.cl100k ?? null;

const rows = db.results.map((r) => ({
    ...r,
    cmd_tok: tok(r.tok_base, 'cmd'),
    out_tok: tok(r.tok_base, 'out'),
    total_tok: tok(r.tok_base, 'cmd') != null ? tok(r.tok_base, 'cmd') + tok(r.tok_base, 'out') : null,
}));

const engines = [...new Set(rows.map((r) => r.engine))];
const sum = (xs) => xs.reduce((a, b) => a + b, 0);
const median = (xs) => { const s = [...xs].sort((a, b) => a - b); return s[Math.floor(s.length / 2)]; };

const L = [];
L.push(`# Agentic shell benchmark\n`);
L.push(`Generated ${db.generated} on ${db.platform}, median of ${db.repeats} runs after one discarded warm-up.`);
L.push(`Token counts: ${exact ? '**exact** cl100k_base BPE via tiktoken' : noTokens ? 'not computed' : '**UNAVAILABLE -- real-tokens build failed**'}.\n`);

L.push(`## Per-engine totals\n`);
L.push(`| Engine | Correct | Byte-stable | Cmd tokens | Output tokens | Total tokens | Median ms | Total ms |`);
L.push(`| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |`);
for (const e of engines) {
    const rs = rows.filter((r) => r.engine === e);
    const t = rs.filter((r) => r.total_tok != null);
    L.push(`| ${e} | ${rs.filter((r) => r.correct).length}/${rs.length} ` +
        `| ${rs.filter((r) => r.stable).length}/${rs.length} ` +
        `| ${sum(t.map((r) => r.cmd_tok))} | ${sum(t.map((r) => r.out_tok))} | ${sum(t.map((r) => r.total_tok))} ` +
        `| ${median(rs.map((r) => r.ms_median)).toFixed(1)} | ${sum(rs.map((r) => r.ms_median)).toFixed(0)} |`);
}

L.push(`\n## Per-task total tokens (command + output)\n`);
const qs = [...new Set(rows.map((r) => r.q))];
L.push(`| Task | ${engines.join(' | ')} |`);
L.push(`| --- | ${engines.map(() => '---:').join(' | ')} |`);
for (const q of qs) {
    const cells = engines.map((e) => {
        const r = rows.find((x) => x.engine === e && x.q === q);
        if (!r) return '--';
        return `${r.total_tok ?? '?'}${r.correct ? '' : ' ✗'}`;
    });
    L.push(`| ${q} | ${cells.join(' | ')} |`);
}

L.push(`\n## Per-task wall clock, median ms\n`);
L.push(`| Task | ${engines.join(' | ')} |`);
L.push(`| --- | ${engines.map(() => '---:').join(' | ')} |`);
for (const q of qs) {
    const cells = engines.map((e) => {
        const r = rows.find((x) => x.engine === e && x.q === q);
        return r ? r.ms_median.toFixed(1) : '--';
    });
    L.push(`| ${q} | ${cells.join(' | ')} |`);
}

const unstable = rows.filter((r) => !r.stable);
L.push(`\n## Determinism\n`);
L.push(unstable.length === 0
    ? `Every engine produced byte-identical output across all ${db.repeats} runs on this corpus.`
    : `${unstable.length} of ${rows.length} measurements produced output that changed between runs:\n\n` +
      unstable.map((r) => `- \`${r.engine}\` ${r.q}`).join('\n'));

const wrong = rows.filter((r) => !r.correct);
if (wrong.length) {
    L.push(`\n## Incorrect answers\n`);
    for (const r of wrong) L.push(`- \`${r.engine}\` ${r.q}: got \`${r.answer}\`, expected \`${r.expected}\``);
}

fs.writeFileSync(path.join(data, shellops ? 'report-shellops.md' : 'report.md'), L.join('\n') + '\n');
fs.writeFileSync(path.join(data, shellops ? 'results-tokens-shellops.json' : 'results-tokens.json'), JSON.stringify({ exact, rows }, null, 2));
console.log(L.join('\n'));
