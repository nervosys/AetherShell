# Agentic shell benchmark

Generated 2026-09-16T20:08:23.933Z on linux x64, median of 11 runs after one discarded warm-up.
Token counts: **exact** cl100k_base BPE via tiktoken.

## Per-engine totals

| Engine | Correct | Byte-stable | Cmd tokens | Output tokens | Total tokens | Median ms | Total ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| aethershell | 8/8 | 8/8 | 163 | 177 | 340 | 5.3 | 66 |
| bash | 8/8 | 8/8 | 68 | 1395 | 1463 | 3.8 | 36 |
| pwsh | 8/8 | 8/8 | 142 | 593 | 735 | 2944.2 | 19725 |
| nushell | 8/8 | 8/8 | 91 | 1180 | 1271 | 31.3 | 251 |

## Per-task total tokens (command + output)

| Task | aethershell | bash | pwsh | nushell |
| --- | ---: | ---: | ---: | ---: |
| t1 | 161 | 1177 | 398 | 913 |
| t2 | 38 | 11 | 17 | 26 |
| t3 | 38 | 137 | 110 | 161 |
| t4 | 5 | 9 | 9 | 9 |
| t5 | 9 | 11 | 15 | 13 |
| t6 | 35 | 19 | 34 | 14 |
| t7 | 22 | 13 | 17 | 8 |
| t8 | 32 | 86 | 135 | 127 |

## Per-task wall clock, median ms

| Task | aethershell | bash | pwsh | nushell |
| --- | ---: | ---: | ---: | ---: |
| t1 | 5.0 | 3.5 | 2944.2 | 31.3 |
| t2 | 31.2 | 10.2 | 3599.0 | 35.4 |
| t3 | 5.3 | 3.9 | 3011.9 | 33.7 |
| t4 | 5.5 | 3.3 | 409.3 | 38.3 |
| t5 | 4.7 | 3.8 | 1945.7 | 25.6 |
| t6 | 5.5 | 3.5 | 1953.3 | 26.4 |
| t7 | 4.3 | 4.2 | 2201.3 | 30.7 |
| t8 | 4.4 | 3.8 | 3660.1 | 29.5 |

## Determinism

Every engine produced byte-identical output across all 11 runs on this corpus.
