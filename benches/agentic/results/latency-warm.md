# Latency: cold processes and a warm session

2026-09-28, Debian on WSL2, release build, 50 queries per arm, the E1 corpus
(`issues.json`, 333 KB, 500 records), query `select count(*) from issues
where is_pr = 1` (answer 266 in every arm). `benches/agentic/warm.mjs`.

| Arm | Run 1 | Run 2 |
| --- | ---: | ---: |
| `ae -c 'sql_value(…)'`, a process per query | 25.9 ms | 20.3 ms |
| `sqlite3 issues.db …`, a process per query | 14.9 ms | 7.0 ms |
| **`ae mcp stdio`, one process, `tools/call` per query** | **0.47 ms** | **0.48 ms** |

The host's timing varied about 2x between runs (see sqlite3's two rows);
the warm arm did not.

## Where the cold time goes

`ae --version` costs the same as `ae -c 1` (21 ms in one measurement), so
it is spent before any AetherShell code runs. `LD_DEBUG=statistics`: 95% of
the dynamic loader's time is relocation, 53,123 relative relocations for the
pointers in a 50 MB position-independent binary's static tables. The two
direct fixes both cost something: a non-PIE build gives up ASLR for the
executable, and packed relocations (`-z pack-relative-relocs`) need glibc
2.36 at run time. Neither is taken. The warm session is the answer for
agents, and it is what `ae mcp stdio` and `ae agent serve` already are.

## Why the warm arm is sub-millisecond

Before `sql_value` cached loaded files, the warm arm measured 29.6 ms per
query against 64.4 ms cold, in a slower run: the process start was gone, but
every query still parsed the JSON and inserted all 500 rows. `src/sql.rs` now
keeps up to four loaded files per thread, keyed on canonical path, size and
modification time, so a changed file is reloaded
(`a_cached_file_is_reloaded_when_it_changes`).

This compares a session with process-per-call, which is how agents invoke
both tools. It is not a claim that AetherShell evaluates SQL faster than
SQLite, which it runs.
