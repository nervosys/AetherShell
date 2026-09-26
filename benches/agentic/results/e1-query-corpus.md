# Agentic shell benchmark

Generated 2026-09-26T06:07:37.465Z on linux x64, median of 11 runs after one discarded warm-up.
Token counts: **exact** cl100k_base BPE via tiktoken.

## Per-engine totals

| Engine | Correct | Byte-stable | Cmd tokens | Output tokens | Total tokens | Median ms | Total ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| aethershell | 10/10 | 10/10 | 381 | 61 | 442 | 12.8 | 131 |
| aethershell (agentic) | 10/10 | 10/10 | 275 | 61 | 336 | 12.6 | 130 |
| aethershell (sql) | 10/10 | 10/10 | 329 | 52 | 381 | 5.8 | 56 |
| bash+jq | 10/10 | 10/10 | 287 | 53 | 340 | 6.2 | 76 |
| bash+coreutils | 10/10 | 10/10 | 647 | 52 | 699 | 17.2 | 2288 |
| sqlite | 10/10 | 10/10 | 202 | 52 | 254 | 2.7 | 27 |
| pwsh | 10/10 | 10/10 | 474 | 52 | 526 | 2593.7 | 25947 |
| nushell | 10/10 | 10/10 | 297 | 51 | 348 | 14.8 | 153 |

## Per-task total tokens (command + output)

| Task | aethershell | aethershell (agentic) | aethershell (sql) | bash+jq | bash+coreutils | sqlite | pwsh | nushell |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| q1 | 39 | 33 | 38 | 36 | 60 | 26 | 50 | 38 |
| q2 | 22 | 17 | 25 | 17 | 18 | 13 | 29 | 13 |
| q3 | 33 | 21 | 27 | 19 | 47 | 14 | 40 | 18 |
| q4 | 55 | 47 | 46 | 55 | 66 | 30 | 71 | 60 |
| q5 | 79 | 68 | 68 | 60 | 129 | 56 | 89 | 65 |
| q6 | 25 | 16 | 33 | 15 | 67 | 21 | 34 | 14 |
| q7 | 38 | 32 | 44 | 27 | 79 | 31 | 43 | 35 |
| q8 | 21 | 12 | 22 | 12 | 30 | 10 | 28 | 11 |
| q9 | 85 | 56 | 41 | 61 | 121 | 29 | 84 | 55 |
| q10 | 45 | 34 | 37 | 38 | 82 | 24 | 58 | 39 |

## Per-task wall clock, median ms

| Task | aethershell | aethershell (agentic) | aethershell (sql) | bash+jq | bash+coreutils | sqlite | pwsh | nushell |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| q1 | 15.0 | 14.6 | 5.3 | 19.8 | 548.6 | 2.7 | 2597.1 | 15.6 |
| q2 | 10.9 | 11.4 | 5.4 | 5.8 | 5.7 | 2.7 | 2584.1 | 14.8 |
| q3 | 12.8 | 12.6 | 5.2 | 5.9 | 8.1 | 2.5 | 2594.9 | 14.7 |
| q4 | 13.3 | 12.4 | 5.8 | 6.2 | 8.8 | 3.2 | 2585.9 | 15.0 |
| q5 | 11.3 | 12.7 | 5.8 | 6.1 | 776.7 | 2.5 | 2593.7 | 14.4 |
| q6 | 10.4 | 10.7 | 5.9 | 6.3 | 9.8 | 2.6 | 2575.5 | 14.2 |
| q7 | 13.0 | 12.8 | 5.8 | 6.2 | 607.1 | 2.7 | 2592.5 | 17.1 |
| q8 | 10.9 | 9.9 | 5.5 | 5.9 | 6.1 | 2.5 | 2581.2 | 13.8 |
| q9 | 22.1 | 21.8 | 6.2 | 7.3 | 17.2 | 3.1 | 2615.0 | 18.8 |
| q10 | 10.9 | 10.9 | 5.7 | 6.5 | 300.1 | 2.7 | 2626.8 | 14.6 |

## Determinism

Every engine produced byte-identical output across all 11 runs on this corpus.
