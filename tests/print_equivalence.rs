use aethershell::{env::Env, eval, parser, value::Value};

fn eval_code(code: &str) -> Value {
    let mut env = Env::default();
    let stmts = parser::parse_program(code).unwrap();
    eval::eval_program(&stmts, &mut env).unwrap()
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(ch) = it.next() {
        if ch == '\x1b' {
            // skip '[' and until 'm'
            if let Some('[') = it.peek() {
                it.next();
                while let Some(&c) = it.peek() {
                    it.next();
                    if c == 'm' {
                        break;
                    }
                }
                continue;
            }
        }
        out.push(ch);
    }
    out
}

#[test]
fn print_equivalence() {
    let a = eval_code(r#"print("hi")"#);
    let b = eval_code(r#"print "hi""#);
    let c = eval_code(r#""hi" | print"#);

    let pretty_a = aethershell::value::pretty::display_inline(
        &a,
        &aethershell::value::pretty::Theme::default(),
    );
    let pretty_b = aethershell::value::pretty::display_inline(
        &b,
        &aethershell::value::pretty::Theme::default(),
    );
    let pretty_c = aethershell::value::pretty::display_inline(
        &c,
        &aethershell::value::pretty::Theme::default(),
    );

    assert_eq!(strip_ansi(&pretty_a), strip_ansi(&pretty_b));
    assert_eq!(strip_ansi(&pretty_b), strip_ansi(&pretty_c));

    assert_eq!(format!("{:?}", a), format!("{:?}", b));
    assert_eq!(format!("{:?}", b), format!("{:?}", c));
}
