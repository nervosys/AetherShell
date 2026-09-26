// E3f -- network egress under a zero network budget.
//
// `AETHER_MAX_NET=0` is how an operator says "this agent may not use the
// network". It is enforced by the effect gate, which charges the network
// budget for every builtin classified `Network` -- and only for those. A
// builtin that reaches the network while classified `pure` is not charged,
// so the setting silently does not apply to it.
//
// `ai("hello")` was such a builtin: classified pure, it loaded an API key and
// headed for the model provider with the budget at zero. A prompt can carry
// file contents, so that is an exfiltration path under an explicit
// no-network policy.
//
// Reading the source for "which builtins touch the network" is the
// name-based reasoning this work exists to remove, and a first attempt at it
// mis-mapped dispatch indices to functions. So this measures the property
// directly: every builtin, run under `strace -f -e trace=connect` with the
// budget at zero, and any IPv4/IPv6 `connect()` recorded -- from the shell or
// from any process it starts.
//
//   node benches/agentic/network-egress.mjs      (Linux, needs strace)
//
// AI providers are pointed at IronGate on localhost, so an attempt is a real
// `connect()` rather than a DNS failure; nothing leaves the machine.

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

const JAIL = fs.mkdtempSync(path.join(os.tmpdir(), 'ae-egress-'));
const TRACE = path.join(JAIL, 'trace.log');
process.on('exit', () => fs.rmSync(JAIL, { recursive: true, force: true }));

const ENV = {
    ...process.env,
    AETHER_MAX_NET: '0',
    AETHER_AI: 'irongate',
    // Agents run headless. With a display, clipboard builtins start an X11
    // helper that outlives `ae`, and strace waits on it forever.
    DISPLAY: '',
    WAYLAND_DISPLAY: '',
};

const traced = (code, env = ENV, agent = true) => {
    fs.rmSync(TRACE, { force: true });
    // spawnSync's own timeout signals strace alone; a tracee that forked into
    // the background keeps strace (and the pipes) alive. So: a new session,
    // coreutils timeout with a hard kill, no pipes to hold open, and the whole
    // process group killed afterwards.
    const r = spawnSync(
        'setsid',
        ['timeout', '-k', '2', '15',
         'strace', '-f', '-qq', '-e', 'trace=connect,%process', '-o', TRACE,
         'ae', ...(agent ? ['--agent', '--policy', 'strict', '--workspace', JAIL] : []), '-c', code],
        { cwd: JAIL, stdio: 'ignore', timeout: 30000, env },
    );
    try { process.kill(-r.pid, 'SIGKILL'); } catch { /* group already gone */ }
    const log = fs.existsSync(TRACE) ? fs.readFileSync(TRACE, 'utf8') : '';
    // A connection is only actionable once we know WHO made it: `ae` itself,
    // or a program it started (a CLI asked for its version that phones home).
    // Map every pid to the last program it exec'd, falling back to its parent.
    const lines = log.split('\n');
    const exe = new Map();
    const parent = new Map();
    for (const l of lines) {
        const ex = l.match(/^(\d+) execve\("([^"]+)"/);
        if (ex && !/= -1 /.test(l)) exe.set(ex[1], path.basename(ex[2]));
        const cl = l.match(/^(\d+) (?:<\.\.\. )?(?:clone3?|v?fork)\b.*= (\d+)$/);
        if (cl) parent.set(cl[2], cl[1]);
    }
    const who = (pid) => {
        for (let p = pid, hops = 0; p && hops < 64; p = parent.get(p), hops++) {
            if (exe.has(p)) return exe.get(p);
        }
        return '?';
    };
    // AF_UNIX connects (nscd, D-Bus) are local. AF_INET/AF_INET6 are not.
    const inet = lines
        .filter((l) => /connect\(/.test(l) && /sa_family=AF_INET6?\b/.test(l))
        .map((l) => ({ line: l, prog: who(l.match(/^(\d+)/)?.[1]) }));
    return { inet, status: r.status, err: r.error };
};

const which = spawnSync('which', ['strace'], { encoding: 'utf8' });
if (which.status !== 0) {
    console.error('! strace is not installed; this probe needs it');
    process.exit(2);
}

// Non-vacuity: the tracer has to SEE a connection before a clean result can
// mean anything. The control opens a TCP connection to a closed local port
// from a CHILD process -- curl -- which also proves `-f` follows
// the processes a builtin starts. (It was first `http.get` to 127.0.0.1, which
// never connects: the SSRF guard refuses local addresses before connect(), so
// the control failed and the probe correctly refused to report.)
const control = traced(
    `sh("curl -s --max-time 2 http://127.0.0.1:9/")`,
    // sh() is refused unless explicitly allowed -- the right default, and
    // lifted here for the control call only.
    { ...process.env, AETHER_ALLOW_SH: 'true' },
    false,
);
if (control.inet.length === 0) {
    console.error('! control call produced no connect(); the tracer is not seeing egress');
    process.exit(2);
}
// The attribution is checked the same way: the control's connection is made
// by curl, never by ae, so anything else means the pid map is wrong.
if (!control.inet.every(({ prog }) => prog === 'curl')) {
    console.error(`! control connect() attributed to ${control.inet.map((c) => c.prog).join(', ')}, not curl`);
    process.exit(2);
}
console.log(`control: tracer sees egress (${control.inet.length} connect() by curl on the control call)`);

const list = spawnSync('ae', ['-c', 'json.stringify(ontology_manifest())'], {
    encoding: 'utf8',
});
const cats = JSON.parse(list.stdout).categories.map((c) => (typeof c === 'string' ? c : c.category));
const names = new Set();
for (const cat of cats) {
    try {
        const d = spawnSync('ae', ['-c', `json.stringify(ontology_describe(${JSON.stringify(cat)}))`], {
            encoding: 'utf8',
        });
        for (const b of JSON.parse(d.stdout).builtins ?? []) names.add(b.name);
    } catch {
        // A category with no listing is not a failure of this sweep.
    }
}
console.log(`${cats.length} categories, ${names.size} builtins`);
if (names.size < 500) {
    console.error(`! only ${names.size} builtins enumerated; the walk is broken`);
    process.exit(2);
}

const leaks = [];
let done = 0;
for (const name of [...names].sort()) {
    const { inet } = traced(`${name}("hello")`);
    if (inet.length) {
        const dest = inet.map(({ line, prog }) => {
            const m = line.match(/sin6?_port=htons\((\d+)\).*?inet_(?:addr|pton)\([^"]*"([^"]+)"/);
            return `${prog} ${m ? `${m[2]}:${m[1]}` : line.slice(0, 80)}`;
        });
        leaks.push(`${name}\t${[...new Set(dest)].join(', ')}`);
    }
    if (++done % 200 === 0) process.stderr.write(`  ${done}/${names.size}\n`);
}

console.log(`\n${leaks.length} builtin(s) opened a network connection with AETHER_MAX_NET=0:`);
for (const l of leaks) console.log(`  ${l.replace('\t', '  ->  ')}`);
fs.writeFileSync('network-egress.txt', `${leaks.join('\n')}\n`);

if (process.env.AE_PROBE_ASSERT === '1' && leaks.length > 0) {
    console.error(`\n! ${leaks.length} builtin(s) reached the network with the budget at zero`);
    process.exit(1);
}
