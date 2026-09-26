// The oracle for E1 and E7: every question answered in plain JavaScript,
// directly from issues.json, sharing no code with any engine under test.
//
// corpus.mjs used to carry these answers as a pasted table, and its comment
// said they were "computed independently in oracle.mjs" -- a file that was
// never committed. So the claim could not be checked, and the table only held
// for the one fetch it was computed from. `prepare.mjs` fetches the latest 500
// items, so on any later fetch every engine was scored wrong while all eight
// agreed with each other. Computing the oracle at run time from the corpus
// actually on disk is what the documentation always said happened.
//
// Values on the 2026-09-17 corpus, for provenance: q1 10, q2 293, q3 430,
// q4 "dependabot[bot] 72", q5 14086..14420 (10 numbers), q6 43, q7 19, q8 97,
// q9 25, q10 4.97.
//
//   node benches/agentic/oracle.mjs <datadir>     prints the answers

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export function oracleFor(dataDir) {
    const recs = JSON.parse(fs.readFileSync(path.join(dataDir, 'issues.json'), 'utf8'));
    const prs = recs.filter((r) => r.is_pr);
    const issues = recs.filter((r) => !r.is_pr);
    const open = (r) => r.state === 'open';

    // q4 asks for "the" author with the most PRs; a tie makes the question
    // ambiguous, and an oracle that silently picked one would score engines
    // on their tie-breaking. Refuse instead.
    const byAuthor = new Map();
    for (const r of prs) byAuthor.set(r.user, (byAuthor.get(r.user) ?? 0) + 1);
    const top = Math.max(...byAuthor.values());
    const leaders = [...byAuthor].filter(([, n]) => n === top).map(([u]) => u);
    if (leaders.length !== 1) {
        throw new Error(`q4 is ambiguous on this corpus: ${leaders.join(', ')} each opened ${top} PRs`);
    }

    const openIssueComments = issues.filter(open).map((r) => r.comments);
    const mean = openIssueComments.reduce((a, b) => a + b, 0) / openIssueComments.length;
    const issueAuthors = new Set(issues.map((r) => r.user));

    return {
        q1: String(recs.filter((r) => open(r) && `${r.title} ${r.body}`.toLowerCase().includes('security')).length),
        q2: String(prs.length),
        q3: String(recs.filter((r) => r.state === 'closed').reduce((a, r) => a + r.comments, 0)),
        q4: `${leaders[0]} ${top}`,
        q5: issues.filter((r) => open(r) && r.comments > 5).map((r) => r.number).sort((a, b) => a - b).join(','),
        q6: String(new Set(recs.flatMap((r) => r.labels)).size),
        q7: String(recs.filter((r) => open(r) && r.labels.includes('bug')).length),
        q8: String(Math.max(...recs.map((r) => r.comments))),
        q9: String(new Set(prs.map((r) => r.user).filter((u) => issueAuthors.has(u))).size),
        q10: String(Math.round(mean * 100) / 100),
    };
}

// What identifies a fetch well enough to quote next to a result. The corpus
// itself is not committed: it is the latest 500 items when prepare.mjs ran.
export function corpusFacts(dataDir) {
    const recs = JSON.parse(fs.readFileSync(path.join(dataDir, 'issues.json'), 'utf8'));
    const numbers = recs.map((r) => r.number);
    return {
        records: recs.length,
        prs: recs.filter((r) => r.is_pr).length,
        open: recs.filter((r) => r.state === 'open').length,
        numbers: `${Math.min(...numbers)}..${Math.max(...numbers)}`,
    };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
    const dir = process.argv[2];
    if (!dir) {
        console.error('usage: oracle.mjs <datadir>');
        process.exit(2);
    }
    for (const [q, a] of Object.entries(oracleFor(dir))) console.log(`${q}\t${a}`);
}
