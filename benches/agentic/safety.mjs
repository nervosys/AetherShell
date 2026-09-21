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

import { spawnSync, spawn } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

const root = process.argv[2];
if (!root) { console.error('usage: safety.mjs <sandboxdir>'); process.exit(2); }

const jail = path.join(root, 'jail');
const outside = path.join(root, 'outside');
const canary = path.join(outside, 'canary.txt');
const CANARY = 'do-not-touch\n';

// Emptied in place rather than removed and recreated. One arm is a long-lived
// server whose working directory *is* the jail: remove that directory and the
// process keeps a handle on a deleted inode, `current_dir()` starts failing,
// and the workspace root silently becomes `.` -- which is to say the jail under
// test would be decided by this harness's cleanup rather than by the shell.
const emptyDir = (d) => {
    fs.mkdirSync(d, { recursive: true });
    for (const e of fs.readdirSync(d)) fs.rmSync(path.join(d, e), { recursive: true, force: true });
};
const reset = () => {
    emptyDir(jail);
    emptyDir(outside);
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
    // The surface an agent actually drives, with the flags an operator actually
    // types -- i.e. none. This arm exists because the two above did not cover
    // `ae agent serve`, and the server turned out to run the *human* profile by
    // default. That was invisible for a second reason: /api/v1/eval could not
    // resolve a module namespace at all, so every probe came back "contained"
    // because the call had never run. A benchmark arm whose subject is broken
    // scores a perfect containment result, which is worth remembering.
    'ae (agent serve, no flags)': (p) => viaApi(p.ae),
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

// One server for the whole run, started in the jail with no flags at all --
// which is the condition under test.
const API_PORT = 3402;
let SERVER; // undefined = not tried yet, null = unavailable, else { child, token }

function startServer() {
    // The banner goes to a file, not to a pipe. This function spin-waits
    // synchronously to keep the harness single-threaded, and a synchronous loop
    // starves Node's event loop -- so a `child.stdout.on('data')` handler would
    // never run and the token would never arrive. Cost me one confusing
    // "did not come up".
    const log = path.join(root, 'agent-serve.log');
    fs.writeFileSync(log, '');
    const fd = fs.openSync(log, 'a');
    const child = spawn('ae', ['agent', 'serve', '--port', String(API_PORT)], {
        cwd: jail, stdio: ['ignore', fd, fd], env: { ...process.env },
    });

    const deadline = Date.now() + 30000;
    while (Date.now() < deadline) {
        const banner = fs.readFileSync(log, 'utf8');
        const token = banner.match(/Auth token \(generated\): (\S+)/)?.[1];
        if (token) {
            const health = spawnSync('curl', ['-s', '-m', '1', `http://127.0.0.1:${API_PORT}/health`],
                { encoding: 'utf8' });
            if ((health.stdout ?? '').includes('healthy')) return { child, token };
        }
        spawnSync('sleep', ['0.25']);
    }
    child.kill();
    console.error(`! \`ae agent serve\` did not come up; its arm is skipped, not scored (see ${log})`);
    return null;
}

// `curl` under spawnSync, so this file stays synchronous like the rest of the
// harness.
function viaApi(code) {
    if (SERVER === undefined) SERVER = startServer();
    if (SERVER === null) return null;
    const r = spawnSync('curl', [
        '-s', '-m', '10', '-X', 'POST', `http://127.0.0.1:${API_PORT}/api/v1/eval`,
        '-H', 'Content-Type: application/json',
        '-H', `Authorization: Bearer ${SERVER.token}`,
        '-d', JSON.stringify({ code }),
    ], { encoding: 'utf8', timeout: 15000 });
    const out = ((r.stdout ?? '') + (r.stderr ?? '')).trim();
    // The endpoint answers HTTP 200 with {"success":false,...} for a refused
    // call, so curl's exit status says nothing. `contained` reads the filesystem
    // regardless; this is only for the transcript.
    let refused = false;
    try { refused = JSON.parse(out).success === false; } catch { /* leave 0 */ }
    return { code: refused ? 1 : 0, out };
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
if (SERVER) SERVER.child.kill();

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
