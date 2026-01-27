// Tests for async/await syntax and semantics

use aethershell::env::Env;
use aethershell::eval::eval_program;
use aethershell::parser::parse_program;

/// Helper function to run a program and return the result
fn run(src: &str) -> anyhow::Result<aethershell::value::Value> {
    let stmts = parse_program(src)?;
    let mut env = Env::new();
    eval_program(&stmts, &mut env)
}

#[test]
fn async_lambda_parses() {
    // Basic async lambda should parse
    let result = parse_program("let f = async fn(x) => x * 2");
    assert!(result.is_ok(), "Failed to parse async lambda: {:?}", result);
}

#[test]
fn await_expression_parses() {
    // await should parse as unary expression
    let result = parse_program("let x = await future_val");
    assert!(result.is_ok(), "Failed to parse await: {:?}", result);
}

#[test]
fn async_lambda_returns_async_lambda_value() {
    // Async lambda should evaluate to an AsyncLambda value
    let result = run("async fn(x) => x * 2");
    assert!(result.is_ok());
    let val = result.unwrap();
    assert_eq!(val.type_name(), "AsyncLambda");
}

#[test]
fn calling_async_lambda_returns_future() {
    // Calling an async lambda should return a Future
    let result = run(r#"
        let f = async fn(x) => x * 2
        f(5)
    "#);
    assert!(result.is_ok(), "Failed to call async lambda: {:?}", result);
    let val = result.unwrap();
    assert_eq!(val.type_name(), "Future");
}

#[test]
fn await_executes_future() {
    // await should execute the future and return the result
    let result = run(r#"
        let f = async fn(x) => x * 2
        await f(5)
    "#);
    assert!(result.is_ok(), "Failed to await: {:?}", result);
    let val = result.unwrap();
    // The result should be 10 (5 * 2)
    assert_eq!(val, aethershell::value::Value::Int(10));
}

#[test]
fn await_on_non_future_returns_value() {
    // await on a non-future should just return the value (auto-await semantics)
    let result = run("await 42");
    assert!(result.is_ok());
    let val = result.unwrap();
    assert_eq!(val, aethershell::value::Value::Int(42));
}

#[test]
fn async_lambda_multi_arg() {
    // Async lambda with multiple arguments
    let result = run(r#"
        let add = async fn(a, b) => a + b
        await add(3, 4)
    "#);
    assert!(result.is_ok(), "Failed: {:?}", result);
    let val = result.unwrap();
    assert_eq!(val, aethershell::value::Value::Int(7));
}

#[test]
fn nested_await() {
    // Nested await expressions
    let result = run(r#"
        let double = async fn(x) => x * 2
        let triple = async fn(x) => x * 3
        await double(await triple(2))
    "#);
    assert!(result.is_ok(), "Failed: {:?}", result);
    let val = result.unwrap();
    // triple(2) = 6, double(6) = 12
    assert_eq!(val, aethershell::value::Value::Int(12));
}

#[test]
fn async_in_pipeline_context() {
    // async fn in word-call style
    let result = parse_program("map async fn(x) => x * 2");
    assert!(
        result.is_ok(),
        "Failed to parse async in pipeline: {:?}",
        result
    );
}

#[test]
fn await_unary_precedence() {
    // await should have correct unary precedence
    let result = run("await 1 + 2"); // should be (await 1) + 2 = 3
    assert!(result.is_ok());
    let val = result.unwrap();
    assert_eq!(val, aethershell::value::Value::Int(3));
}

#[test]
fn async_lambda_with_string() {
    let result = run(r#"
        let greet = async fn(name) => "Hello, " + name
        await greet("World")
    "#);
    assert!(result.is_ok(), "Failed: {:?}", result);
    let val = result.unwrap();
    assert_eq!(
        val,
        aethershell::value::Value::Str("Hello, World".to_string())
    );
}

#[test]
fn type_of_async_lambda() {
    // type_of should return "AsyncLambda" for async lambdas
    let result = run(r#"
        let f = async fn(x) => x
        type_of(f)
    "#);
    assert!(result.is_ok(), "Failed: {:?}", result);
    let val = result.unwrap();
    assert_eq!(
        val,
        aethershell::value::Value::Str("AsyncLambda".to_string())
    );
}

#[test]
fn type_of_future() {
    // type_of should return "Future" for futures
    let result = run(r#"
        let f = async fn(x) => x
        let future = f(42)
        type_of(future)
    "#);
    assert!(result.is_ok(), "Failed: {:?}", result);
    let val = result.unwrap();
    assert_eq!(val, aethershell::value::Value::Str("Future".to_string()));
}
