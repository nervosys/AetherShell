use aether_shell::{env::Env, eval::eval_program, parser::parse_program, value::Value};

fn eval_src(src: &str) -> Value {
    let stmts = parse_program(src).expect("parse");
    eval_program(&stmts, &mut Env::new()).expect("eval")
}

#[test]
fn sanity_array_literal() {
    let v = eval_src("[1,2,3]");
    match v {
        Value::Array(a) => assert_eq!(a.len(), 3),
        other => panic!("expected Array, got {:?}", other),
    }
}

#[test]
fn sanity_map_reduce() {
    let v = eval_src("[1,2,3,4] | map(fn(x) => x*x) | reduce(fn(a,b) => a+b, 0)");
    match v {
        Value::Int(n) => assert_eq!(n, 30),
        other => panic!("expected Int(30), got {:?}", other),
    }
}

#[test]
fn sanity_where_take() {
    let v = eval_src("[5,4,3,2,1] | where(fn(x) => x>2) | take(2)");
    match v {
        Value::Array(a) => assert_eq!(a.len(), 2),
        other => panic!("expected Array, got {:?}", other),
    }
}

#[test]
fn sanity_map_lambda_shorthand() {
    let v = eval_src("[1,2,3] | fn(x)=> x*2");
    match v {
        Value::Array(a) => {
            let nums: Vec<i64> = a
                .into_iter()
                .map(|v| {
                    if let Value::Int(n) = v {
                        n
                    } else {
                        panic!("not int")
                    }
                })
                .collect();
            assert_eq!(nums, vec![2, 4, 6]);
        }
        other => panic!("expected Array, got {:?}", other),
    }
}

#[test]
fn sanity_record_literal() {
    let v = eval_src("{name:\"a\", size:42}");
    match v {
        Value::Record(m) => {
            assert!(matches!(m.get("name"), Some(Value::Str(_))));
            assert!(matches!(m.get("size"), Some(Value::Int(42))));
        }
        other => panic!("expected Record, got {:?}", other),
    }
}

#[test]
fn sanity_print_variants() {
    // print("hi") and print "hi" and "hi" | print should all return Str
    let v1 = eval_src("print(\"hi\")");
    let v2 = eval_src("print \"hi\"");
    let v3 = eval_src("\"hi\" | print");
    match (v1, v2, v3) {
        (Value::Str(_), Value::Str(_), Value::Str(_)) => {}
        other => panic!("expected 3 Strs, got {:?}", other),
    }
}

#[test]
fn sanity_lambda_literal() {
    let v = eval_src("fn(x)=> x*x");
    match v {
        Value::Lambda(_) => {}
        other => panic!("expected Lambda, got {:?}", other),
    }
}
