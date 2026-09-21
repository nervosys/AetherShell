// E7 -- does a borrowed syntax beat an invented one?
//
// Protocol: e7-prompts/ + PREREGISTERED_E7.md, including Amendment 1. This file
// implements that protocol and nothing else; it is not the place to decide
// anything the pre-registration left open.
//
//   node benches/agentic/e7.mjs <datadir> --replay          # no model, no spend
//   node benches/agentic/e7.mjs <datadir> --provider <name> [--model M] [--seeds N]
//
// `--replay` is not a dry run. It pushes the *known-good* commands from
// corpus.mjs through the identical prompt-hash, extraction, execution, oracle
// and scoring path, so the harness is proven to score a correct answer correct
// and a wrong one wrong before a single token is bought. A benchmark whose
// scoring has never been exercised is how E2 once scored a placeholder as the
// cheapest correct answer.

import { spawnSync } from 'node:child_process';
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { COMMANDS, ORACLE, QUESTIONS, normalise } from './corpus.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const PROMPTS = path.join(HERE, 'e7-prompts');

const argv = process.argv.slice(2);
const dataDir = argv.find((a) => !a.startsWith('--'));
const flag = (name, dflt) => {
    const i = argv.indexOf('--' + name);
    return i >= 0 && argv[i + 1] && !argv[i + 1].startsWith('--') ? argv[i + 1] : dflt;
};
const has = (name) => argv.includes('--' + name);
if (!dataDir) {
    console.error('usage: e7.mjs <datadir> [--replay | --provider <name>] [--model M] [--seeds N]');
    process.exit(2);
}

// ── arms ────────────────────────────────────────────────────────────────
//
// `run` is how a produced command is executed; `replay` names the corpus entry
// whose known-good command stands in for the model in --replay mode.
const ARMS = [
    { id: 'sql', prompt: 'sql.md', bin: 'sqlite3', run: (c) => ['sqlite3', ['issues.db', c]], replay: 'sqlite' },
    { id: 'jq', prompt: 'jq.md', bin: 'jq', run: (c) => ['bash', ['-c', c]], replay: 'bash+jq' },
    { id: 'aethershell', prompt: 'aethershell.md', bin: 'ae', run: (c) => ['ae', ['-c', c]], replay: 'aethershell' },
    { id: 'agentic', prompt: 'agentic.md', bin: 'ae', run: (c) => ['ae', ['-a', '-c', c]], replay: 'aethershell (agentic)' },
    { id: 'agentic-nocheat', prompt: 'agentic-nocheat.md', bin: 'ae', run: (c) => ['ae', ['-a', '-c', c]], replay: 'aethershell (agentic)' },
];

// An arm whose interpreter is not installed must be SKIPPED and named, never
// scored. Without this the first replay run reported `sql 0/10` on commands
// known to be correct, because this host has no `sqlite3` -- a missing tool
// reading exactly like a model that cannot write SQL. Every comparative number
// in benches/agentic/ is worthless the moment absence and failure look alike.
function available(arm) {
    const r = spawnSync('sh', ['-c', 'command -v ' + arm.bin], { encoding: 'utf8' });
    return r.status === 0 && (r.stdout ?? '').trim().length > 0;
}

// ── the prompts are frozen; prove it ────────────────────────────────────
function promptHash() {
    const h = crypto.createHash('sha256');
    for (const f of fs.readdirSync(PROMPTS).sort()) {
        h.update(f);
        h.update(fs.readFileSync(path.join(PROMPTS, f)));
    }
    return h.digest('hex').slice(0, 16);
}

// The ontology is substituted live, from the shell that is about to be tested,
// rather than pasted into the prompt files -- otherwise the reference material
// and the shell drift apart and the arm is judged on a stale copy.
const WORKING_SET = [
    'cat', 'from_json', 'where', 'map', 'sum', 'len', 'group_by', 'sort_by', 'sort', 'last',
    'first', 'flatten', 'unique', 'any', 'all', 'max', 'min', 'mean', 'round', 'to_string',
    'lower', 'contains',
];
function ontologyText() {
    const lines = [];
    for (const name of WORKING_SET) {
        const r = spawnSync('ae', ['-c', 'ontology_describe("' + name + '")'],
            { encoding: 'utf8', timeout: 20000 });
        const out = (r.stdout ?? '').trim();
        if (!out || out.includes('is not a known')) {
            throw new Error('ontology has no entry for ' + name + ' -- the AetherShell arms would ' +
                'be handed incomplete reference material, which PREREGISTERED_E7.md names as a ' +
                'reason an arm would lose for a fixable rather than a fundamental reason.');
        }
        lines.push(out);
    }
    return lines.join('\n');
}

function buildPrompt(arm, question, onto) {
    const common = fs.readFileSync(path.join(PROMPTS, 'common.md'), 'utf8');
    const armText = fs.readFileSync(path.join(PROMPTS, arm.prompt), 'utf8');
    return armText.replace('{{ONTOLOGY}}', onto) + '\n\n' + common.replace('{{QUESTION}}', question);
}

// ── extraction ──────────────────────────────────────────────────────────
// The prompt asks for exactly one fenced block. Anything else is the model
// failing the output contract, and is scored as a failure rather than repaired:
// the protocol says no retries, and quietly fixing the output would be scoring a
// different experiment from the one that was pre-registered.
export function extractCommand(text) {
    if (typeof text !== 'string') return null;
    const fenced = text.match(/```(?:[a-zA-Z0-9_+-]*)\n([\s\S]*?)```/);
    if (!fenced) return null;
    const body = fenced[1].trim();
    return body.length ? body : null;
}

// ── scoring ─────────────────────────────────────────────────────────────
export function score(arm, command, q, cwd) {
    if (!command) return { correct: false, reason: 'no fenced command' };
    const [bin, args] = arm.run(command);
    const r = spawnSync(bin, args, { cwd, encoding: 'utf8', timeout: 60000 });
    const got = normalise((r.stdout ?? '') + (r.stderr ?? ''));
    const want = normalise(ORACLE[q]);
    return { correct: got === want, got: got.slice(0, 200), want, exit: r.status };
}

// ── model call ──────────────────────────────────────────────────────────
const KEY_FOR = {
    anthropic: 'ANTHROPIC_API_KEY',
    openai: 'OPENAI_API_KEY',
    openrouter: 'OPENROUTER_API_KEY',
};
const ENDPOINT = {
    anthropic: 'https://api.anthropic.com/v1/messages',
    openai: 'https://api.openai.com/v1/chat/completions',
    openrouter: 'https://openrouter.ai/api/v1/chat/completions',
};

function generate(prompt, provider, model, temperature, seed) {
    const key = process.env[KEY_FOR[provider] ?? 'OPENAI_API_KEY'];
    if (!key) throw new Error('no API key for provider "' + provider + '": set ' + KEY_FOR[provider]);
    const anthropic = provider === 'anthropic';
    const body = anthropic
        ? { model, max_tokens: 700, temperature, messages: [{ role: 'user', content: prompt }] }
        : { model, max_tokens: 700, temperature, seed, messages: [{ role: 'user', content: prompt }] };
    const headers = anthropic
        ? ['-H', 'x-api-key: ' + key, '-H', 'anthropic-version: 2023-06-01']
        : ['-H', 'Authorization: Bearer ' + key];
    const r = spawnSync('curl', [
        '-s', '-m', '120', '-X', 'POST', ENDPOINT[provider] ?? ENDPOINT.openai,
        '-H', 'Content-Type: application/json', ...headers, '--data-binary', '@-',
    ], { input: JSON.stringify(body), encoding: 'utf8', timeout: 130000 });
    let j;
    try { j = JSON.parse(r.stdout || '{}'); } catch { throw new Error('unparseable provider response'); }
    if (j.error) throw new Error('provider error: ' + (j.error.message ?? JSON.stringify(j.error)));
    return anthropic ? (j.content?.[0]?.text ?? '') : (j.choices?.[0]?.message?.content ?? '');
}

// ── run ─────────────────────────────────────────────────────────────────
const replay = has('replay');
const provider = replay ? 'replay' : flag('provider', null);
const model = flag('model', provider === 'anthropic' ? 'claude-opus-5' : 'gpt-4o');
const seeds = Number(flag('seeds', '1'));
const temperature = Number(flag('temperature', seeds > 1 ? '0.7' : '0'));

if (!replay && !provider) {
    console.error('refusing to run: pass --provider <anthropic|openai|openrouter>, or --replay\n' +
        'to exercise the harness without calling a model.');
    process.exit(2);
}
if (seeds > 1 && temperature === 0) {
    // Amendment 1 exists because the protocol originally asked for this.
    console.error('refusing to run: ' + seeds + ' seeds at temperature 0 are ' + seeds +
        ' identical generations.\nSee Amendment 1 in PREREGISTERED_E7.md.');
    process.exit(2);
}

// ── the harness must be able to fail ────────────────────────────────────
//
// `--replay` showing 10/10 is consistent with a scorer that returns true for
// everything. This pins the difference before any of it is believed, and it is
// the guard E2 did not have when it scored `[{...}, {...}, ...]` as the
// cheapest correct answer in the whole benchmark.
function selfCheck(cwd) {
    const ae = ARMS.find((a) => a.id === 'aethershell');
    const cases = [
        ['a known-good command', COMMANDS.aethershell.q2, true],
        ['a wrong but valid command', 'cat("issues.json") | from_json | len', false],
        ['a command that errors', 'this_is_not_a_builtin()', false],
        ['an empty answer', 'echo("")', false],
        ['a plausible placeholder', 'echo("[{...}, {...}, ...]")', false],
    ];
    for (const [label, cmd, want] of cases) {
        const got = score(ae, cmd, 'q2', cwd).correct;
        if (got !== want) {
            console.error('SELF-CHECK FAILED: ' + label + ' scored ' + got + ', expected ' + want +
                '\nThe scorer cannot tell right from wrong, so no number it produces means anything.');
            process.exit(3);
        }
    }
    // ...and extraction must reject output that breaks the contract.
    for (const [label, text, want] of [
        ['a fenced block', '```sql\nSELECT 1;\n```', 'SELECT 1;'],
        ['prose with no fence', 'The answer is SELECT 1;', null],
        ['an empty fence', '```\n\n```', null],
    ]) {
        if (extractCommand(text) !== want) {
            console.error('SELF-CHECK FAILED: extraction of ' + label);
            process.exit(3);
        }
    }
    console.log('self-check: scorer distinguishes correct from incorrect, extraction honours the contract');
}

const hash = promptHash();
selfCheck(dataDir);
const onto = ontologyText();
console.log('E7  prompts=' + hash + '  provider=' + provider + '  model=' + (replay ? '-' : model) +
    '  temp=' + temperature + '  seeds=' + seeds);
if (replay) {
    console.log('REPLAY: known-good corpus commands stand in for the model.');
    console.log('        This scores the harness, not a model.');
}

// Arm order is randomised per question, per the protocol.
const shuffle = (a) => a.map((v) => [Math.random(), v]).sort((x, y) => x[0] - y[0]).map(([, v]) => v);

const ACTIVE = ARMS.filter(available);
const MISSING = ARMS.filter((a) => !available(a));
if (MISSING.length) {
    console.log('skipped, not scored (interpreter absent): ' +
        MISSING.map((a) => a.id + ' (needs ' + a.bin + ')').join(', '));
}
if (!ACTIVE.length) {
    console.error('no arm can run on this host; refusing to report a table of zeros.');
    process.exit(2);
}

const rows = [];
for (const [q, question] of Object.entries(QUESTIONS)) {
    for (const arm of shuffle([...ACTIVE])) {
        for (let seed = 0; seed < seeds; seed++) {
            let command = null;
            let error = null;
            try {
                command = replay
                    ? COMMANDS[arm.replay][q]
                    : extractCommand(generate(buildPrompt(arm, question, onto), provider, model, temperature, seed));
            } catch (e) {
                error = e.message;
            }
            const s = command
                ? score(arm, command, q, dataDir)
                : { correct: false, reason: error ?? 'no command' };
            rows.push({ q, arm: arm.id, seed, command, ...s });
        }
    }
    process.stdout.write(q + ' ');
}
console.log('');

const outFile = path.join(dataDir, 'e7-' + hash + '.json');
fs.writeFileSync(outFile, JSON.stringify(
    { prompts: hash, provider, model, temperature, seeds,
        skipped: MISSING.map((a) => ({ arm: a.id, needs: a.bin })), rows }, null, 2));

const pad = (s, n) => String(s).padEnd(n);
console.log('');
console.log(pad('arm', 22) + 'first-try correct');
for (const arm of ARMS) {
    const rs = rows.filter((r) => r.arm === arm.id);
    console.log(pad(arm.id, 22) + (rs.length
        ? rs.filter((r) => r.correct).length + '/' + rs.length
        : 'skipped (' + arm.bin + ' not installed)'));
}
console.log('');
console.log('results: ' + outFile);
