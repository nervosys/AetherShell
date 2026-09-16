// E3 -- what a failure costs and what it tells you.
//
// An agent's total token bill is dominated by turns it did not intend to take.
// A failure it can branch on costs one retry; a failure it has to interpret
// costs a read, a guess, and often a second wrong guess. This measures the
// difference on ten failures an agent actually causes.
//
// Three properties per failure, all checked against the bytes the shell
// emitted:
//   distinct exit  -- is the status something other than the generic 1?
//   machine code   -- is there a stable identifier (E_UNKNOWN_BUILTIN, ...)
//                     that a handler can switch on without parsing prose?
//   repair hint    -- does the message name a next action or a candidate?
//
//   node benches/agentic/errors.mjs <sandboxdir>

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

const root = process.argv[2];
if (!root) { console.error('usage: errors.mjs <sandboxdir>'); process.exit(2); }
const cwd = path.join(root, 'errdir');
fs.rmSync(cwd, { recursive: true, force: true });
fs.mkdirSync(cwd, { recursive: true });
fs.writeFileSync(path.join(cwd, 'data.json'), '{"a": 1, "b": [1,2,3]}');
fs.writeFileSync(path.join(cwd, 'broken.json'), '{"a": 1,');

const ENGINES = {
    aethershell: { bin: 'ae', argv: (c) => ['-c', c] },
    bash: { bin: 'bash', argv: (c) => ['-c', c] },
    pwsh: { bin: 'pwsh', argv: (c) => ['-NoProfile', '-Command', c] },
    nushell: { bin: 'nu', argv: (c) => ['-n', '-c', c] },
};

// Each case is the same mistake expressed in each language.
const CASES = [
    { id: 'misspelled-builtin',
      aethershell: 'lenght([1,2,3])', bash: 'lenght 1 2 3',
      pwsh: 'Get-ChildItemz', nushell: 'lenght [1 2 3]' },
    { id: 'missing-file',
      aethershell: 'cat("nope.json")', bash: 'cat nope.json',
      pwsh: 'Get-Content nope.json', nushell: 'open nope.json' },
    { id: 'malformed-json',
      aethershell: 'cat("broken.json") | from_json', bash: 'cat broken.json | jq .',
      pwsh: 'Get-Content broken.json -Raw | ConvertFrom-Json', nushell: 'open broken.json' },
    { id: 'wrong-arg-type',
      aethershell: 'contains([1,2,3], "x")', bash: 'expr abc + 1',
      pwsh: '[int]"abc"', nushell: '"abc" + 1' },
    { id: 'missing-field',
      aethershell: 'cat("data.json") | from_json | get("nosuch")',
      bash: 'jq -e .nosuch data.json', pwsh: '(Get-Content data.json -Raw | ConvertFrom-Json).nosuch.deeper',
      nushell: 'open data.json | get nosuch' },
    { id: 'syntax-error',
      aethershell: '[1,2,3] | map(fn(x) => x *', bash: 'for i in 1 2 3; do echo $i',
      pwsh: 'if ($true) {', nushell: '[1 2 3] | each {|x| $x *' },
    // `max([1,2,3])` stood here until the aggregate form was implemented, at
    // which point it stopped being an error. `starts_with` genuinely needs two.
    { id: 'wrong-arity',
      aethershell: 'starts_with("abc")', bash: 'basename',
      pwsh: 'Join-Path', nushell: 'str substring' },
    { id: 'divide-by-zero',
      aethershell: '1 / 0', bash: 'echo $((1/0))',
      pwsh: '1/0', nushell: '1 / 0' },
    { id: 'permission-or-path',
      aethershell: 'cat("/root/.ssh/id_rsa")', bash: 'cat /root/.ssh/id_rsa',
      pwsh: 'Get-Content /root/.ssh/id_rsa', nushell: 'open /root/.ssh/id_rsa' },
    { id: 'index-out-of-range',
      aethershell: '[1,2,3] | nth(99)', bash: 'a=(1 2 3); echo ${a[99]:?out of range}',
      pwsh: '@(1,2,3)[99].Length', nushell: '[1 2 3] | get 99' },
];

// A machine code is a stable, greppable identifier -- not a sentence.
const CODE = /\b(E_[A-Z][A-Z0-9_]+|nu::[a-z_:]+|[A-Za-z]+Exception|CommandNotFoundException|error\[[A-Za-z0-9_]+\])/;
// A repair hint names a next action or a candidate spelling.
const HINT = /\b(hint|did you mean|help|suggestion|try |instead|expected|Suggestion)\b/i;

const rows = [];
for (const [engine, spec] of Object.entries(ENGINES)) {
    for (const c of CASES) {
        const cmd = c[engine];
        if (!cmd) continue;
        const r = spawnSync(spec.bin, spec.argv(cmd), { cwd, encoding: 'utf8', timeout: 30000 });
        const text = ((r.stdout ?? '') + (r.stderr ?? '')).trim();
        rows.push({
            engine, case: c.id, command: cmd,
            exit: r.status,
            failed: r.status !== 0,
            distinct_exit: r.status !== 0 && r.status !== 1,
            machine_code: CODE.test(text),
            repair_hint: HINT.test(text),
            bytes: Buffer.byteLength(text),
            lines: text ? text.split('\n').length : 0,
            excerpt: text.replace(/\s+/g, ' ').slice(0, 160),
        });
    }
}

fs.writeFileSync(path.join(root, 'errors.json'), JSON.stringify(rows, null, 2));

const pad = (s, n) => String(s).padEnd(n);
console.log(pad('engine', 14) + pad('failed', 9) + pad('machine code', 15) +
    pad('repair hint', 14) + pad('distinct exit', 15) + 'mean bytes');
for (const e of Object.keys(ENGINES)) {
    const rs = rows.filter((r) => r.engine === e);
    const n = rs.length;
    const mean = Math.round(rs.reduce((a, r) => a + r.bytes, 0) / n);
    console.log(pad(e, 14) + pad(`${rs.filter((r) => r.failed).length}/${n}`, 9) +
        pad(`${rs.filter((r) => r.machine_code).length}/${n}`, 15) +
        pad(`${rs.filter((r) => r.repair_hint).length}/${n}`, 14) +
        pad(`${rs.filter((r) => r.distinct_exit).length}/${n}`, 15) + mean);
}

const silent = rows.filter((r) => !r.failed);
if (silent.length) {
    console.log('\nFailures that exited 0 -- an agent checking status alone sees success:');
    for (const r of silent) console.log(`  ${r.engine} ${r.case}: ${r.excerpt.slice(0, 80)}`);
}
