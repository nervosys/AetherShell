Write the command in **AetherShell**, run as `ae -c '<command>'`.
Output only the command itself (not the `ae -c` wrapper).

The data is `issues.json`, a JSON array of 500 objects with this shape:

```json
{
  "number": 14465, "title": "Build structured image library",
  "state": "closed", "user": "leamcprice20",
  "created_at": "2026-09-16T10:45:19Z", "updated_at": "2026-09-16T10:49:21Z",
  "closed_at": "2026-09-16T10:49:21Z", "comments": 1,
  "is_pr": false, "labels": ["suspected-spam"], "body": "Objective\n\n..."
}
```

`state` is `"open"` or `"closed"`. `is_pr` is a boolean. `labels` is an array of
strings, possibly empty.

AetherShell is a typed shell: values flow through `|` as structured data, not
text. Lambdas are written `fn(x) => expr`. Read the file with `cat("issues.json")`
and parse it with `from_json`.

These are the builtins available, as the shell's own ontology describes them:

{{ONTOLOGY}}
