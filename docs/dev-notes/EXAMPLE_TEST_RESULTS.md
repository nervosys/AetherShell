# Example Test Results

## Summary

Test Date: October 18, 2025

| Example                         | Status     | Notes                                                            |
| ------------------------------- | ---------- | ---------------------------------------------------------------- |
| 00_hello.ae                     | ✅ PASS     | Basic execution and string interpolation work                    |
| 01_pipelines.ae                 | ⚠️ PARTIAL  | First part works, second part fails (runtime error)              |
| 02_tables.ae                    | ❌ FAIL     | Syntax error: unknown character `.` (field access not in lexer?) |
| 03_http.ae                      | ❓ UNTESTED | Requires network                                                 |
| 04_match.ae                     | ❌ FAIL     | Pattern matching not implemented                                 |
| 05_ai.ae                        | ❓ UNTESTED | Requires AI API                                                  |
| 06_agent.ae                     | ❓ UNTESTED | Requires AI API                                                  |
| 07_uri_types.ae                 | ❓ UNTESTED | Need to test                                                     |
| 08_transpiler.bash              | N/A        | Bash file, not AetherShell                                       |
| 09_tui_multimodal.ae            | ❓ UNTESTED | TUI examples                                                     |
| 10_tui_agent_swarm.ae           | ❓ UNTESTED | TUI examples                                                     |
| 11_tui_showcase.ae              | ❓ UNTESTED | TUI examples                                                     |
| 12_multi_agent_orchestration.ae | ❓ UNTESTED | Advanced AI features                                             |
| 13_multimodal_ai.ae             | ❓ UNTESTED | AI features                                                      |
| 14_typed_pipelines.ae           | ❓ UNTESTED | Need to test                                                     |
| 15_ai_protocols.ae              | ❓ UNTESTED | AI features                                                      |
| 16_mcp_servers.ae               | ❓ UNTESTED | AI features                                                      |

## Detailed Results

### 00_hello.ae ✅
```
"Hello, Æther!"
"Hi, world!"
Bool(false)
```
String interpolation works! External commands execute.

### 01_pipelines.ae ⚠️
```
20
Error: where requires array input, got Str("20")
```
First pipeline works (map + reduce). Second pipeline fails - seems like the output of the first expression is being piped to the second.

### 02_tables.ae ❌
```
Error: unknown character: .
```
Field access syntax `r.type` not supported by lexer.

### 04_match.ae ❌
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
