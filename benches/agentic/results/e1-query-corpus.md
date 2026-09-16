# Agentic shell benchmark

Generated 2026-09-16T20:04:07.490Z on linux x64, median of 11 runs after one discarded warm-up.
Token counts: **exact** cl100k_base BPE via tiktoken.

## Per-engine totals

| Engine | Correct | Byte-stable | Cmd tokens | Output tokens | Total tokens | Median ms | Total ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| aethershell | 10/10 | 10/10 | 381 | 67 | 448 | 39.2 | 419 |
| bash+jq | 10/10 | 10/10 | 287 | 58 | 345 | 13.4 | 140 |
| bash+coreutils | 10/10 | 10/10 | 647 | 57 | 704 | 28.8 | 3686 |
| sqlite | 10/10 | 10/10 | 202 | 56 | 258 | 5.4 | 52 |
| pwsh | 10/10 | 10/10 | 474 | 57 | 531 | 3362.9 | 33995 |
| nushell | 10/10 | 10/10 | 297 | 56 | 353 | 35.0 | 361 |

## Per-task total tokens (command + output)

| Task | aethershell | bash+jq | bash+coreutils | sqlite | pwsh | nushell |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| q1 | 39 | 36 | 60 | 26 | 50 | 38 |
| q2 | 22 | 17 | 18 | 13 | 29 | 13 |
| q3 | 33 | 19 | 47 | 14 | 40 | 18 |
| q4 | 57 | 57 | 68 | 31 | 73 | 62 |
| q5 | 83 | 63 | 132 | 59 | 92 | 68 |
| q6 | 25 | 15 | 67 | 21 | 34 | 14 |
| q7 | 38 | 27 | 79 | 31 | 43 | 35 |
| q8 | 21 | 12 | 30 | 10 | 28 | 11 |
| q9 | 85 | 61 | 121 | 29 | 84 | 55 |
| q10 | 45 | 38 | 82 | 24 | 58 | 39 |

## Per-task wall clock, median ms

| Task | aethershell | bash+jq | bash+coreutils | sqlite | pwsh | nushell |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| q1 | 44.8 | 26.5 | 803.7 | 4.8 | 3462.2 | 52.4 |
| q2 | 34.0 | 13.6 | 13.8 | 4.9 | 3042.2 | 53.1 |
| q3 | 51.7 | 13.4 | 22.9 | 4.8 | 2734.1 | 50.4 |
| q4 | 46.9 | 11.3 | 20.6 | 5.5 | 3296.2 | 33.6 |
| q5 | 31.7 | 10.6 | 1247.1 | 4.9 | 3240.8 | 34.1 |
| q6 | 37.3 | 10.3 | 17.8 | 5.4 | 3362.9 | 35.0 |
| q7 | 39.2 | 11.4 | 1035.6 | 5.4 | 3137.8 | 36.6 |
| q8 | 30.7 | 11.9 | 13.1 | 5.7 | 3527.2 | 21.2 |
| q9 | 78.6 | 15.3 | 28.8 | 6.0 | 3933.4 | 23.7 |
| q10 | 23.9 | 15.4 | 482.7 | 4.6 | 4258.0 | 21.1 |

## Determinism

Every engine produced byte-identical output across all 11 runs on this corpus.
