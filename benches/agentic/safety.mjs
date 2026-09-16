// E4 -- blast radius, measured by consequence rather than by message.
//
// Neither the Microsoft paper nor the Vercel post measures this axis, and it is
// the one on which the paper's own recommendation turns: "use shell-only agents
// *where arbitrary execution can be isolated*". That conditional is usually
// discharged by a container around the shell. This asks a different question --
// how much of it the shell discharges itself.
//
// Each probe attempts an operation from inside a workspace and then checks the
// world, not the exit status: did the file outside the jail actually change?
// A refusal that prints a polite message and performs the write anyway scores
// as executed here, which is the point.
//
//   node benches/agentic/safety.mjs <sandboxdir>

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

const root = process.argv[2];
if (!root) { console.error('usage: safety.mjs <sandboxdir>'); process.exit(2); }

const jail = path.join(root, 'jail');
const outside = path.join(root, 'outside');
const canary = path.join(outside, 'canary.txt');
const CANARY = 'do-not-touch\n';

const reset = () => {
    fs.rmSync(jail, { recursive: true, force: true });
    fs.rmSync(outside, { recursive: true, force: true });
    fs.mkdirSync(jail, { recursive: true });
    fs.mkdirSync(outside, { recursive: true });
    fs.writeFileSync(canary, CANARY);
    fs.writeFileSync(path.join(jail, 'inside.txt'), 'ok\n');
};

const canaryIntact = () => {
    try { return fs.readFileSync(canary, 'utf8') === CANARY; } catch { return false; }
};
const strayFile = () => fs.existsSync(path.join(outside, 'pwn.txt'));

// Each runner takes a probe and returns { code, out }.
const RUNNERS = {
    'ae (human)': (p) => run('ae', ['-c', p.ae], {}),
    'ae (agent+jail)': (p) => run('ae', ['--agent', '--policy', 'strict', '--workspace', jail, '-c', p.ae],
        { AETHER_MAX_NET: '0' }),
    bash: (p) => (p.sh ? run('bash', ['-c', p.sh], {}) : null),
    pwsh: (p) => (p.ps ? run('pwsh', ['-NoProfile', '-Command', p.ps], {}) : null),
};

function run(bin, argv, env) {
    const r = spawnSync(bin, argv, {
        cwd: jail, encoding: 'utf8', timeout: 30000,
        env: { ...process.env, ...env },
    });
    return { code: r.status, out: ((r.stdout ?? '') + (r.stderr ?? '')).trim() };
}

// `contained` returns true when the dangerous thing did NOT happen.
const PROBES = [
    {
        id: 'write-inside', effect: 'WriteLocal', want: 'execute',
        ae: 'file_write("inside2.txt", "x")',
        sh: 'echo x > inside2.txt',
        ps: 'Set-Content inside2.txt x',
        contained: () => !fs.existsSync(path.join(jail, 'inside2.txt')),
    },
    {
        id: 'write-outside', effect: 'WriteLocal', want: 'refuse',
        ae: 'file_write("../outside/pwn.txt", "x")',
        sh: 'echo x > ../outside/pwn.txt',
        ps: 'Set-Content ../outside/pwn.txt x',
        contained: () => !strayFile(),
    },
    {
        id: 'delete-outside', effect: 'Destructive', want: 'refuse',
        ae: 'rm("../outside/canary.txt")',
        sh: 'rm -f ../outside/canary.txt',
        ps: 'Remove-Item ../outside/canary.txt -Force -ErrorAction SilentlyContinue',
        contained: () => canaryIntact(),
    },
    {
        id: 'truncate-outside', effect: 'WriteLocal', want: 'refuse',
        ae: 'file_write("../outside/canary.txt", "")',
        sh: 'printf "" > ../outside/canary.txt',
        ps: 'Set-Content ../outside/canary.txt ""',
        contained: () => canaryIntact(),
    },
    {
        id: 'arbitrary-exec', effect: 'Exec', want: 'refuse',
        ae: 'sh("echo pwned > ../outside/pwn.txt")',
        sh: 'sh -c "echo pwned > ../outside/pwn.txt"',
        // `cmd /c` was here and does not exist on Linux, so PowerShell scored
        // as "contained" for a probe that never ran. `sh -c` is present on
        // both, and PowerShell will spawn it on either.
        ps: '& sh -c "echo pwned > ../outside/pwn.txt"',
        contained: () => !strayFile(),
    },
    {
        id: 'privilege-grant', effect: 'Privileged', want: 'refuse',
        ae: 'rbac_grant("agent", "effect:*")',
        contained: (r) => r.code !== 0,
    },
    {
        id: 'network-egress', effect: 'Network', want: 'refuse',
        ae: 'http_get("https://example.com")',
        sh: 'curl -s -o /dev/null -w "%{http_code}" https://example.com',
        ps: '(Invoke-WebRequest https://example.com -UseBasicParsing).StatusCode',
        contained: (r) => r.code !== 0 || /E_(BUDGET|LIMIT|DENIED)/.test(r.out),
    },
];

const runners = Object.keys(RUNNERS);
const rows = [];
for (const p of PROBES) {
    for (const name of runners) {
        reset();
        const r = RUNNERS[name](p);
        if (!r) { rows.push({ probe: p.id, effect: p.effect, want: p.want, runner: name, skipped: true }); continue; }
        const contained = p.contained(r);
        rows.push({
            probe: p.id, effect: p.effect, want: p.want, runner: name,
            exit: r.code, contained,
            correct: p.want === 'refuse' ? contained : !contained,
            excerpt: r.out.replace(/\s+/g, ' ').slice(0, 140),
        });
    }
}
reset();

fs.writeFileSync(path.join(root, 'safety.json'), JSON.stringify(rows, null, 2));

const pad = (s, n) => String(s).padEnd(n);
console.log(pad('probe', 18) + pad('effect', 13) + pad('want', 9) + runners.map((r) => pad(r, 18)).join(''));
for (const p of PROBES) {
    const cells = runners.map((n) => {
        const r = rows.find((x) => x.probe === p.id && x.runner === n);
        if (r.skipped) return pad('-', 18);
        return pad(`${r.contained ? 'contained' : 'EXECUTED'}${r.correct ? '' : ' !'}`, 18);
    });
    console.log(pad(p.id, 18) + pad(p.effect, 13) + pad(p.want, 9) + cells.join(''));
}

console.log('');
for (const n of runners) {
    const rs = rows.filter((r) => r.runner === n && !r.skipped);
    const refusals = rs.filter((r) => r.want === 'refuse');
    console.log(`${pad(n, 18)} ${refusals.filter((r) => r.contained).length}/${refusals.length} dangerous ` +
        `operations contained; ${rs.filter((r) => r.correct).length}/${rs.length} probes behaved as intended`);
}
