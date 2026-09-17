// E6 -- can a shell-native agentic benchmark be pointed at AetherShell?
//
// The shell-native benchmarks in the field (AgentBench's OS environment, the
// SWE-bench family, OSWorld's terminal tasks) all drive the agent through the
// same interface: a string of shell, executed, with stdout and the exit status
// coming back. Retargeting one at AetherShell therefore rests on a single
// empirical question -- how much of the shell an agent actually emits does
// `ae -b` run? -- and the repository has claimed "Bash compatibility (via
// transpiler)" as a competitive feature without ever measuring it.
//
// The measurement has to separate two outcomes that both look like success:
//
//   native     AetherShell executed it, and the answer matches bash's.
//   delegated  AetherShell's transpiler fell back to `sh(["bash","-lc", …])`.
//              bash did the work. Whatever the run proves, it is not about
//              AetherShell, and every safety property in E4 is bypassed --
//              the effect gate sees one `sh` call, not the fifty things the
//              script did.
//
// A benchmark retargeted onto the delegated path measures bash wearing a
// costume. So `delegated` is counted as a failure of compatibility here, which
// is stricter than the feature claim and is the only counting that means
// anything.
//
//   node benches/agentic/bashcompat.mjs <repo-root>

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

const repo = process.argv[2];
if (!repo) { console.error('usage: bashcompat.mjs <repo-root>'); process.exit(2); }

// What an agent emits while working a SWE-bench-shaped task: orient, search,
// read, edit, build, test, inspect version control. Deliberately ordinary --
// nothing here is an exotic bash feature.
const CORPUS = [
    ['orient', 'pwd'],
    ['orient', 'ls'],
    ['orient', 'ls -la src | head -5'],
    ['orient', 'ls src/*.rs'],
    ['orient', 'find src -name "*.rs" | head -3'],
    ['search', 'grep -rn "fn main" src | head -3'],
    ['search', 'grep -c "fn " src/lib.rs'],
    ['search', 'grep -l "pub fn" src/*.rs | wc -l'],
    ['read', 'cat Cargo.toml | head -5'],
    ['read', 'head -20 src/lib.rs | tail -5'],
    ['read', 'wc -l src/lib.rs'],
    ['read', 'sed -n "1,3p" Cargo.toml'],
    ['vars', 'X=5; echo $X'],
    ['vars', 'echo $HOME'],
    ['vars', 'export FOO=bar && echo $FOO'],
    ['vars', 'echo "${PATH:-unset}" | head -c 10'],
    ['subst', 'echo $(date +%Y)'],
    ['subst', 'echo "lines: $(wc -l < Cargo.toml)"'],
    ['control', 'for i in 1 2 3; do echo $i; done'],
    ['control', 'if [ -f Cargo.toml ]; then echo yes; else echo no; fi'],
    ['control', 'test -d src && echo present'],
    ['control', 'while read -r l; do echo "$l"; break; done < Cargo.toml'],
    ['pipes', 'cat Cargo.toml | grep version | head -1'],
    ['pipes', 'ls src | sort | head -3'],
    ['pipes', 'grep "name" Cargo.toml | cut -d= -f2 | head -1'],
    ['redirect', 'echo hi > /tmp/ae_compat_probe.txt && cat /tmp/ae_compat_probe.txt'],
    ['redirect', 'cat Cargo.toml 2>/dev/null | head -1'],
    ['git', 'git rev-parse --abbrev-ref HEAD'],
    ['git', 'git status --porcelain | head -3'],
    ['git', 'git log --oneline -1'],
    ['exit', 'false || echo recovered'],
    ['exit', 'true && echo chained'],
];

const run = (bin, argv, env) => {
    const r = spawnSync(bin, argv, {
        cwd: repo, encoding: 'utf8', timeout: 30000, maxBuffer: 32 << 20,
        env: { ...process.env, ...env },
    });
    return {
        code: r.status,
        out: (r.stdout ?? '').trim(),
        err: (r.stderr ?? '').trim(),
    };
};

// The transpiler announces its own fallback, and the security layer logs the
// delegation. Either marker means bash ran the command, not AetherShell.
const DELEGATED = /\[SECURITY\] sh\(\) executed|sh\(\["bash"/;

const rows = [];
for (const [group, cmd] of CORPUS) {
    const reference = run('bash', ['-c', cmd], {});

    // Default posture: sh() disabled, which is what E4 measures and what any
    // deployment that cares about blast radius runs.
    const sealed = run('ae', ['-b', '-c', cmd], { AETHER_ALLOW_SH: '' });
    // Permissive posture: sh() allowed, which is what the compat claim needs.
    const open = run('ae', ['-b', '-c', cmd], { AETHER_ALLOW_SH: 'true' });

    const delegated = DELEGATED.test(open.err) || DELEGATED.test(open.out);
    const matches = (a, b) => a.code === b.code && a.out === b.out;

    let verdict;
    if (sealed.code === 0 && !DELEGATED.test(sealed.err) && matches(sealed, reference)) {
        verdict = 'native';
    } else if (delegated && open.code === 0) {
        verdict = 'delegated';
    } else if (/E_POLICY_DENY/.test(sealed.err)) {
        verdict = 'refused';
    } else {
        verdict = 'failed';
    }

    rows.push({
        group, cmd, verdict,
        bash_code: reference.code,
        sealed_code: sealed.code,
        open_code: open.code,
        sealed_excerpt: (sealed.err || sealed.out).replace(/\s+/g, ' ').slice(0, 90),
    });
}

fs.writeFileSync(path.join(repo, 'bashcompat.json'), JSON.stringify(rows, null, 2));

const tally = (v) => rows.filter((r) => r.verdict === v).length;
const pad = (s, n) => String(s).padEnd(n);

console.log(`${rows.length} ordinary shell commands through \`ae -b\`\n`);
console.log(`  native     ${tally('native')}\t AetherShell ran it and matched bash`);
console.log(`  delegated  ${tally('delegated')}\t handed to \`bash -lc\`; bash did the work`);
console.log(`  refused    ${tally('refused')}\t blocked by the sh() gate in the default posture`);
console.log(`  failed     ${tally('failed')}\t did not run, or gave a different answer\n`);

const groups = [...new Set(rows.map((r) => r.group))];
console.log(pad('group', 11) + ['native', 'delegated', 'refused', 'failed'].map((v) => pad(v, 11)).join(''));
for (const g of groups) {
    const rs = rows.filter((r) => r.group === g);
    console.log(
        pad(g, 11) +
        ['native', 'delegated', 'refused', 'failed']
            .map((v) => pad(`${rs.filter((r) => r.verdict === v).length}/${rs.length}`, 11))
            .join('')
    );
}

console.log('\nEverything that did not run natively:');
for (const r of rows.filter((x) => x.verdict !== 'native')) {
    console.log(`  [${pad(r.verdict, 9)}] ${r.cmd}`);
}
