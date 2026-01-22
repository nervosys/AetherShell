//! Tests for import statement parsing

use aether_shell::parser::parse_program;
use aether_shell::ast::Stmt;

#[test]
fn test_import_simple_path() {
    let src = r#"import "path/to/module.ae""#;
    let stmts = parse_program(src).expect("should parse");
    assert_eq!(stmts.len(), 1);
    
    match &stmts[0] {
        Stmt::Import { items, source, alias } => {
            assert!(items.is_empty(), "no specific items for simple import");
            assert_eq!(source, "path/to/module.ae");
            assert!(alias.is_none());
        }
        _ => panic!("expected import statement"),
    }
}

#[test]
fn test_import_with_alias() {
    let src = r#"import "utils.ae" as utils"#;
    let stmts = parse_program(src).expect("should parse");
    assert_eq!(stmts.len(), 1);
    
    match &stmts[0] {
        Stmt::Import { items, source, alias } => {
            assert!(items.is_empty());
            assert_eq!(source, "utils.ae");
            assert_eq!(alias.as_deref(), Some("utils"));
        }
        _ => panic!("expected import statement"),
    }
}

#[test]
fn test_import_selective() {
    let src = r#"import { add, subtract } from "math.ae""#;
    let stmts = parse_program(src).expect("should parse");
    assert_eq!(stmts.len(), 1);
    
    match &stmts[0] {
        Stmt::Import { items, source, alias } => {
            assert_eq!(items.len(), 2);
            assert_eq!(items[0].name, "add");
            assert!(items[0].alias.is_none());
            assert_eq!(items[1].name, "subtract");
            assert!(items[1].alias.is_none());
            assert_eq!(source, "math.ae");
            assert!(alias.is_none());
        }
        _ => panic!("expected import statement"),
    }
}

#[test]
fn test_import_selective_with_aliases() {
    let src = r#"import { add as plus, subtract as minus } from "math.ae""#;
    let stmts = parse_program(src).expect("should parse");
    assert_eq!(stmts.len(), 1);
    
    match &stmts[0] {
        Stmt::Import { items, source, alias } => {
            assert_eq!(items.len(), 2);
            assert_eq!(items[0].name, "add");
            assert_eq!(items[0].alias.as_deref(), Some("plus"));
            assert_eq!(items[1].name, "subtract");
            assert_eq!(items[1].alias.as_deref(), Some("minus"));
            assert_eq!(source, "math.ae");
            assert!(alias.is_none());
        }
        _ => panic!("expected import statement"),
    }
}

#[test]
fn test_import_package_reference() {
    let src = r#"import "pkg:math-utils@1.0""#;
    let stmts = parse_program(src).expect("should parse");
    assert_eq!(stmts.len(), 1);
    
    match &stmts[0] {
        Stmt::Import { items, source, alias } => {
            assert!(items.is_empty());
            assert_eq!(source, "pkg:math-utils@1.0");
            assert!(alias.is_none());
        }
        _ => panic!("expected import statement"),
    }
}

#[test]
fn test_import_selective_from_package() {
    let src = r#"import { sin, cos } from "pkg:math-utils""#;
    let stmts = parse_program(src).expect("should parse");
    assert_eq!(stmts.len(), 1);
    
    match &stmts[0] {
        Stmt::Import { items, source, alias } => {
            assert_eq!(items.len(), 2);
            assert_eq!(items[0].name, "sin");
            assert_eq!(items[1].name, "cos");
            assert_eq!(source, "pkg:math-utils");
            assert!(alias.is_none());
        }
        _ => panic!("expected import statement"),
    }
}

#[test]
fn test_import_in_program() {
    let src = r#"
        import "utils.ae" as u
        let x = 5
        print(u.helper(x))
    "#;
    let stmts = parse_program(src).expect("should parse");
    assert_eq!(stmts.len(), 3);
    
    // First statement is import
    assert!(matches!(&stmts[0], Stmt::Import { .. }));
    // Second is let
    assert!(matches!(&stmts[1], Stmt::Let { .. }));
    // Third is expression
    assert!(matches!(&stmts[2], Stmt::Expr(_)));
}
