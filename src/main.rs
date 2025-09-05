use anyhow::{Context, Result};
use std::env;
use std::fs;
use std::io::{self, Read};

use aurora_shell::{env::Env, eval, parser, transpile};

fn usage() -> &'static str {
    r#"Aurora Shell
Usage:
  ae                     # start REPL
  ae FILE.ae             # run Aurora file
  ae --bash FILE.sh      # run Bash file in compatibility mode
  ae -b                  # read Bash from stdin and run (compat mode)
"#
}

fn main() -> Result<()> {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print!("{}", usage());
        return Ok(());
    }

    // flags
    let bash_mode = take_flag(&mut args, "--bash") || take_flag(&mut args, "-b");

    if bash_mode && args.is_empty() {
        // Read bash from stdin, transpile, then run
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf)?;
        let code = transpile::bash::transpile_bash_to_ae(&buf)?;
        return run_code(&code);
    }

    match args.as_slice() {
        // No file args → REPL
        [] => repl()?,
        // One file → run it (with optional --bash)
        [file] => run_file(file, bash_mode)?,
        _ => {
            eprintln!("{}", usage());
            std::process::exit(2);
        }
    }
    Ok(())
}

fn repl() -> Result<()> {
    // Simple REPL; keep it lean and rely on your existing `repl.rs` if you have one.
    // Here we do a tiny inline REPL to avoid extra wires.
    use std::io::Write;
    let mut env = Env::default();
    let mut line = String::new();

    println!("Aurora REPL — type Ctrl-D to exit");
    loop {
        line.clear();
        print!("au> ");
        io::stdout().flush().ok();
        if io::stdin().read_line(&mut line)? == 0 {
            println!();
            break;
        }
        let src = line.trim();
        if src.is_empty() {
            continue;
        }

        match parser::parse_program(src) {
            Ok(stmts) => match eval::eval_program(&stmts, &mut env) {
                Ok(val) => println!("{:?}", val),
                Err(e) => eprintln!("eval error: {e}"),
            },
            Err(e) => eprintln!("parse error: {e}"),
        }
    }
    Ok(())
}

fn run_file(path: &str, bash_mode: bool) -> Result<()> {
    let mut code = fs::read_to_string(path).with_context(|| format!("failed to read {}", path))?;

    if bash_mode {
        code = transpile::bash::transpile_bash_to_ae(&code)
            .with_context(|| format!("bash→aurora transpile failed for {}", path))?;
    }

    run_code(&code)
}

fn run_code(code: &str) -> Result<()> {
    let stmts = parser::parse_program(code)?;
    let mut env = Env::default();
    let val = eval::eval_program(&stmts, &mut env)?;
    // Print the last value (shell style you might suppress; we show for now)
    println!("{:?}", val);
    Ok(())
}

/// Pull a flag out of args, returning true if it existed.
fn take_flag(args: &mut Vec<String>, name: &str) -> bool {
    if let Some(i) = args.iter().position(|a| a == name) {
        args.remove(i);
        true
    } else {
        false
    }
}
