// Build the structured-data corpus used by E1 (the Vercel-blog replication).
//
// The dataset is *real*: 500 issues and pull requests from github.com/cli/cli,
// fetched with `gh api`. It is materialised in the three representations the
// blog post compared, so every engine reads the same facts in the shape its
// own idiom expects:
//
//   issues.json    one JSON array          (jq, AetherShell, pwsh, nushell)
//   issues/*.json  one pretty file each    (the "filesystem agent")
//   issues.db      SQLite, labels as JSON  (the SQL agent)
//
// Usage: node benches/agentic/prepare.mjs <outdir>
import { execFileSync } from 'node:child_process';
import { DatabaseSync } from 'node:sqlite';
import fs from 'node:fs';
import path from 'node:path';

const out = process.argv[2];
if (!out) { console.error('usage: prepare.mjs <outdir>'); process.exit(2); }
fs.mkdirSync(out, { recursive: true });

const REPO = process.env.BENCH_REPO ?? 'cli/cli';
const PAGES = 5; // 100 per page

let raw = [];
for (let p = 1; p <= PAGES; p++) {
    const body = execFileSync(
        'gh', ['api', `repos/${REPO}/issues?state=all&per_page=100&page=${p}`],
        { encoding: 'utf8', maxBuffer: 64 << 20, shell: true }
    );
    raw = raw.concat(JSON.parse(body));
}

// Body is truncated to 400 chars so the corpus stays a fixed, quotable size;
// every engine sees the identical truncation.
const recs = raw.map((x) => ({
    number: x.number,
    title: x.title,
    state: x.state,
    user: x.user && x.user.login,
    created_at: x.created_at,
    updated_at: x.updated_at,
    closed_at: x.closed_at,
    comments: x.comments,
    is_pr: !!x.pull_request,
    labels: (x.labels || []).map((l) => l.name),
    body: (x.body || '').slice(0, 400),
}));

fs.writeFileSync(path.join(out, 'issues.json'), JSON.stringify(recs));
fs.writeFileSync(path.join(out, 'issues.jsonl'), recs.map((r) => JSON.stringify(r)).join('\n') + '\n');

const dir = path.join(out, 'issues');
fs.rmSync(dir, { recursive: true, force: true });
fs.mkdirSync(dir);
for (const r of recs) fs.writeFileSync(path.join(dir, `${r.number}.json`), JSON.stringify(r, null, 2));

const dbPath = path.join(out, 'issues.db');
fs.rmSync(dbPath, { force: true });
const db = new DatabaseSync(dbPath);
db.exec(`CREATE TABLE issues(number INTEGER PRIMARY KEY, title TEXT, state TEXT,
    user TEXT, created_at TEXT, updated_at TEXT, closed_at TEXT, comments INTEGER,
    is_pr INTEGER, labels TEXT, body TEXT)`);
const ins = db.prepare('INSERT INTO issues VALUES(?,?,?,?,?,?,?,?,?,?,?)');
for (const r of recs) {
    ins.run(r.number, r.title, r.state, r.user, r.created_at, r.updated_at,
        r.closed_at, r.comments, r.is_pr ? 1 : 0, JSON.stringify(r.labels), r.body);
}

console.log(`${recs.length} records from ${REPO}: ` +
    `${recs.filter((r) => r.is_pr).length} PRs, ${recs.filter((r) => r.state === 'open').length} open`);
