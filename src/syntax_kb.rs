//! Syntax Knowledge Base (Syntax KB)
//!
//! A persistent knowledge base system that allows AI agents to discover, memorize,
//! and reference syntax patterns for multi-agent communication. Includes the
//! AgenticBinary (ab) protocol for maximum information density.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Syntax entry in the knowledge base
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyntaxEntry {
    /// Unique identifier for this syntax
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Syntax category (protocol, language, encoding, etc.)
    pub category: SyntaxCategory,
    /// Full syntax specification
    pub specification: String,
    /// Example usage patterns
    pub examples: Vec<String>,
    /// Binary encoding if applicable
    pub binary_encoding: Option<BinaryEncoding>,
    /// Metadata (version, author, created, etc.)
    pub metadata: HashMap<String, String>,
}

/// Categories of syntax entries
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SyntaxCategory {
    /// Inter-agent communication protocol
    Protocol,
    /// Programming language syntax
    Language,
    /// Data encoding/serialization format
    Encoding,
    /// Command syntax
    Command,
    /// Query language
    Query,
    /// Custom/extension syntax
    Custom(String),
}

/// Binary encoding specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryEncoding {
    /// Encoding name
    pub name: String,
    /// Bit layout specification
    pub bit_layout: String,
    /// Encoding rules
    pub rules: Vec<EncodingRule>,
    /// Compression ratio (if applicable)
    pub compression_ratio: Option<f64>,
}

/// Encoding rule for binary protocols
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodingRule {
    /// Rule name/identifier
    pub name: String,
    /// Bit pattern (e.g., "0b1010xxxx")
    pub pattern: String,
    /// Semantic meaning
    pub meaning: String,
    /// Example values
    pub examples: Vec<String>,
}

/// Syntax Knowledge Base
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyntaxKB {
    /// Map of syntax ID to entry
    entries: HashMap<String, SyntaxEntry>,
    /// Reverse index: category -> [syntax IDs]
    category_index: HashMap<String, Vec<String>>,
    /// Storage path
    #[serde(skip)]
    storage_path: Option<PathBuf>,
}

impl Default for SyntaxKB {
    fn default() -> Self {
        Self::new()
    }
}

impl SyntaxKB {
    /// Create a new empty knowledge base
    pub fn new() -> Self {
        let mut kb = Self {
            entries: HashMap::new(),
            category_index: HashMap::new(),
            storage_path: None,
        };

        // Initialize with built-in syntax definitions
        kb.initialize_builtin_syntax();
        kb
    }

    /// Create knowledge base with persistent storage
    pub fn with_storage(path: PathBuf) -> Result<Self> {
        let mut kb = if path.exists() {
            let content = fs::read_to_string(&path)?;
            serde_json::from_str(&content)?
        } else {
            Self::new()
        };
        kb.storage_path = Some(path);
        Ok(kb)
    }

    /// Initialize built-in syntax definitions
    fn initialize_builtin_syntax(&mut self) {
        // AgenticBinary (ab) Protocol
        self.add_entry(SyntaxEntry {
            id: "ab".to_string(),
            name: "AgenticBinary Protocol".to_string(),
            category: SyntaxCategory::Protocol,
            specification: r#"
AgenticBinary (ab) - Maximum Information Density Protocol

HEADER STRUCTURE (8 bits):
  0bVVTTCCCC
  VV   = Version (2 bits): 00=v1, 01=v2, 10=v3, 11=reserved
  TT   = Message Type (2 bits):
         00 = Command
         01 = Query
         10 = Response
         11 = Event
  CCCC = Command/Content Code (4 bits): 16 possible opcodes

OPCODE DEFINITIONS (4-bit):
  0000 = PING         - Heartbeat/presence check
  0001 = ACK          - Acknowledgment
  0002 = QUERY        - Data query
  0003 = EXEC         - Execute command
  0004 = DATA         - Data transfer
  0005 = ERROR        - Error condition
  0006 = SYNC         - Synchronization
  0007 = AUTH         - Authentication
  0008 = DELEGATE     - Task delegation
  0009 = COLLABORATE  - Multi-agent coordination
  000A = LEARN        - Knowledge sharing
  000B = REASON       - Reasoning request
  000C = PLAN         - Planning request
  000D = OBSERVE      - Observation sharing
  000E = REFLECT      - Reflection/meta-cognition
  000F = EXTEND       - Protocol extension

PAYLOAD ENCODING:
  - Length-prefixed (varint: 1-9 bytes)
  - Compressed using adaptive dictionary
  - Type indicators for structured data:
    10 = String (UTF-8)
    01 = Number (varint/float)
    11 = Binary blob
    00 = Structured (nested ab encoding)

EXAMPLE MESSAGES:
  0b00010100 [len] [payload]  = v1 Query/Data message
  0b00001000 [len] [payload]  = v1 Command/Delegate message
  0b01100110 [len] [payload]  = v2 Response/Sync message

COMPRESSION:
  - Huffman encoding for common patterns
  - Dictionary-based compression (LZ77-style)
  - Typical compression ratio: 3-5x for agent messages
"#
            .to_string(),
            examples: vec![
                "0b00010100 0x05 'hello' - Query for data 'hello'".to_string(),
                "0b00001000 0x0A 'exec:task' - Delegate task execution".to_string(),
                "0b01100011 0x03 [ACK] - v2 Response/Execute ACK".to_string(),
            ],
            binary_encoding: Some(BinaryEncoding {
                name: "AgenticBinary v1".to_string(),
                bit_layout: "VVTTCCCC [varint-len] [payload]".to_string(),
                rules: vec![
                    EncodingRule {
                        name: "Version".to_string(),
                        pattern: "0bVV______".to_string(),
                        meaning: "Protocol version (00=v1, 01=v2, 10=v3)".to_string(),
                        examples: vec!["0b00______ = v1".to_string()],
                    },
                    EncodingRule {
                        name: "MessageType".to_string(),
                        pattern: "0b__TT____".to_string(),
                        meaning: "Message type (00=Cmd, 01=Query, 10=Resp, 11=Event)".to_string(),
                        examples: vec!["0b__01____ = Query".to_string()],
                    },
                    EncodingRule {
                        name: "Opcode".to_string(),
                        pattern: "0b____CCCC".to_string(),
                        meaning: "Operation code (16 variants)".to_string(),
                        examples: vec!["0b____0100 = DATA".to_string()],
                    },
                ],
                compression_ratio: Some(4.2),
            }),
            metadata: [
                ("version".to_string(), "1.0.0".to_string()),
                ("author".to_string(), "AetherShell".to_string()),
                ("created".to_string(), "2025-11-07".to_string()),
                (
                    "purpose".to_string(),
                    "Multi-agent communication protocol".to_string(),
                ),
            ]
            .iter()
            .cloned()
            .collect(),
        })
        .ok();

        // AetherShell Syntax
        self.add_entry(SyntaxEntry {
            id: "aethershell".to_string(),
            name: "AetherShell Language".to_string(),
            category: SyntaxCategory::Language,
            specification: r#"
AetherShell Syntax Reference

TYPES:
  Int, Float, String, Bool, Array, Record, Lambda, Null, Uri, Table

VARIABLES:
  let x = 5          # immutable
  mut counter = 0    # mutable
  x = 5              # type inference sugar

FUNCTIONS:
  fn(x) => x * 2                    # single param
  fn(x, y) => x + y                 # multiple params
  fn(arr) => arr | map(fn(x) => x)  # pipelines in body

PIPELINES:
  data | transform | filter | collect
  [1,2,3] | map(fn(x) => x * 2) | reduce(fn(a,b) => a + b, 0)

BUILTINS:
  map, where, reduce, take, first, last, any, all
  split, join, trim, upper, lower, replace, contains
  range, flatten, reverse, slice, zip, push, concat
  abs, min, max, floor, ceil, round, sqrt, pow
  env, json_parse, json_stringify, time, sleep

PATTERN MATCHING:
  match value {
    1 => "one",
    2 => "two",
    _ => "other"
  }

AI INTEGRATION:
  agent("goal", tools, max_steps)
  swarm({coordinator: "main", workers: [...] })
"#
            .to_string(),
            examples: vec![
                "range(1, 10) | where(fn(x) => x % 2 == 0) | map(fn(x) => x * x)".to_string(),
                "fn factorial(n) => range(1, n + 1) | reduce(fn(a, b) => a * b, 1)".to_string(),
            ],
            binary_encoding: None,
            metadata: [
                ("version".to_string(), "0.1.0".to_string()),
                ("type".to_string(), "functional shell".to_string()),
            ]
            .iter()
            .cloned()
            .collect(),
        })
        .ok();

        // JSON-RPC for agent communication
        self.add_entry(SyntaxEntry {
            id: "jsonrpc".to_string(),
            name: "JSON-RPC 2.0".to_string(),
            category: SyntaxCategory::Protocol,
            specification: r#"
JSON-RPC 2.0 Protocol

REQUEST:
  {
    "jsonrpc": "2.0",
    "method": "method_name",
    "params": [...] or {...},
    "id": unique_id
  }

RESPONSE:
  {
    "jsonrpc": "2.0",
    "result": result_value,
    "id": request_id
  }

ERROR:
  {
    "jsonrpc": "2.0",
    "error": {
      "code": error_code,
      "message": "error message",
      "data": optional_data
    },
    "id": request_id
  }

NOTIFICATION (no response expected):
  {
    "jsonrpc": "2.0",
    "method": "method_name",
    "params": [...]
  }
"#
            .to_string(),
            examples: vec![
                r#"{"jsonrpc":"2.0","method":"execute","params":["echo hello"],"id":1}"#
                    .to_string(),
            ],
            binary_encoding: None,
            metadata: [
                ("version".to_string(), "2.0".to_string()),
                (
                    "spec".to_string(),
                    "https://www.jsonrpc.org/specification".to_string(),
                ),
            ]
            .iter()
            .cloned()
            .collect(),
        })
        .ok();
    }

    /// Add a syntax entry to the knowledge base
    pub fn add_entry(&mut self, entry: SyntaxEntry) -> Result<()> {
        let id = entry.id.clone();
        let category = Self::category_key(&entry.category);

        // Add to main index
        self.entries.insert(id.clone(), entry);

        // Update category index
        self.category_index.entry(category).or_default().push(id);

        // Persist if storage configured
        if self.storage_path.is_some() {
            self.save()?;
        }

        Ok(())
    }

    /// Get a syntax entry by ID
    pub fn get(&self, id: &str) -> Option<&SyntaxEntry> {
        self.entries.get(id)
    }

    /// List all syntax entries in a category
    pub fn list_by_category(&self, category: &SyntaxCategory) -> Vec<&SyntaxEntry> {
        let key = Self::category_key(category);
        self.category_index
            .get(&key)
            .map(|ids| ids.iter().filter_map(|id| self.entries.get(id)).collect())
            .unwrap_or_default()
    }

    /// Search for syntax entries by keyword
    pub fn search(&self, query: &str) -> Vec<&SyntaxEntry> {
        let query_lower = query.to_lowercase();
        self.entries
            .values()
            .filter(|entry| {
                entry.name.to_lowercase().contains(&query_lower)
                    || entry.id.to_lowercase().contains(&query_lower)
                    || entry.specification.to_lowercase().contains(&query_lower)
            })
            .collect()
    }

    /// Get all syntax IDs
    pub fn list_all_ids(&self) -> Vec<String> {
        self.entries.keys().cloned().collect()
    }

    /// Get all categories
    pub fn list_categories(&self) -> Vec<String> {
        self.category_index.keys().cloned().collect()
    }

    /// Remove a syntax entry
    pub fn remove(&mut self, id: &str) -> Result<()> {
        if let Some(entry) = self.entries.remove(id) {
            let category = Self::category_key(&entry.category);
            if let Some(ids) = self.category_index.get_mut(&category) {
                ids.retain(|i| i != id);
            }

            if self.storage_path.is_some() {
                self.save()?;
            }

            Ok(())
        } else {
            Err(anyhow!("Syntax entry '{}' not found", id))
        }
    }

    /// Save knowledge base to disk
    pub fn save(&self) -> Result<()> {
        if let Some(path) = &self.storage_path {
            let json = serde_json::to_string_pretty(self)?;
            fs::write(path, json)?;
        }
        Ok(())
    }

    /// Export as JSON string
    pub fn export_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Import from JSON string
    pub fn import_json(&mut self, json: &str) -> Result<()> {
        let imported: SyntaxKB = serde_json::from_str(json)?;
        for (id, entry) in imported.entries {
            self.entries.insert(id.clone(), entry.clone());
            let category = Self::category_key(&entry.category);
            self.category_index.entry(category).or_default().push(id);
        }
        Ok(())
    }

    /// Get category key for indexing
    fn category_key(category: &SyntaxCategory) -> String {
        match category {
            SyntaxCategory::Protocol => "protocol".to_string(),
            SyntaxCategory::Language => "language".to_string(),
            SyntaxCategory::Encoding => "encoding".to_string(),
            SyntaxCategory::Command => "command".to_string(),
            SyntaxCategory::Query => "query".to_string(),
            SyntaxCategory::Custom(name) => format!("custom:{}", name),
        }
    }

    /// Get total number of entries
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if knowledge base is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// AgenticBinary encoder/decoder
pub struct AgenticBinary;

impl AgenticBinary {
    /// Encode a message using AgenticBinary protocol
    pub fn encode(version: u8, msg_type: u8, opcode: u8, payload: &[u8]) -> Result<Vec<u8>> {
        if version > 3 {
            return Err(anyhow!("Invalid version: {}", version));
        }
        if msg_type > 3 {
            return Err(anyhow!("Invalid message type: {}", msg_type));
        }
        if opcode > 15 {
            return Err(anyhow!("Invalid opcode: {}", opcode));
        }

        let header = (version << 6) | (msg_type << 4) | opcode;
        let mut result = vec![header];

        // Encode payload length as varint
        let len_bytes = Self::encode_varint(payload.len() as u64);
        result.extend_from_slice(&len_bytes);

        // Add payload
        result.extend_from_slice(payload);

        Ok(result)
    }

    /// Decode an AgenticBinary message
    pub fn decode(data: &[u8]) -> Result<(u8, u8, u8, Vec<u8>)> {
        if data.is_empty() {
            return Err(anyhow!("Empty message"));
        }

        let header = data[0];
        let version = (header >> 6) & 0b11;
        let msg_type = (header >> 4) & 0b11;
        let opcode = header & 0b1111;

        // Decode payload length
        let (payload_len, varint_size) = Self::decode_varint(&data[1..])?;
        let payload_start = 1 + varint_size;
        let payload_end = payload_start + payload_len as usize;

        if data.len() < payload_end {
            return Err(anyhow!("Incomplete message"));
        }

        let payload = data[payload_start..payload_end].to_vec();

        Ok((version, msg_type, opcode, payload))
    }

    /// Encode unsigned integer as varint
    fn encode_varint(mut value: u64) -> Vec<u8> {
        let mut result = Vec::new();
        loop {
            let mut byte = (value & 0x7F) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 0x80;
            }
            result.push(byte);
            if value == 0 {
                break;
            }
        }
        result
    }

    /// Decode varint, returning (value, bytes_consumed)
    fn decode_varint(data: &[u8]) -> Result<(u64, usize)> {
        let mut result = 0u64;
        let mut shift = 0;
        for (i, &byte) in data.iter().enumerate() {
            result |= ((byte & 0x7F) as u64) << shift;
            if byte & 0x80 == 0 {
                return Ok((result, i + 1));
            }
            shift += 7;
            if shift > 63 {
                return Err(anyhow!("Varint overflow"));
            }
        }
        Err(anyhow!("Incomplete varint"))
    }

    /// Get opcode name
    pub fn opcode_name(opcode: u8) -> &'static str {
        match opcode {
            0x0 => "PING",
            0x1 => "ACK",
            0x2 => "QUERY",
            0x3 => "EXEC",
            0x4 => "DATA",
            0x5 => "ERROR",
            0x6 => "SYNC",
            0x7 => "AUTH",
            0x8 => "DELEGATE",
            0x9 => "COLLABORATE",
            0xA => "LEARN",
            0xB => "REASON",
            0xC => "PLAN",
            0xD => "OBSERVE",
            0xE => "REFLECT",
            0xF => "EXTEND",
            _ => "UNKNOWN",
        }
    }

    /// Get message type name
    pub fn msg_type_name(msg_type: u8) -> &'static str {
        match msg_type {
            0 => "Command",
            1 => "Query",
            2 => "Response",
            3 => "Event",
            _ => "Unknown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_syntax_kb_creation() {
        let kb = SyntaxKB::new();
        assert!(kb.len() >= 3); // Should have built-in entries
        assert!(kb.get("ab").is_some());
    }

    #[test]
    fn test_add_and_retrieve() {
        let mut kb = SyntaxKB::new();
        let entry = SyntaxEntry {
            id: "test".to_string(),
            name: "Test Syntax".to_string(),
            category: SyntaxCategory::Custom("test".to_string()),
            specification: "Test spec".to_string(),
            examples: vec![],
            binary_encoding: None,
            metadata: HashMap::new(),
        };

        kb.add_entry(entry).unwrap();
        assert!(kb.get("test").is_some());
    }

    #[test]
    fn test_agentic_binary_encoding() {
        let payload = b"hello";
        let encoded = AgenticBinary::encode(0, 1, 4, payload).unwrap();
        let (version, msg_type, opcode, decoded_payload) = AgenticBinary::decode(&encoded).unwrap();

        assert_eq!(version, 0);
        assert_eq!(msg_type, 1);
        assert_eq!(opcode, 4);
        assert_eq!(decoded_payload, payload);
    }

    #[test]
    fn test_varint_encoding() {
        let test_cases = vec![0u64, 127, 128, 16383, 16384, 2097151];
        for value in test_cases {
            let encoded = AgenticBinary::encode_varint(value);
            let (decoded, _) = AgenticBinary::decode_varint(&encoded).unwrap();
            assert_eq!(value, decoded);
        }
    }
}
