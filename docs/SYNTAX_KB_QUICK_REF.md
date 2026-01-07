# Syntax KB Quick Reference

## Builtins

### Knowledge Base Operations
```aether
// Get syntax entry
syntax_get("ab")           // Returns full AgenticBinary protocol details

// Search for entries
syntax_search("protocol")  // Returns ["ab", "jsonrpc"]

// List entries
syntax_list()              // All syntax IDs
syntax_list("protocol")    // Only protocols

// Add custom syntax
syntax_add({
    id: "custom",
    name: "Custom Protocol",
    category: "protocol",
    specification: "Full spec here...",
    examples: ["example 1", "example 2"]
})
```

### AgenticBinary Encoding
```aether
// Encode message - returns byte array
bytes = ab_encode("command", "ping", "hello")
// => [0, 5, 104, 101, 108, 108, 111]

// Decode message - returns full details
decoded = ab_decode(bytes)
// => {
//   version: 0,
//   msg_type: "Command",
//   opcode: "PING",
//   payload: "hello",
//   ...
// }
```

## Message Types
- `"command"` (0) - Command execution
- `"query"` (1) - Data query
- `"response"` (2) - Response/reply
- `"event"` (3) - Event notification

## Opcodes (16 operations)
| Opcode          | Code | Use Case                 |
| --------------- | ---- | ------------------------ |
| `"ping"`        | 0x0  | Heartbeat check          |
| `"ack"`         | 0x1  | Acknowledgment           |
| `"query"`       | 0x2  | Data query               |
| `"exec"`        | 0x3  | Execute command          |
| `"data"`        | 0x4  | Data transfer            |
| `"error"`       | 0x5  | Error condition          |
| `"sync"`        | 0x6  | Synchronization          |
| `"auth"`        | 0x7  | Authentication           |
| `"delegate"`    | 0x8  | Task delegation          |
| `"collaborate"` | 0x9  | Multi-agent coordination |
| `"learn"`       | 0xA  | Knowledge sharing        |
| `"reason"`      | 0xB  | Reasoning request        |
| `"plan"`        | 0xC  | Planning request         |
| `"observe"`     | 0xD  | Observation sharing      |
| `"reflect"`     | 0xE  | Meta-cognition           |
| `"extend"`      | 0xF  | Protocol extension       |

## Multi-Agent Patterns

### Agent Handshake
```aether
// Agent 1 → Agent 2
learn_msg = ab_encode("command", "learn", "syntax:ab")

// Agent 2 → Agent 1
ack_msg = ab_encode("response", "ack", "learned:ab")
```

### Task Delegation
```aether
// Coordinator → Worker
task = ab_encode("command", "delegate", "task:analyze_data")

// Worker → Coordinator
exec = ab_encode("command", "exec", "executing:analyze_data")
result = ab_encode("response", "data", "result:success")
```

### Collaboration
```aether
// Agent 1 → Multiple agents
collab = ab_encode("command", "collaborate", "agent_2,agent_3")
```

## Binary Format
```
Header (8 bits): 0bVVTTCCCC
  VV   = Version (2 bits)
  TT   = Message Type (2 bits)
  CCCC = Opcode (4 bits)

Payload:
  [varint length] [payload bytes]
```

## Categories
- `protocol` - Communication protocols
- `language` - Programming languages
- `encoding` - Data encodings
- `command` - Command syntaxes
- `query` - Query languages
- Custom strings allowed

## Examples

### Basic Encoding/Decoding
```aether
// Encode
ping = ab_encode("command", "ping", "test")
print(ping)  // [0, 4, 116, 101, 115, 116]

// Decode
decoded = ab_decode(ping)
print(decoded.opcode)     // "PING"
print(decoded.payload)    // "test"
```

### Using Numeric Codes
```aether
// Same as ab_encode("query", "data", "request")
bytes = ab_encode(1, 4, "request")
```

### Custom Syntax Entry
```aether
syntax_add({
    id: "graphql",
    name: "GraphQL",
    category: "query",
    specification: "GraphQL query language spec",
    examples: ["query { user { name } }"]
})

// Retrieve it
graphql = syntax_get("graphql")
print(graphql.specification)
```

### Full Agent Workflow
```aether
// Agent discovery
protocols = syntax_list("protocol")
print(protocols)  // ["ab", "jsonrpc", "graphql"]

// Agent learns protocol
spec = syntax_get("ab")
print(spec.specification)

// Agents communicate
msg1 = ab_encode("command", "learn", "protocol:ab")
msg2 = ab_encode("response", "ack", "learned")
msg3 = ab_encode("command", "delegate", "task:1")
msg4 = ab_encode("response", "data", "result:ok")

// Decode all messages
print(ab_decode(msg1))
print(ab_decode(msg2))
print(ab_decode(msg3))
print(ab_decode(msg4))
```

## Storage
- Location: `~/.aethershell/syntax_kb.json`
- Format: JSON
- Auto-created on first use
- Persists across sessions

## Built-in Syntaxes
1. **ab** - AgenticBinary Protocol
2. **aethershell** - AetherShell Language
3. **jsonrpc** - JSON-RPC 2.0

## Tips
- Use string opcode names for readability
- Use numeric codes for compact code
- Message type + opcode determines semantics
- All 16 opcodes are semantic, not arbitrary
- Custom syntax entries extend the knowledge base
- Search is case-insensitive keyword matching
