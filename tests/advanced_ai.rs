//! Tests for advanced AI builtins (RAG, Knowledge Graphs, Semantic Caching)

use aether_shell::builtins::call;
use aether_shell::env::Env;
use aether_shell::value::Value;
use std::collections::BTreeMap;

// ===================== RAG Tests =====================

#[test]
fn test_rag_index_basic() {
    let mut env = Env::new();
    let result = call(
        "rag_index",
        vec![Value::Str(
            "This is a test document about machine learning and AI.".to_string(),
        )],
        &mut env,
    )
    .unwrap();

    if let Value::Record(rec) = result {
        assert!(rec.contains_key("doc_id"));
        assert!(rec.contains_key("total_docs"));
    } else {
        panic!("Expected Record, got {:?}", result);
    }
}

#[test]
fn test_rag_index_with_source() {
    let mut env = Env::new();
    let result = call(
        "rag_index",
        vec![
            Value::Str("Document content about neural networks.".to_string()),
            Value::Str("neural_networks.txt".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    if let Value::Record(rec) = result {
        assert_eq!(
            rec.get("source"),
            Some(&Value::Str("neural_networks.txt".to_string()))
        );
    } else {
        panic!("Expected Record, got {:?}", result);
    }
}

#[test]
fn test_index_docs_alias() {
    let mut env = Env::new();
    let result = call(
        "index_docs",
        vec![Value::Str("Alias test content.".to_string())],
        &mut env,
    )
    .unwrap();

    if let Value::Record(rec) = result {
        assert!(rec.contains_key("doc_id"));
    } else {
        panic!("Expected Record");
    }
}

#[test]
fn test_rag_search() {
    let mut env = Env::new();

    // Clear any existing state from other tests
    call("rag_clear", vec![], &mut env).unwrap();

    // Index some documents first
    call(
        "rag_index",
        vec![Value::Str(
            "Deep learning is a subset of machine learning.".to_string(),
        )],
        &mut env,
    )
    .unwrap();

    call(
        "rag_index",
        vec![Value::Str(
            "Neural networks process information like the human brain.".to_string(),
        )],
        &mut env,
    )
    .unwrap();

    // Search
    let result = call(
        "rag_search",
        vec![Value::Str("machine learning".to_string()), Value::Int(2)],
        &mut env,
    )
    .unwrap();

    if let Value::Array(results) = result {
        // Should have at least 1 result
        assert!(
            !results.is_empty(),
            "Expected non-empty results from rag_search"
        );

        // Each result should have score and content
        if let Some(Value::Record(first)) = results.first() {
            assert!(first.contains_key("score"));
            assert!(first.contains_key("content"));
        }
    } else {
        panic!("Expected Array, got {:?}", result);
    }
}

#[test]
fn test_search_docs_alias() {
    let mut env = Env::new();

    call(
        "rag_index",
        vec![Value::Str("Test document for search alias.".to_string())],
        &mut env,
    )
    .unwrap();

    let result = call(
        "search_docs",
        vec![Value::Str("test".to_string())],
        &mut env,
    )
    .unwrap();

    if let Value::Array(_) = result {
        // Success
    } else {
        panic!("Expected Array");
    }
}

#[test]
fn test_rag_query() {
    let mut env = Env::new();

    // Index documents
    call(
        "rag_index",
        vec![
            Value::Str("Python is a programming language.".to_string()),
            Value::Str("python.txt".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    call(
        "rag_index",
        vec![
            Value::Str("Rust is a systems programming language.".to_string()),
            Value::Str("rust.txt".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    let result = call(
        "rag_query",
        vec![Value::Str("programming language".to_string())],
        &mut env,
    )
    .unwrap();

    if let Value::Record(rec) = result {
        assert!(rec.contains_key("query"));
        assert!(rec.contains_key("context"));
        assert!(rec.contains_key("sources"));
        assert!(rec.contains_key("doc_count"));
    } else {
        panic!("Expected Record, got {:?}", result);
    }
}

#[test]
fn test_rag_alias() {
    let mut env = Env::new();

    call(
        "rag_index",
        vec![Value::Str("RAG alias test content.".to_string())],
        &mut env,
    )
    .unwrap();

    let result = call("rag", vec![Value::Str("alias".to_string())], &mut env).unwrap();

    if let Value::Record(rec) = result {
        assert!(rec.contains_key("context"));
    } else {
        panic!("Expected Record");
    }
}

#[test]
fn test_rag_clear() {
    let mut env = Env::new();

    // Index some documents
    call(
        "rag_index",
        vec![Value::Str("Document 1".to_string())],
        &mut env,
    )
    .unwrap();

    call(
        "rag_index",
        vec![Value::Str("Document 2".to_string())],
        &mut env,
    )
    .unwrap();

    // Clear
    let result = call("rag_clear", vec![], &mut env).unwrap();

    // Should return count of cleared documents
    if let Value::Int(count) = result {
        assert!(count >= 2);
    } else {
        panic!("Expected Int, got {:?}", result);
    }
}

// ===================== Knowledge Graph Tests =====================

#[test]
fn test_kg_add_entity() {
    let mut env = Env::new();
    let result = call(
        "kg_add",
        vec![
            Value::Str("Person".to_string()),
            Value::Str("Alice".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    if let Value::Record(rec) = result {
        assert!(rec.contains_key("entity_id"));
        assert_eq!(rec.get("type"), Some(&Value::Str("Person".to_string())));
        assert_eq!(rec.get("name"), Some(&Value::Str("Alice".to_string())));
    } else {
        panic!("Expected Record, got {:?}", result);
    }
}

#[test]
fn test_add_entity_alias() {
    let mut env = Env::new();
    let result = call(
        "add_entity",
        vec![
            Value::Str("Company".to_string()),
            Value::Str("Acme Corp".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    if let Value::Record(rec) = result {
        assert!(rec.contains_key("entity_id"));
    } else {
        panic!("Expected Record");
    }
}

#[test]
fn test_kg_relate() {
    let mut env = Env::new();

    // Add entities first
    let alice = call(
        "kg_add",
        vec![
            Value::Str("Person".to_string()),
            Value::Str("Alice".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    let bob = call(
        "kg_add",
        vec![
            Value::Str("Person".to_string()),
            Value::Str("Bob".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    let alice_id = if let Value::Record(rec) = alice {
        rec.get("entity_id").cloned().unwrap()
    } else {
        panic!("Expected Record");
    };

    let bob_id = if let Value::Record(rec) = bob {
        rec.get("entity_id").cloned().unwrap()
    } else {
        panic!("Expected Record");
    };

    // Add relation
    let result = call(
        "kg_relate",
        vec![alice_id, bob_id, Value::Str("knows".to_string())],
        &mut env,
    )
    .unwrap();

    if let Value::Record(rec) = result {
        assert!(rec.contains_key("relation_id"));
        assert_eq!(
            rec.get("relation_type"),
            Some(&Value::Str("knows".to_string()))
        );
    } else {
        panic!("Expected Record, got {:?}", result);
    }
}

#[test]
fn test_add_relation_alias() {
    let mut env = Env::new();

    let e1 = call(
        "kg_add",
        vec![
            Value::Str("City".to_string()),
            Value::Str("New York".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    let e2 = call(
        "kg_add",
        vec![
            Value::Str("Country".to_string()),
            Value::Str("USA".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    let e1_id = if let Value::Record(rec) = e1 {
        rec.get("entity_id").cloned().unwrap()
    } else {
        panic!("Expected Record");
    };

    let e2_id = if let Value::Record(rec) = e2 {
        rec.get("entity_id").cloned().unwrap()
    } else {
        panic!("Expected Record");
    };

    let result = call(
        "add_relation",
        vec![e1_id, e2_id, Value::Str("located_in".to_string())],
        &mut env,
    )
    .unwrap();

    if let Value::Record(rec) = result {
        assert!(rec.contains_key("relation_id"));
    } else {
        panic!("Expected Record");
    }
}

#[test]
fn test_kg_relate_invalid_entity() {
    let mut env = Env::new();

    let result = call(
        "kg_relate",
        vec![
            Value::Str("nonexistent_1".to_string()),
            Value::Str("nonexistent_2".to_string()),
            Value::Str("test".to_string()),
        ],
        &mut env,
    );

    assert!(result.is_err());
}

#[test]
fn test_kg_query() {
    let mut env = Env::new();

    // Add some entities
    call(
        "kg_add",
        vec![
            Value::Str("Language".to_string()),
            Value::Str("Rust".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    call(
        "kg_add",
        vec![
            Value::Str("Language".to_string()),
            Value::Str("Python".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    let result = call("kg_query", vec![Value::Str("Rust".to_string())], &mut env).unwrap();

    if let Value::Array(results) = result {
        assert!(!results.is_empty());
    } else {
        panic!("Expected Array, got {:?}", result);
    }
}

#[test]
fn test_query_kg_alias() {
    let mut env = Env::new();

    call(
        "kg_add",
        vec![
            Value::Str("Test".to_string()),
            Value::Str("QueryAlias".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    let result = call(
        "query_kg",
        vec![Value::Str("QueryAlias".to_string())],
        &mut env,
    )
    .unwrap();

    if let Value::Array(_) = result {
        // Success
    } else {
        panic!("Expected Array");
    }
}

#[test]
fn test_kg_entities_list() {
    let mut env = Env::new();

    call(
        "kg_add",
        vec![
            Value::Str("TestType".to_string()),
            Value::Str("TestEntity".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    let result = call("kg_entities", vec![], &mut env).unwrap();

    if let Value::Array(entities) = result {
        assert!(!entities.is_empty());
    } else {
        panic!("Expected Array, got {:?}", result);
    }
}

#[test]
fn test_entities_alias() {
    let mut env = Env::new();
    let result = call("entities", vec![], &mut env).unwrap();

    if let Value::Array(_) = result {
        // Success
    } else {
        panic!("Expected Array");
    }
}

#[test]
fn test_kg_relations_list() {
    let mut env = Env::new();

    // Create entities and relation
    let e1 = call(
        "kg_add",
        vec![
            Value::Str("A".to_string()),
            Value::Str("Entity1".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    let e2 = call(
        "kg_add",
        vec![
            Value::Str("B".to_string()),
            Value::Str("Entity2".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    let e1_id = if let Value::Record(rec) = e1 {
        rec.get("entity_id").cloned().unwrap()
    } else {
        panic!("Expected Record");
    };

    let e2_id = if let Value::Record(rec) = e2 {
        rec.get("entity_id").cloned().unwrap()
    } else {
        panic!("Expected Record");
    };

    call(
        "kg_relate",
        vec![e1_id, e2_id, Value::Str("test_rel".to_string())],
        &mut env,
    )
    .unwrap();

    let result = call("kg_relations", vec![], &mut env).unwrap();

    if let Value::Array(relations) = result {
        assert!(!relations.is_empty());
    } else {
        panic!("Expected Array, got {:?}", result);
    }
}

#[test]
fn test_relations_alias() {
    let mut env = Env::new();
    let result = call("relations", vec![], &mut env).unwrap();

    if let Value::Array(_) = result {
        // Success
    } else {
        panic!("Expected Array");
    }
}

// ===================== Semantic Cache Tests =====================

#[test]
fn test_semantic_cache_put() {
    let mut env = Env::new();
    let result = call(
        "semantic_cache",
        vec![
            Value::Str("What is the capital of France?".to_string()),
            Value::Str("The capital of France is Paris.".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    if let Value::Record(rec) = result {
        assert_eq!(rec.get("cached"), Some(&Value::Bool(true)));
        assert!(rec.contains_key("total_entries"));
    } else {
        panic!("Expected Record, got {:?}", result);
    }
}

#[test]
fn test_cache_ai_alias() {
    let mut env = Env::new();
    let result = call(
        "cache_ai",
        vec![
            Value::Str("Test query".to_string()),
            Value::Str("Test response".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    if let Value::Record(rec) = result {
        assert_eq!(rec.get("cached"), Some(&Value::Bool(true)));
    } else {
        panic!("Expected Record");
    }
}

#[test]
fn test_semantic_cache_get_hit() {
    let mut env = Env::new();

    // First cache a response
    call(
        "semantic_cache",
        vec![
            Value::Str("What is machine learning?".to_string()),
            Value::Str("Machine learning is a subset of AI.".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    // Try to get with similar query
    let result = call(
        "semantic_cache_get",
        vec![Value::Str("What is machine learning?".to_string())],
        &mut env,
    )
    .unwrap();

    if let Value::Record(rec) = result {
        assert_eq!(rec.get("hit"), Some(&Value::Bool(true)));
        assert!(rec.contains_key("response"));
    } else {
        panic!("Expected Record, got {:?}", result);
    }
}

#[test]
fn test_semantic_cache_get_miss() {
    let mut env = Env::new();

    // Query without caching first
    let result = call(
        "semantic_cache_get",
        vec![Value::Str(
            "Completely unrelated query for testing".to_string(),
        )],
        &mut env,
    )
    .unwrap();

    if let Value::Record(rec) = result {
        assert_eq!(rec.get("hit"), Some(&Value::Bool(false)));
    } else {
        panic!("Expected Record, got {:?}", result);
    }
}

#[test]
fn test_cache_get_alias() {
    let mut env = Env::new();
    let result = call(
        "cache_get",
        vec![Value::Str("test query alias".to_string())],
        &mut env,
    )
    .unwrap();

    if let Value::Record(rec) = result {
        assert!(rec.contains_key("hit"));
    } else {
        panic!("Expected Record");
    }
}

#[test]
fn test_semantic_cache_clear() {
    let mut env = Env::new();

    // Clear first to start fresh
    call("semantic_cache_clear", vec![], &mut env).unwrap();

    // Cache some entries
    call(
        "semantic_cache",
        vec![
            Value::Str("Query 1 for clear test".to_string()),
            Value::Str("Response 1".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    call(
        "semantic_cache",
        vec![
            Value::Str("Query 2 for clear test".to_string()),
            Value::Str("Response 2".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    // Clear cache
    let result = call("semantic_cache_clear", vec![], &mut env).unwrap();

    if let Value::Int(count) = result {
        assert!(
            count >= 2,
            "Expected at least 2 cleared entries, got {}",
            count
        );
    } else {
        panic!("Expected Int, got {:?}", result);
    }
}

#[test]
fn test_cache_clear_ai_alias() {
    let mut env = Env::new();

    call(
        "semantic_cache",
        vec![
            Value::Str("Alias clear test".to_string()),
            Value::Str("Response".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    let result = call("cache_clear_ai", vec![], &mut env).unwrap();

    if let Value::Int(_) = result {
        // Success
    } else {
        panic!("Expected Int");
    }
}

#[test]
fn test_semantic_similarity_caching() {
    let mut env = Env::new();

    // Cache a response
    call(
        "semantic_cache",
        vec![
            Value::Str("How do I learn programming?".to_string()),
            Value::Str("Start with basic concepts and practice regularly.".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    // Try a semantically similar query
    let result = call(
        "semantic_cache_get",
        vec![Value::Str("How do I learn programming?".to_string())],
        &mut env,
    )
    .unwrap();

    if let Value::Record(rec) = result {
        // Should be a hit for exact match
        assert_eq!(rec.get("hit"), Some(&Value::Bool(true)));
    } else {
        panic!("Expected Record, got {:?}", result);
    }
}

#[test]
fn test_rag_workflow() {
    let mut env = Env::new();

    // Clear any existing state first
    call("rag_clear", vec![], &mut env).unwrap();

    // Full RAG workflow test
    // 1. Index documents
    call(
        "rag_index",
        vec![
            Value::Str("AetherShell is a next-generation shell written in Rust.".to_string()),
            Value::Str("intro.txt".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    call(
        "rag_index",
        vec![
            Value::Str("The shell supports typed pipelines and AI integration.".to_string()),
            Value::Str("features.txt".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    // 2. Query
    let query_result = call(
        "rag_query",
        vec![Value::Str("What is AetherShell?".to_string())],
        &mut env,
    )
    .unwrap();

    if let Value::Record(rec) = query_result {
        // Should have context and sources
        assert!(rec.contains_key("context"));
        if let Some(Value::Array(sources)) = rec.get("sources") {
            assert!(
                !sources.is_empty(),
                "Expected non-empty sources in RAG workflow"
            );
        }
    } else {
        panic!("Expected Record");
    }
}

#[test]
fn test_knowledge_graph_workflow() {
    let mut env = Env::new();

    // Full knowledge graph workflow
    // 1. Add entities
    let rust = call(
        "kg_add",
        vec![
            Value::Str("Language".to_string()),
            Value::Str("Rust".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    let aether = call(
        "kg_add",
        vec![
            Value::Str("Project".to_string()),
            Value::Str("AetherShell".to_string()),
        ],
        &mut env,
    )
    .unwrap();

    let rust_id = if let Value::Record(rec) = rust {
        rec.get("entity_id").cloned().unwrap()
    } else {
        panic!("Expected Record");
    };

    let aether_id = if let Value::Record(rec) = aether {
        rec.get("entity_id").cloned().unwrap()
    } else {
        panic!("Expected Record");
    };

    // 2. Add relation
    call(
        "kg_relate",
        vec![aether_id, rust_id, Value::Str("written_in".to_string())],
        &mut env,
    )
    .unwrap();

    // 3. Query
    let query_result = call("kg_query", vec![Value::Str("Rust".to_string())], &mut env).unwrap();

    if let Value::Array(results) = query_result {
        assert!(!results.is_empty());
    } else {
        panic!("Expected Array");
    }
}
