Write the command as **bash using jq**, against `issues.json`.

`issues.json` is a JSON array of 500 objects with this shape:

```json
{
  "number": 14465,
  "title": "Build structured image library",
  "state": "closed",
  "user": "leamcprice20",
  "created_at": "2026-09-16T10:45:19Z",
  "updated_at": "2026-09-16T10:49:21Z",
  "closed_at": "2026-09-16T10:49:21Z",
  "comments": 1,
  "is_pr": false,
  "labels": ["suspected-spam"],
  "body": "Objective\n\nBuild a persistent ..."
}
```

`state` is `"open"` or `"closed"`. `is_pr` is a boolean. `labels` is an array of
strings, possibly empty.
