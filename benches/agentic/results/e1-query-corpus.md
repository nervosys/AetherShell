# Agentic shell benchmark

Generated 2026-09-17T19:07:46.305Z on linux x64, median of 11 runs after one discarded warm-up.
Token counts: **exact** cl100k_base BPE via tiktoken.

## Per-engine totals

| Engine | Correct | Byte-stable | Cmd tokens | Output tokens | Total tokens | Median ms | Total ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| bash+jq | 10/10 | 10/10 | 287 | 58 | 345 | 13.4 | 140 |
| bash+coreutils | 10/10 | 10/10 | 647 | 57 | 704 | 28.8 | 3686 |
| sqlite | 10/10 | 10/10 | 202 | 56 | 258 | 5.4 | 52 |
| pwsh | 10/10 | 10/10 | 474 | 57 | 531 | 3362.9 | 33995 |
| nushell | 10/10 | 10/10 | 297 | 56 | 353 | 35.0 | 361 |
| aethershell | 10/10 | 10/10 | 381 | 67 | 448 | 24.7 | 308 |
| aethershell (sql) | 10/10 | 10/10 | 329 | 57 | 386 | 7.9 | 80 |
| aethershell (agentic) | 10/10 | 10/10 | 275 | 67 | 342 | 38.8 | 438 |

## Per-task total tokens (command + output)

| Task | bash+jq | bash+coreutils | sqlite | pwsh | nushell | aethershell | aethershell (sql) | aethershell (agentic) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| q1 | 36 | 60 | 26 | 50 | 38 | 39 | 38 | 33 |
| q2 | 17 | 18 | 13 | 29 | 13 | 22 | 25 | 17 |
| q3 | 19 | 47 | 14 | 40 | 18 | 33 | 27 | 21 |
| q4 | 57 | 68 | 31 | 73 | 62 | 57 | 48 | 49 |
| q5 | 63 | 132 | 59 | 92 | 68 | 83 | 71 | 72 |
| q6 | 15 | 67 | 21 | 34 | 14 | 25 | 33 | 16 |
| q7 | 27 | 79 | 31 | 43 | 35 | 38 | 44 | 32 |
| q8 | 12 | 30 | 10 | 28 | 11 | 21 | 22 | 12 |
| q9 | 61 | 121 | 29 | 84 | 55 | 85 | 41 | 56 |
| q10 | 38 | 82 | 24 | 58 | 39 | 45 | 37 | 34 |

## Per-task wall clock, median ms

| Task | bash+jq | bash+coreutils | sqlite | pwsh | nushell | aethershell | aethershell (sql) | aethershell (agentic) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| q1 | 26.5 | 803.7 | 4.8 | 3462.2 | 52.4 | 66.9 | 8.8 | 59.0 |
| q2 | 13.6 | 13.8 | 4.9 | 3042.2 | 53.1 | 69.0 | 7.9 | 46.8 |
| q3 | 13.4 | 22.9 | 4.8 | 2734.1 | 50.4 | 39.0 | 7.1 | 53.8 |
| q4 | 11.3 | 20.6 | 5.5 | 3296.2 | 33.6 | 14.7 | 7.7 | 36.5 |
| q5 | 10.6 | 1247.1 | 4.9 | 3240.8 | 34.1 | 31.7 | 8.5 | 26.0 |
| q6 | 10.3 | 17.8 | 5.4 | 3362.9 | 35.0 | 11.3 | 6.9 | 27.3 |
| q7 | 11.4 | 1035.6 | 5.4 | 3137.8 | 36.6 | 17.1 | 7.2 | 38.8 |
| q8 | 11.9 | 13.1 | 5.7 | 3527.2 | 21.2 | 13.1 | 6.2 | 35.5 |
| q9 | 15.3 | 28.8 | 6.0 | 3933.4 | 23.7 | 24.7 | 8.1 | 79.7 |
| q10 | 15.4 | 482.7 | 4.6 | 4258.0 | 21.1 | 21.1 | 11.4 | 34.6 |

## Determinism

Every engine produced byte-identical output across all 11 runs on this corpus.
