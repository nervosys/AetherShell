# Agentic shell benchmark

Generated 2026-09-26T05:58:42.583Z on linux x64, median of 11 runs after one discarded warm-up.
Token counts: **exact** cl100k_base BPE via tiktoken.

## Per-engine totals

| Engine | Correct | Byte-stable | Cmd tokens | Output tokens | Total tokens | Median ms | Total ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| aethershell | 8/8 | 8/8 | 163 | 693 | 856 | 48.9 | 922 |
| aethershell (agent) | 8/8 | 8/8 | 163 | 348 | 511 | 50.3 | 938 |
| bash | 8/8 | 8/8 | 68 | 1422 | 1490 | 90.5 | 567 |
| pwsh | 8/8 | 8/8 | 142 | 600 | 742 | 3045.3 | 20252 |
| nushell | 8/8 | 8/8 | 91 | 1192 | 1283 | 125.2 | 2160 |

## Per-task total tokens (command + output)

| Task | aethershell | aethershell (agent) | bash | pwsh | nushell |
| --- | ---: | ---: | ---: | ---: | ---: |
| t1 | 594 | 290 | 1204 | 405 | 931 |
| t2 | 38 | 38 | 11 | 17 | 26 |
| t3 | 87 | 61 | 137 | 110 | 161 |
| t4 | 5 | 5 | 9 | 9 | 9 |
| t5 | 9 | 9 | 11 | 15 | 13 |
| t6 | 35 | 35 | 19 | 34 | 14 |
| t7 | 22 | 22 | 13 | 17 | 8 |
| t8 | 66 | 51 | 86 | 135 | 121 |

## Per-task wall clock, median ms

| Task | aethershell | aethershell (agent) | bash | pwsh | nushell |
| --- | ---: | ---: | ---: | ---: | ---: |
| t1 | 48.9 | 50.2 | 124.9 | 3473.1 | 125.2 |
| t2 | 468.1 | 475.5 | 124.3 | 4138.4 | 747.8 |
| t3 | 48.1 | 50.3 | 90.5 | 3045.3 | 104.9 |
| t4 | 40.8 | 42.5 | 80.9 | 484.7 | 99.1 |
| t5 | 166.9 | 167.1 | 19.1 | 2320.8 | 623.8 |
| t6 | 10.6 | 11.9 | 6.9 | 1650.9 | 31.2 |
| t7 | 89.7 | 90.1 | 29.7 | 2078.7 | 328.0 |
| t8 | 48.7 | 50.0 | 91.3 | 3059.7 | 99.8 |

## Determinism

Every engine produced byte-identical output across all 11 runs on this corpus.
