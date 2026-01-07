use aether_shell::env::Env;
use aether_shell::eval::eval_program;
use aether_shell::parser::parse_program;
use aether_shell::value::Value;

#[test]
fn test_map_reduce_print_pipelines() {
    // Test map and reduce with proper syntax
    let src1 = "[1,2,3,4] | map(fn(x) => x*2) | reduce(fn(a,b) => a+b, 0)";
    let stmts = parse_program(src1).expect("parse");
    let mut env = Env::new();
    let v = eval_program(&stmts, &mut env).expect("eval");
    match v {
        Value::Int(20) => {}
        other => panic!("expected Int(20), got {:?}", other),
    }

    // Test map piped to print
    let src2 = "[1,2,3,4] | map(fn(x) => x*2) | print";
    let stmts2 = parse_program(src2).expect("parse2");
    let mut env2 = Env::new();
    let v2 = eval_program(&stmts2, &mut env2).expect("eval2");
    // print returns a string representation
    match v2 {
        Value::Str(_) => {}
        other => panic!("expected Str from print, got {:?}", other),
    }
}
