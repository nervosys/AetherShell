use aether_shell::parser::parse_program;

#[test]
fn debug_parse_pipeline() {
    let src = r#"[1,2,3,4] | map fn(x)=> x*x | reduce fn(a,b)=> a+b 0"#;
    let p = parse_program(src).expect("parse");
    println!("AST: {:#?}", p);
}
