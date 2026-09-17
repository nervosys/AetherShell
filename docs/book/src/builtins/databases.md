# Databases & SQL

AetherShell can put a corpus into SQLite and query it with SQL, from inside the
shell. That matters more than it sounds, and the measurement is in
`benches/agentic/`.

## Why: parsing per query is the cost

A pipeline that reads and parses a document on every invocation pays for that
parse every time. On 500 GitHub issue records — 334 KB of JSON — answering ten
questions costs:

| Route | Total tokens | Total time |
| --- | ---: | ---: |
| `cat(f) \| from_json \| where(…)` per query | 448 | 308 ms |
| `sqlite_query` against a prepared database | **386** | **80 ms** |
| bare `sqlite3` outside the shell | 258 | 52 ms |

Same answers — all ten correct by every route. The difference is that SQLite
reads an index and the pipeline re-parses the document. Nothing about the
evaluator closes that gap; not re-parsing does.

This is also the configuration a published comparison of agent setups found
best overall: a shell for exploration and SQL for the query, rather than either
alone. Here it is one shell, so the typed output, the structured errors and the
effect gate are still around the query.

## Ingest, then query

Note the argument order: **database first**, then the source file, then the
table. It reads backwards from the name.

```aethershell
db_json_to_sqlite("issues.db", "issues.json", "issues")
# true

sqlite_query("issues.db", "SELECT count(*) FROM issues WHERE is_pr=1")
# [{count(*): 293}]
```

`db_csv_to_sqlite` does the same from CSV. `db_json_query` and `db_csv_query`
query those formats directly when a database would be overkill.

The result is ordinary typed data, so it composes with the rest of the language:

```aethershell
sqlite_query("issues.db", "SELECT user, count(*) n FROM issues GROUP BY user")
    | where(fn(r) => r.n > 10)
    | sort_by("n")
    | last(5)
    | len
# 4
```

## Two things to know before relying on this

**`sqlite_query` is not in the ontology.** It works, and
`ontology_describe("sqlite_query")` reports that it is not a known builtin, so
an agent that discovers the surface by asking — which is what the ontology is
for — will not find it. Use `db_sqlite_*` names, which are listed, or call
`sqlite_query` knowing it is currently undiscoverable. This is a real gap, not
a documentation convention.

**The listed entries carry no signatures.** `ontology_describe(
"db_json_to_sqlite")` returns `signature=db_json_to_sqlite() -> Value` with an
empty parameter list and no examples, which is why the argument order above is
spelled out here. The same is true of much of the `Database` category.

## Safety

`sqlite_query` goes through the same execution guard as any other builtin, so
agent mode's effect gate applies to it. A query against a database outside the
workspace is not silently allowed: in agent mode it raises an **approval**
request carrying the blast radius, rather than a flat refusal, because reading
a database is a `ReadLocal` effect rather than a destructive one.

See [Security & Auth](../advanced/security.md).
