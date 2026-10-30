# Example Test Results

## Summary

Test Date: October 18, 2025

| Example                         | Status     | Notes                                                            |
| ------------------------------- | ---------- | ---------------------------------------------------------------- |
| 00_hello.ish                     | ✅ PASS     | Basic execution and string interpolation work                    |
| 01_pipelines.ish                 | ⚠️ PARTIAL  | First part works, second part fails (runtime error)              |
| 02_tables.ish                    | ❌ FAIL     | Syntax error: unknown character `.` (field access not in lexer?) |
| 03_http.ish                      | ❓ UNTESTED | Requires network                                                 |
| 04_match.ish                     | ❌ FAIL     | Pattern matching not implemented                                 |
| 05_ai.ish                        | ❓ UNTESTED | Requires AI API                                                  |
| 06_agent.ish                     | ❓ UNTESTED | Requires AI API                                                  |
| 07_uri_types.ish                 | ❓ UNTESTED | Need to test                                                     |
| 08_transpiler.bash              | N/A        | Bash file, not IronShell                                       |
| 09_tui_multimodal.ish            | ❓ UNTESTED | TUI examples                                                     |
| 10_tui_agent_swarm.ish           | ❓ UNTESTED | TUI examples                                                     |
| 11_tui_showcase.ish              | ❓ UNTESTED | TUI examples                                                     |
| 12_multi_agent_orchestration.ish | ❓ UNTESTED | Advanced AI features                                             |
| 13_multimodal_ai.ish             | ❓ UNTESTED | AI features                                                      |
| 14_typed_pipelines.ish           | ❓ UNTESTED | Need to test                                                     |
| 15_ai_protocols.ish              | ❓ UNTESTED | AI features                                                      |
| 16_mcp_servers.ish               | ❓ UNTESTED | AI features                                                      |

## Detailed Results

### 00_hello.ish ✅
```
"Hello, Æther!"
"Hi, world!"
Bool(false)
```
String interpolation works! External commands execute.

### 01_pipelines.ish ⚠️
```
20
Error: where requires array input, got Str("20")
```
First pipeline works (map + reduce). Second pipeline fails - seems like the output of the first expression is being piped to the second.

### 02_tables.ish ❌
```
Error: unknown character: .
```
Field access syntax `r.type` not supported by lexer.

### 04_match.ish ❌
```
Error: unexpected token Match
```
Pattern matching not implemented in parser/evaluator.

## Issues Discovered

1. **Field Access (`.` operator)**: Not implemented in lexer - this is critical!
2. **Pipeline Isolation**: Multiple pipelines in same file seem to interfere
3. **Pattern Matching**: `match` keyword not implemented
4. **AI Features**: Can't test without API keys

## Recommendations

1. **High Priority**: Implement field access (`.`) - needed by many examples
2. **Medium Priority**: Fix pipeline isolation between statements  
3. **Low Priority**: Pattern matching (complex feature, low ROI for now)
4. **Documentation**: Mark AI examples as requiring API keys

## Next Steps

- Implement `.` field access operator
- Test remaining non-AI examples (07, 14)
- Create smoke tests for working examples
- Add example status badges to README
