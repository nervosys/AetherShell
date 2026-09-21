Write the command as a **SQLite query**, run as `sqlite3 issues.db "<query>"`.
Output only the query itself (not the `sqlite3` invocation).

Table schema:

```sql
CREATE TABLE issues(
  number     INTEGER PRIMARY KEY,
  title      TEXT,
  state      TEXT,     -- 'open' or 'closed'
  user       TEXT,
  created_at TEXT,
  updated_at TEXT,
  closed_at  TEXT,
  comments   INTEGER,
  is_pr      INTEGER,  -- 1 for pull requests, 0 for issues
  labels     TEXT,     -- a JSON array of label names, e.g. '["bug","p1"]'
  body       TEXT
);
```
