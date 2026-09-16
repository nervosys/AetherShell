# Agentic shell benchmark

Generated 2026-09-16T23:31:26.176Z on linux x64, median of 11 runs after one discarded warm-up.
Token counts: **exact** cl100k_base BPE via tiktoken.

## Per-engine totals

| Engine | Correct | Byte-stable | Cmd tokens | Output tokens | Total tokens | Median ms | Total ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| aethershell | 8/8 | 8/8 | 163 | 679 | 842 | 9.7 | 141 |
| aethershell (agent) | 8/8 | 8/8 | 163 | 334 | 497 | 9.7 | 110 |
| bash | 8/8 | 8/8 | 68 | 1395 | 1463 | 5.3 | 50 |
| pwsh | 8/8 | 8/8 | 142 | 593 | 735 | 3284.7 | 26566 |
| nushell | 8/8 | 8/8 | 91 | 1183 | 1274 | 36.6 | 286 |

## Per-task total tokens (command + output)

| Task | aethershell | aethershell (agent) | bash | pwsh | nushell |
| --- | ---: | ---: | ---: | ---: | ---: |
| t1 | 582 | 285 | 1177 | 398 | 913 |
| t2 | 38 | 38 | 11 | 17 | 26 |
| t3 | 87 | 61 | 137 | 110 | 161 |
| t4 | 5 | 5 | 9 | 9 | 9 |
| t5 | 9 | 9 | 11 | 15 | 13 |
| t6 | 35 | 35 | 19 | 34 | 14 |
| t7 | 22 | 22 | 13 | 17 | 8 |
| t8 | 64 | 42 | 86 | 135 | 130 |

## Per-task wall clock, median ms

| Task | aethershell | aethershell (agent) | bash | pwsh | nushell |
| --- | ---: | ---: | ---: | ---: | ---: |
| t1 | 8.1 | 10.8 | 4.5 | 3077.2 | 29.6 |
| t2 | 77.2 | 51.0 | 13.3 | 4573.6 | 49.0 |
| t3 | 8.4 | 10.7 | 6.7 | 5988.5 | 38.0 |
| t4 | 9.7 | 9.7 | 5.0 | 521.0 | 37.1 |
| t5 | 10.3 | 6.0 | 5.3 | 2938.8 | 36.6 |
| t6 | 11.0 | 7.9 | 3.2 | 3284.7 | 30.2 |
| t7 | 8.9 | 6.3 | 5.3 | 2862.2 | 31.3 |
| t8 | 7.5 | 7.5 | 6.6 | 3319.8 | 33.9 |

## Determinism

Every engine produced byte-identical output across all 11 runs on this corpus.
