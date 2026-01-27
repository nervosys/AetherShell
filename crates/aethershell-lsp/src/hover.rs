//! Hover information for AetherShell
//!
//! Provides documentation and type info on hover

use tower_lsp::lsp_types::*;

use crate::document::DocumentStore;

/// Builtin documentation
const BUILTIN_DOCS: &[(&str, &str, &str)] = &[
    ("print", "fn(value: Any) -> String", "Prints the given value to stdout and returns a string representation.\n\n**Example:**\n```aethershell\nprint(\"Hello, World!\")\n[1, 2, 3] | print\n```"),
    ("echo", "fn(value: Any) -> String", "Alias for print. Outputs the value to stdout.\n\n**Example:**\n```aethershell\necho \"Hello\"\n```"),
    ("map", "fn(fn(x: T) -> U) -> Array<U>", "Transforms each element in an array using the provided function.\n\n**Example:**\n```aethershell\n[1, 2, 3] | map(fn(x) => x * 2)  // [2, 4, 6]\n```"),
    ("where", "fn(fn(x: T) -> Bool) -> Array<T>", "Filters elements in an array that satisfy the predicate.\n\n**Example:**\n```aethershell\n[1, 2, 3, 4, 5] | where(fn(x) => x > 2)  // [3, 4, 5]\n```"),
    ("reduce", "fn(fn(acc: U, x: T) -> U, initial: U) -> U", "Reduces an array to a single value using an accumulator function.\n\n**Example:**\n```aethershell\n[1, 2, 3, 4] | reduce(fn(acc, x) => acc + x, 0)  // 10\n```"),
    ("take", "fn(n: Int) -> Array<T>", "Returns the first n elements of an array.\n\n**Example:**\n```aethershell\n[1, 2, 3, 4, 5] | take(3)  // [1, 2, 3]\n```"),
    ("first", "fn() -> T | Null", "Returns the first element of an array, or null if empty.\n\n**Example:**\n```aethershell\n[1, 2, 3] | first  // 1\n```"),
    ("last", "fn() -> T | Null", "Returns the last element of an array, or null if empty.\n\n**Example:**\n```aethershell\n[1, 2, 3] | last  // 3\n```"),
    ("len", "fn(collection: Array | String) -> Int", "Returns the length of an array or string.\n\n**Example:**\n```aethershell\nlen([1, 2, 3])  // 3\nlen(\"hello\")   // 5\n```"),
    ("type_of", "fn(value: Any) -> String", "Returns the type of a value as a string.\n\n**Example:**\n```aethershell\ntype_of(42)        // \"Int\"\ntype_of(\"hello\")   // \"String\"\ntype_of([1, 2])    // \"Array\"\n```"),
    ("ls", "fn(path?: String) -> Array<Record>", "Lists directory contents. Returns records with name, size, is_dir, modified.\n\n**Example:**\n```aethershell\nls \".\" | where(fn(f) => f.is_dir == false)\n```"),
    ("pwd", "fn() -> String", "Returns the current working directory.\n\n**Example:**\n```aethershell\npwd()  // \"/home/user/projects\"\n```"),
    ("cat", "fn(path: String) -> String", "Reads and returns the contents of a file.\n\n**Example:**\n```aethershell\ncat(\"file.txt\")\n```"),
    ("head", "fn(n?: Int) -> Array<String> | String", "Returns the first n lines (default 10).\n\n**Example:**\n```aethershell\ncat(\"log.txt\") | head(5)\n```"),
    ("tail", "fn(n?: Int) -> Array<String> | String", "Returns the last n lines (default 10).\n\n**Example:**\n```aethershell\ncat(\"log.txt\") | tail(5)\n```"),
    ("grep", "fn(pattern: String) -> Array<String>", "Filters lines matching the pattern.\n\n**Example:**\n```aethershell\ncat(\"log.txt\") | grep(\"ERROR\")\n```"),
    ("sort", "fn() -> Array<T>", "Sorts an array in ascending order.\n\n**Example:**\n```aethershell\n[3, 1, 4, 1, 5] | sort  // [1, 1, 3, 4, 5]\n```"),
    ("uniq", "fn() -> Array<T>", "Removes consecutive duplicate elements.\n\n**Example:**\n```aethershell\n[1, 1, 2, 2, 3] | uniq  // [1, 2, 3]\n```"),
    ("split", "fn(delimiter: String) -> Array<String>", "Splits a string by delimiter.\n\n**Example:**\n```aethershell\n\"a,b,c\" | split(\",\")  // [\"a\", \"b\", \"c\"]\n```"),
    ("join", "fn(delimiter: String) -> String", "Joins array elements with delimiter.\n\n**Example:**\n```aethershell\n[\"a\", \"b\", \"c\"] | join(\",\")  // \"a,b,c\"\n```"),
    ("trim", "fn() -> String", "Removes leading/trailing whitespace.\n\n**Example:**\n```aethershell\n\"  hello  \" | trim  // \"hello\"\n```"),
    ("upper", "fn() -> String", "Converts string to uppercase.\n\n**Example:**\n```aethershell\n\"hello\" | upper  // \"HELLO\"\n```"),
    ("lower", "fn() -> String", "Converts string to lowercase.\n\n**Example:**\n```aethershell\n\"HELLO\" | lower  // \"hello\"\n```"),
    ("http_get", "fn(url: String) -> Record", "Makes an HTTP GET request and returns the response.\n\n**Example:**\n```aethershell\nhttp_get(\"https://api.example.com/data\")\n```"),
    ("ai", "fn(prompt: String) -> String", "Sends a prompt to the configured AI model.\n\n**Example:**\n```aethershell\nai(\"Explain quantum computing in simple terms\")\n```"),
    ("agent", "fn(goal: String) -> Record", "Creates and runs an AI agent with the given goal.\n\n**Example:**\n```aethershell\nagent(\"Find all TODO comments in the codebase\")\n```"),
    ("swarm", "fn(config: Record) -> Record", "Runs a multi-agent swarm with the provided configuration.\n\n**Example:**\n```aethershell\nswarm({\n    agents: [\n        {id: \"analyzer\", role: \"analyze code\"},\n        {id: \"reviewer\", role: \"review changes\"}\n    ]\n})\n```"),
    ("foreach", "fn(fn(x: T) -> Any) -> Null", "Executes a function for each element in an array. Returns null.\n\n**Example:**\n```aethershell\n[1, 2, 3] | foreach(fn(x) => print(x))\n```"),
    ("range", "fn(start: Int, end: Int) -> Array<Int>", "Creates an array of integers from start to end (exclusive).\n\n**Example:**\n```aethershell\nrange(0, 5)  // [0, 1, 2, 3, 4]\n```"),
    ("keys", "fn(record: Record) -> Array<String>", "Returns the keys of a record.\n\n**Example:**\n```aethershell\nkeys({a: 1, b: 2})  // [\"a\", \"b\"]\n```"),
    ("any", "fn(fn(x: T) -> Bool) -> Bool", "Returns true if any element satisfies the predicate.\n\n**Example:**\n```aethershell\n[1, 2, 3] | any(fn(x) => x > 2)  // true\n```"),
    ("all", "fn(fn(x: T) -> Bool) -> Bool", "Returns true if all elements satisfy the predicate.\n\n**Example:**\n```aethershell\n[1, 2, 3] | all(fn(x) => x > 0)  // true\n```"),
    ("flatten", "fn() -> Array<T>", "Flattens nested arrays by one level.\n\n**Example:**\n```aethershell\n[[1, 2], [3, 4]] | flatten  // [1, 2, 3, 4]\n```"),
    ("reverse", "fn() -> Array<T>", "Reverses the order of elements.\n\n**Example:**\n```aethershell\n[1, 2, 3] | reverse  // [3, 2, 1]\n```"),
    ("contains", "fn(value: T) -> Bool", "Checks if string/array contains value.\n\n**Example:**\n```aethershell\n\"hello\" | contains(\"ell\")  // true\n[1, 2, 3] | contains(2)    // true\n```"),
];

/// Keyword documentation
const KEYWORD_DOCS: &[(&str, &str)] = &[
    ("let", "**Variable Binding**\n\nDeclares an immutable variable.\n\n```aethershell\nlet x = 42\nlet name = \"Alice\"\nlet numbers = [1, 2, 3]\n```"),
    ("mut", "**Mutable Binding**\n\nUsed with `let` to create a mutable variable.\n\n```aethershell\nlet mut counter = 0\ncounter = counter + 1\n```"),
    ("fn", "**Lambda Function**\n\nDefines an anonymous function (lambda).\n\n```aethershell\nlet double = fn(x) => x * 2\n[1, 2, 3] | map(fn(x) => x * x)\n```"),
    ("match", "**Pattern Matching**\n\nMatches a value against patterns.\n\n```aethershell\nmatch result {\n    Some(x) => print(x),\n    None => print(\"No value\"),\n    _ => print(\"Unknown\")\n}\n```"),
    ("if", "**Conditional Guard**\n\nUsed in match arms for conditional matching.\n\n```aethershell\nmatch x {\n    n if n > 0 => \"positive\",\n    n if n < 0 => \"negative\",\n    _ => \"zero\"\n}\n```"),
    ("true", "**Boolean Literal**\n\nThe boolean true value.\n\n```aethershell\nlet active = true\n```"),
    ("false", "**Boolean Literal**\n\nThe boolean false value.\n\n```aethershell\nlet inactive = false\n```"),
    ("null", "**Null Value**\n\nRepresents the absence of a value.\n\n```aethershell\nlet empty = null\n```"),
];

pub fn get_hover(store: &DocumentStore, uri: &Url, position: Position) -> Option<Hover> {
    let (word, range) = store.get_word_at_position(uri, position)?;

    // Check builtins
    for (name, signature, doc) in BUILTIN_DOCS.iter() {
        if *name == word {
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!("```aethershell\n{}\n```\n\n---\n\n{}", signature, doc),
                }),
                range: Some(range),
            });
        }
    }

    // Check keywords
    for (keyword, doc) in KEYWORD_DOCS.iter() {
        if *keyword == word {
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: doc.to_string(),
                }),
                range: Some(range),
            });
        }
    }

    // Check for user-defined variables
    if let Some(doc) = store.get(uri) {
        if let Some(ref ast) = doc.ast {
            for stmt in ast {
                if let aethershell::ast::Stmt::Let { name, is_mut, .. } = stmt {
                    if name == &word {
                        let mutability = if *is_mut { "mutable" } else { "immutable" };
                        return Some(Hover {
                            contents: HoverContents::Markup(MarkupContent {
                                kind: MarkupKind::Markdown,
                                value: format!("**Variable** `{}`\n\n*{}*", name, mutability),
                            }),
                            range: Some(range),
                        });
                    }
                }
            }
        }
    }

    None
}
