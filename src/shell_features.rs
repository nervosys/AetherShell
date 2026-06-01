// src/shell_features.rs
//! Modern shell features inspired by Fish, PowerShell, and Nushell
//! with native AI integration

use crate::{env::Env, value::Value};
use anyhow::{anyhow, Result};
use std::collections::{HashMap, VecDeque};

/// Fish-style command history and autosuggestions
#[derive(Debug, Clone)]
pub struct CommandHistory {
    history: VecDeque<HistoryEntry>,
    max_entries: usize,
    frequency_map: HashMap<String, u32>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct HistoryEntry {
    command: String,
    timestamp: u64,
    exit_code: i32,
    working_dir: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AutoSuggestion {
    suggestion: String,
    confidence: f32,
    source: SuggestionSource,
}

#[derive(Debug, Clone)]
pub enum SuggestionSource {
    History,
    AI,
    Pattern,
    Builtin,
}

impl CommandHistory {
    pub fn new(max_entries: usize) -> Self {
        Self {
            history: VecDeque::with_capacity(max_entries),
            max_entries,
            frequency_map: HashMap::new(),
        }
    }

    /// Add a command to history (Fish-style)
    pub fn add_command(&mut self, command: String, exit_code: i32, working_dir: String) {
        let entry = HistoryEntry {
            command: command.clone(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            exit_code,
            working_dir,
        };

        // Remove oldest if at capacity
        if self.history.len() >= self.max_entries {
            if let Some(removed) = self.history.pop_front() {
                // Decrease frequency count
                if let Some(count) = self.frequency_map.get_mut(&removed.command) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        self.frequency_map.remove(&removed.command);
                    }
                }
            }
        }

        // Add to frequency map
        *self.frequency_map.entry(command.clone()).or_insert(0) += 1;

        self.history.push_back(entry);
    }

    /// Get suggestions for partial command (Fish-style)
    pub fn get_suggestions(&self, partial: &str) -> Vec<AutoSuggestion> {
        let mut suggestions = Vec::new();

        // History-based suggestions (most frequent first)
        let mut history_matches: Vec<_> = self
            .frequency_map
            .iter()
            .filter(|(cmd, _)| cmd.starts_with(partial) && cmd.as_str() != partial)
            .collect();

        history_matches.sort_by(|a, b| b.1.cmp(a.1)); // Sort by frequency descending

        for (command, frequency) in history_matches.into_iter().take(3) {
            let confidence = (*frequency as f32 / self.history.len() as f32).min(1.0);
            suggestions.push(AutoSuggestion {
                suggestion: command.clone(),
                confidence,
                source: SuggestionSource::History,
            });
        }

        suggestions
    }

    /// Get AI-enhanced suggestions
    pub async fn get_ai_suggestions(
        &self,
        partial: &str,
        _context: &str,
    ) -> Result<Vec<AutoSuggestion>> {
        // This would integrate with the AI system
        // For now, return pattern-based suggestions
        let mut suggestions = Vec::new();

        // Pattern-based suggestions (Nushell-style)
        if partial.contains("|") {
            suggestions.extend(self.get_pipeline_suggestions(partial));
        }

        // Command completion suggestions
        suggestions.extend(self.get_command_completion_suggestions(partial));

        Ok(suggestions)
    }

    fn get_pipeline_suggestions(&self, partial: &str) -> Vec<AutoSuggestion> {
        let mut suggestions = Vec::new();

        if partial.ends_with("| ") {
            // Suggest common pipeline commands
            for cmd in &["map", "where", "sort", "uniq", "head", "tail", "grep", "ai"] {
                suggestions.push(AutoSuggestion {
                    suggestion: format!("{}{}", partial, cmd),
                    confidence: 0.8,
                    source: SuggestionSource::Pattern,
                });
            }
        }

        suggestions
    }

    fn get_command_completion_suggestions(&self, partial: &str) -> Vec<AutoSuggestion> {
        let builtins = [
            "ls", "cat", "head", "tail", "grep", "find", "sort", "uniq", "wc", "map", "where",
            "reduce", "take", "print", "echo", "pwd", "ai", "agent", "swarm",
        ];

        builtins
            .iter()
            .filter(|cmd| cmd.starts_with(partial) && **cmd != partial)
            .map(|cmd| AutoSuggestion {
                suggestion: cmd.to_string(),
                confidence: 0.9,
                source: SuggestionSource::Builtin,
            })
            .collect()
    }
}

/// PowerShell-style parameter binding and validation
#[derive(Debug, Clone)]
pub struct ParameterSet {
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub pipeline_input: Option<PipelineInputType>,
}

#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: String,
    pub aliases: Vec<String>,
    pub parameter_type: ParameterType,
    pub required: bool,
    pub position: Option<u32>,
    pub help_text: String,
}

#[derive(Debug, Clone)]
pub enum ParameterType {
    String,
    Int,
    Float,
    Bool,
    Array,
    Record,
    Switch, // PowerShell-style switch parameters
}

#[derive(Debug, Clone)]
pub enum PipelineInputType {
    ByValue(ParameterType),
    ByPropertyName,
}

/// Nushell-style structured data processing
#[derive(Debug, Clone)]
pub struct DataProcessor {
    pub type_info: HashMap<String, DataType>,
}

#[derive(Debug, Clone)]
pub enum DataType {
    Text,
    Number,
    Boolean,
    Date,
    Duration,
    FileSize,
    Structured(HashMap<String, DataType>),
}

impl Default for DataProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl DataProcessor {
    pub fn new() -> Self {
        Self {
            type_info: HashMap::new(),
        }
    }

    /// Infer data types from values (Nushell-style)
    pub fn infer_type(&mut self, value: &Value) -> DataType {
        match value {
            Value::Str(s) => {
                // Smart type inference
                if s.parse::<i64>().is_ok() {
                    DataType::Number
                } else if s.parse::<f64>().is_ok() {
                    DataType::Number
                } else if s.parse::<bool>().is_ok() {
                    DataType::Boolean
                } else if s.ends_with("KB") || s.ends_with("MB") || s.ends_with("GB") {
                    DataType::FileSize
                } else {
                    DataType::Text
                }
            }
            Value::Int(_) | Value::Float(_) => DataType::Number,
            Value::Bool(_) => DataType::Boolean,
            Value::Array(_) => DataType::Text, // Could be more sophisticated
            Value::Record(map) => {
                let mut fields = HashMap::new();
                for (key, val) in map {
                    fields.insert(key.clone(), self.infer_type(val));
                }
                DataType::Structured(fields)
            }
            _ => DataType::Text,
        }
    }

    /// Convert values based on type information
    pub fn convert_value(&self, value: &Value, target_type: &DataType) -> Result<Value> {
        match (value, target_type) {
            (Value::Str(s), DataType::Number) => {
                if let Ok(i) = s.parse::<i64>() {
                    Ok(Value::Int(i))
                } else if let Ok(f) = s.parse::<f64>() {
                    Ok(Value::Float(f))
                } else {
                    Err(anyhow!("Cannot convert '{}' to number", s))
                }
            }
            (Value::Str(s), DataType::Boolean) => match s.to_lowercase().as_str() {
                "true" | "yes" | "1" | "on" => Ok(Value::Bool(true)),
                "false" | "no" | "0" | "off" => Ok(Value::Bool(false)),
                _ => Err(anyhow!("Cannot convert '{}' to boolean", s)),
            },
            (value, _) => Ok(value.clone()),
        }
    }
}

/// AI-powered error explanations (Fish + AI)
#[derive(Debug, Clone)]
pub struct ErrorExplainer;

impl ErrorExplainer {
    /// Generate helpful error explanations with AI enhancement
    pub fn explain_error(&self, error: &str, command: &str, context: &str) -> String {
        // This would integrate with AI for contextual explanations
        // For now, provide pattern-based helpful messages

        if error.contains("unknown builtin") {
            format!(
                "🤖 Command '{}' not found. Did you mean:\n  • Similar commands: {}\n  • Try 'help' to see available commands\n  • AI suggestion: {}",
                command,
                self.suggest_similar_commands(command),
                self.ai_suggest_command(command, context)
            )
        } else if error.contains("requires array input") {
            format!(
                "🔧 Type mismatch: '{}' expects an array but got a different type.\n  • Try: your_data | to_array | {}\n  • Or: [your_data] | {}",
                command, command, command
            )
        } else if error.contains("file not found") || error.contains("No such file") {
            format!(
                "📁 File not found. AI suggestions:\n  • Check if the file path is correct\n  • Use 'ls' to see available files\n  • Try: find . \"*{}*\" to search for similar files",
                command
            )
        } else {
            format!(
                "❓ {}\n💡 Try 'ai help \"{}\"' for contextual assistance",
                error, error
            )
        }
    }

    fn suggest_similar_commands(&self, command: &str) -> String {
        let builtins = [
            "ls", "cat", "head", "tail", "grep", "find", "sort", "uniq", "wc", "map", "where",
            "reduce", "take", "print", "echo", "pwd",
        ];

        let mut similar = Vec::new();
        for builtin in &builtins {
            if self.levenshtein_distance(command, builtin) <= 2 {
                similar.push(*builtin);
            }
        }

        if similar.is_empty() {
            "ls, cat, grep".to_string()
        } else {
            similar.join(", ")
        }
    }

    fn ai_suggest_command(&self, _command: &str, _context: &str) -> String {
        // Placeholder for AI integration
        "Use 'ai \"what command should I use to...\"' for intelligent suggestions".to_string()
    }

    fn levenshtein_distance(&self, s1: &str, s2: &str) -> usize {
        let len1 = s1.len();
        let len2 = s2.len();
        let mut matrix = vec![vec![0; len2 + 1]; len1 + 1];

        for i in 0..=len1 {
            matrix[i][0] = i;
        }
        for j in 0..=len2 {
            matrix[0][j] = j;
        }

        for i in 1..=len1 {
            for j in 1..=len2 {
                let cost = if s1.chars().nth(i - 1) == s2.chars().nth(j - 1) {
                    0
                } else {
                    1
                };
                matrix[i][j] = (matrix[i - 1][j] + 1)
                    .min(matrix[i][j - 1] + 1)
                    .min(matrix[i - 1][j - 1] + cost);
            }
        }

        matrix[len1][len2]
    }
}

/// PowerShell-style cmdlet structure
#[derive(Debug, Clone)]
pub struct Cmdlet {
    pub verb: String,
    pub noun: String,
    pub parameter_sets: Vec<ParameterSet>,
    pub synopsis: String,
    pub description: String,
}

impl Cmdlet {
    pub fn new(verb: &str, noun: &str) -> Self {
        Self {
            verb: verb.to_string(),
            noun: noun.to_string(),
            parameter_sets: Vec::new(),
            synopsis: String::new(),
            description: String::new(),
        }
    }

    pub fn full_name(&self) -> String {
        format!("{}-{}", self.verb, self.noun)
    }
}

/// Global shell features manager
pub struct ShellFeatures {
    pub history: CommandHistory,
    pub data_processor: DataProcessor,
    pub error_explainer: ErrorExplainer,
    pub cmdlets: HashMap<String, Cmdlet>,
}

impl Default for ShellFeatures {
    fn default() -> Self {
        Self::new()
    }
}

impl ShellFeatures {
    pub fn new() -> Self {
        let mut features = Self {
            history: CommandHistory::new(1000),
            data_processor: DataProcessor::new(),
            error_explainer: ErrorExplainer,
            cmdlets: HashMap::new(),
        };

        features.register_default_cmdlets();
        features
    }

    fn register_default_cmdlets(&mut self) {
        // PowerShell-style cmdlets
        let mut get_files = Cmdlet::new("Get", "Files");
        get_files.synopsis = "Gets files and directories from the specified path".to_string();
        get_files.parameter_sets.push(ParameterSet {
            name: "Path".to_string(),
            parameters: vec![Parameter {
                name: "Path".to_string(),
                aliases: vec!["P".to_string()],
                parameter_type: ParameterType::String,
                required: false,
                position: Some(0),
                help_text: "The path to list files from".to_string(),
            }],
            pipeline_input: Some(PipelineInputType::ByValue(ParameterType::String)),
        });
        self.cmdlets.insert("Get-Files".to_string(), get_files);

        let mut get_content = Cmdlet::new("Get", "Content");
        get_content.synopsis = "Gets the content of a file".to_string();
        self.cmdlets.insert("Get-Content".to_string(), get_content);

        let mut select_object = Cmdlet::new("Select", "Object");
        select_object.synopsis = "Selects specified properties of objects".to_string();
        self.cmdlets
            .insert("Select-Object".to_string(), select_object);
    }

    /// Get AI-enhanced command suggestions
    pub async fn get_smart_suggestions(
        &mut self,
        partial: &str,
        _context: &Env,
    ) -> Result<Vec<AutoSuggestion>> {
        let mut suggestions = self.history.get_suggestions(partial);

        // Add AI suggestions
        let working_dir = std::env::current_dir()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let ai_suggestions = self
            .history
            .get_ai_suggestions(partial, &working_dir)
            .await?;
        suggestions.extend(ai_suggestions);

        // Sort by confidence
        suggestions.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(suggestions)
    }
}
