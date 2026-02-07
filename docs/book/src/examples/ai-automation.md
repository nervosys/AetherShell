# AI Automation

Examples of using AetherShell's AI capabilities for automation tasks.

## Code Documentation Generator

```aethershell
# Generate documentation for all Rust files
ls "src"
  | where(fn(f) => f.extension == "rs")
  | map(fn(f) => {
      let code = cat f.path
      let doc = ai "Write a brief module-level doc comment for this Rust file. Include purpose, key types, and public API:\n\n${code}" {
        model: "openai:gpt-4o-mini"
      }
      { file: f.name, documentation: doc }
  })
  | each(fn(d) => {
      echo "## ${d.file}\n${d.documentation}\n"
  })
```

## Intelligent Log Analysis

```aethershell
# Index logs for RAG
let errors = cat "app.log" | split "\n" | where(fn(l) => contains l "ERROR")
errors | each(fn(e) => rag_index e "app.log")

# Ask questions about the errors
let ctx = rag_query "What are the most common error patterns?" 5
ai "Based on these log entries:\n${ctx.context}\n\nIdentify the top 3 error patterns and suggest fixes."
```

## Automated Code Review

```aethershell
# Agent-powered code review
agent {
  goal: "Review all .rs files in src/ for: 1) unwrap() calls that should use ? or expect(), 2) TODO/FIXME comments, 3) functions over 50 lines. Provide a summary with file:line references.",
  tools: ["ls", "cat", "grep", "wc"],
  max_steps: 15
}
```

## Commit Message Generator

```aethershell
# Generate a commit message from the current diff
let diff = sh "git diff --staged"

if len(diff) > 0 {
  let msg = ai "Write a concise conventional commit message for this diff. Use format: type(scope): description\n\nDiff:\n${diff}" {
    model: "openai:gpt-4o-mini"
  }
  echo "Suggested commit message:"
  echo msg
} else {
  echo "No staged changes"
}
```

## RAG-Powered Q&A System

```aethershell
# Step 1: Index your project docs
ls "docs" | where(fn(f) => f.extension == "md")
  | each(fn(f) => {
      echo "Indexing ${f.name}..."
      rag_index (cat f.path) f.name
  })

# Step 2: Interactive Q&A
let question = "How do I create a custom pipeline?"
let cached = semantic_cache_get question

let answer = if cached.hit {
  echo "(cached)"
  cached.response
} else {
  let ctx = rag_query question 5
  let resp = ai "Context:\n${ctx.context}\n\nQuestion: ${question}\n\nAnswer based only on the context above."
  semantic_cache question resp
  resp
}

echo answer
```

## Knowledge Graph Builder

```aethershell
# Build a project knowledge graph from source code
ls "src" | where(fn(f) => f.extension == "rs") | each(fn(f) => {
  # Add each file as an entity
  let file_id = (kg_add "File" f.name { path: f.path, size: f.size }).entity_id

  # Find imports
  let imports = cat f.path | split "\n"
    | where(fn(l) => starts_with (trim l) "use crate::")
    | map(fn(l) => trim l | replace "use crate::" "" | replace ";" "")

  imports | each(fn(imp) => {
    let mod_id = (kg_add "Module" imp {}).entity_id
    kg_relate file_id mod_id "imports" {}
  })
})

# Query the graph
echo "Files importing eval:"
kg_query "eval"
```

## Multi-Agent Analysis

```aethershell
# Swarm-based project analysis
swarm {
  goal: "Perform a comprehensive analysis: 1) Code quality assessment, 2) Dependency audit, 3) Test coverage estimate, 4) Documentation completeness. Produce a structured report.",
  tools: ["ls", "cat", "grep", "find", "wc", "fs_tree"],
  max_steps: 30
}
```

## Automated Testing

```aethershell
# Generate test cases with AI
let source = cat "src/parser.rs"
let tests = ai "Generate 5 unit test functions in Rust for the parser module. Cover edge cases:\n\n${source}" {
  model: "openai:gpt-4o"
}

file_write "tests/generated_parser.rs" tests
echo "Generated test file"
```

## Batch Classification

```aethershell
# Classify support tickets
let tickets = web_json_get "https://api.example.com/tickets?status=new"

tickets | map(fn(t) => {
  let category = ai "Classify this support ticket into one of: bug, feature, question, billing. Respond with only the category.\n\nTicket: ${t.description}" {
    model: "openai:gpt-4o-mini"
  }
  { ...t, category: trim(category) }
}) | save_json "classified_tickets.json"
```
