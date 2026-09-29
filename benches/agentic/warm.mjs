// Cold vs warm: `ae -c` per query, one `ae mcp stdio` process answering N
// tools/call requests, and `sqlite3` per query, on the E1 corpus.
//
//   cd <datadir> && AE_BIN=$(command -v ae) node benches/agentic/warm.mjs
//
// The warm arm pays the process start once, and `sql_value` keeps each file's
// loaded database between calls (src/sql.rs), so this measures what an agent
// connected over MCP pays per question. The sqlite3 arm starts a process per
// query, as a tool call to the CLI does; SQLite as a library held open would
// also answer in well under a millisecond.
import { spawn, spawnSync } from 'node:child_process';

const AE = process.env.AE_BIN;
const N = 50;
const SQL = 'select count(*) from issues where is_pr = 1';

function cold() {
    const t0 = process.hrtime.bigint();
    for (let i = 0; i < N; i++) {
        const r = spawnSync(AE, ['-c', `sql_value("issues.json", "${SQL}")`], { encoding: 'utf8' });
        if (r.status !== 0) throw new Error(r.stderr);
    }
    return Number(process.hrtime.bigint() - t0) / 1e6 / N;
}

function warm() {
    return new Promise((resolve, reject) => {
        const p = spawn(AE, ['mcp', 'stdio'], { stdio: ['pipe', 'pipe', 'inherit'] });
        let buf = '';
        let got = 0;
        let t0;
        const send = (id) => p.stdin.write(JSON.stringify({
            jsonrpc: '2.0', id, method: 'tools/call',
            params: { name: 'aether', arguments: { name: 'sql_value', args: ['issues.json', SQL] } },
        }) + '\n');
        p.stdout.on('data', (d) => {
            buf += d;
            let nl;
            while ((nl = buf.indexOf('\n')) >= 0) {
                const line = buf.slice(0, nl);
                buf = buf.slice(nl + 1);
                const msg = JSON.parse(line);
                if (msg.error || msg.result?.isError) { reject(new Error(line)); return; }
                if (msg.id === 0) { t0 = process.hrtime.bigint(); send(1); continue; }
                got++;
                if (got === 1 && !String(JSON.stringify(msg.result)).match(/\d/)) {
                    reject(new Error('no answer: ' + line)); return;
                }
                if (got < N) send(got + 1);
                else {
                    const per = Number(process.hrtime.bigint() - t0) / 1e6 / N;
                    p.stdin.end();
                    resolve({ per, sample: JSON.stringify(msg.result).slice(0, 120) });
                }
            }
        });
        // Warm-up call (id 0) is not timed: it pays the process start.
        send(0);
    });
}

const c = cold();
const w = await warm();
console.log(`cold ae -c:        ${c.toFixed(2)} ms/query`);
console.log(`warm mcp stdio:    ${w.per.toFixed(2)} ms/query   (${w.sample})`);
const s = spawnSync('sqlite3', ['issues.db', SQL], { encoding: 'utf8' });
const t0 = process.hrtime.bigint();
for (let i = 0; i < N; i++) spawnSync('sqlite3', ['issues.db', SQL]);
console.log(`cold sqlite3:      ${(Number(process.hrtime.bigint() - t0) / 1e6 / N).toFixed(2)} ms/query   (${s.stdout.trim()})`);
