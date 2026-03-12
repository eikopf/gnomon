use std::io::{self, BufRead, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use etcetera::BaseStrategy;
use gnomon_db::{
    Database, Diagnostic, RenderWithDb, Severity, SourceFile,
    check_syntax, evaluate_repl_input,
};
use gnomon_db::eval::types::Value;

const PROMPT: &str = "gnomon> ";
const CONTINUATION: &str = "  ...> ";

// r[impl cli.subcommand.repl]
pub fn run_repl() -> ExitCode {
    let db = Database::default();
    run_repl_inner(&db)
}

fn run_repl_inner<'db>(db: &'db Database) -> ExitCode {
    let mut env: Vec<(String, Value<'db>)> = Vec::new();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    if io::stdin().is_terminal() {
        run_interactive(db, &mut env, &cwd)
    } else {
        run_piped(db, &mut env, &cwd)
    }
}

// r[impl cli.subcommand.repl.history]
fn run_interactive<'db>(
    db: &'db Database,
    env: &mut Vec<(String, Value<'db>)>,
    cwd: &PathBuf,
) -> ExitCode {
    use rustyline::error::ReadlineError;
    use rustyline::DefaultEditor;

    let mut editor = match DefaultEditor::new() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("error: failed to initialize line editor: {e}");
            return ExitCode::FAILURE;
        }
    };

    // Load history from XDG cache directory.
    let history_path = history_file_path();
    if let Some(ref path) = history_path {
        let _ = editor.load_history(path);
    }

    let mut input_buf = String::new();
    let mut continuation = false;

    loop {
        // r[impl cli.subcommand.repl.prompt]
        // r[impl cli.subcommand.repl.prompt.continuation]
        let prompt = if continuation { CONTINUATION } else { PROMPT };
        match editor.readline(prompt) {
            Ok(line) => {
                if !continuation {
                    input_buf.clear();
                }
                if !input_buf.is_empty() {
                    input_buf.push('\n');
                }
                input_buf.push_str(&line);

                // Meta-commands only on first line.
                if !continuation {
                    let trimmed = input_buf.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    if trimmed.starts_with(':') {
                        let _ = editor.add_history_entry(&input_buf);
                        handle_meta_command(trimmed, db, env);
                        input_buf.clear();
                        continue;
                    }
                }

                // r[impl cli.subcommand.repl.multiline]
                if !gnomon_parser::is_balanced(&input_buf) {
                    continuation = true;
                    continue;
                }

                continuation = false;
                let _ = editor.add_history_entry(&input_buf);
                eval_and_print(db, &input_buf, env, cwd);
                input_buf.clear();
            }
            // r[impl cli.subcommand.repl.meta.quit]
            Err(ReadlineError::Eof) => break,
            Err(ReadlineError::Interrupted) => {
                if continuation {
                    input_buf.clear();
                    continuation = false;
                    continue;
                }
                break;
            }
            Err(e) => {
                eprintln!("error: {e}");
                break;
            }
        }
    }

    if let Some(ref path) = history_path {
        let _ = std::fs::create_dir_all(path.parent().unwrap());
        let _ = editor.save_history(path);
    }

    ExitCode::SUCCESS
}

/// Non-interactive mode for piped stdin (used in tests).
fn run_piped<'db>(
    db: &'db Database,
    env: &mut Vec<(String, Value<'db>)>,
    cwd: &PathBuf,
) -> ExitCode {
    let stdin = io::stdin();
    let mut input_buf = String::new();
    let mut continuation = false;

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        if !continuation {
            input_buf.clear();
        }
        if !input_buf.is_empty() {
            input_buf.push('\n');
        }
        input_buf.push_str(&line);

        if !continuation {
            let trimmed = input_buf.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.starts_with(':') {
                handle_meta_command(trimmed, db, env);
                input_buf.clear();
                continue;
            }
        }

        if !gnomon_parser::is_balanced(&input_buf) {
            continuation = true;
            continue;
        }

        continuation = false;
        eval_and_print(db, &input_buf, env, cwd);
        input_buf.clear();
    }

    ExitCode::SUCCESS
}

// r[impl cli.subcommand.repl.eval]
fn eval_and_print<'db>(
    db: &'db Database,
    input: &str,
    env: &mut Vec<(String, Value<'db>)>,
    cwd: &PathBuf,
) {
    let source = SourceFile::new(db, cwd.join("<repl>"), input.to_string());
    let result = evaluate_repl_input(db, source, env);

    // Collect parse + validation diagnostics.
    let mut diagnostics: Vec<Diagnostic> = check_syntax::accumulated::<Diagnostic>(db, source)
        .into_iter()
        .cloned()
        .collect();
    diagnostics.extend(result.diagnostics);
    diagnostics.sort_by_key(|d| d.range.start());

    // r[impl cli.subcommand.repl.diagnostics]
    let has_errors = diagnostics.iter().any(|d| d.severity == Severity::Error);

    if has_errors {
        for diag in &diagnostics {
            eprintln!("error: {}", diag.message);
        }
        // Don't update env on error.
        return;
    }

    for diag in &diagnostics {
        if diag.severity == Severity::Warning {
            eprintln!("warning: {}", diag.message);
        }
    }

    // r[impl cli.subcommand.repl.let-persist]
    let has_new_bindings = !result.new_bindings.is_empty();
    env.extend(result.new_bindings);

    // Suppress output for bare let bindings that produce an empty list.
    let is_empty_list = matches!(&result.value, Value::List(v) if v.is_empty());
    if !(has_new_bindings && is_empty_list) {
        let out = io::stdout();
        let _ = writeln!(out.lock(), "{}", result.value.render(db));
    }
}

fn handle_meta_command<'db>(
    cmd: &str,
    db: &'db Database,
    env: &mut Vec<(String, Value<'db>)>,
) {
    let (command, arg) = match cmd.split_once(char::is_whitespace) {
        Some((c, a)) => (c, Some(a.trim())),
        None => (cmd, None),
    };

    match command {
        // r[impl cli.subcommand.repl.meta.quit]
        ":quit" | ":q" => std::process::exit(0),
        // r[impl cli.subcommand.repl.meta.reset]
        ":reset" => {
            env.clear();
            println!("Environment cleared.");
        }
        // r[impl cli.subcommand.repl.meta.type]
        ":type" => {
            if let Some(expr_str) = arg {
                let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                let source = SourceFile::new(db, cwd.join("<repl>"), expr_str.to_string());
                let result = evaluate_repl_input(db, source, env);

                let mut diagnostics: Vec<Diagnostic> =
                    check_syntax::accumulated::<Diagnostic>(db, source)
                        .into_iter()
                        .cloned()
                        .collect();
                diagnostics.extend(result.diagnostics);

                let has_errors = diagnostics.iter().any(|d| d.severity == Severity::Error);
                if has_errors {
                    for diag in &diagnostics {
                        eprintln!("error: {}", diag.message);
                    }
                } else {
                    println!("{}", result.value.type_name());
                }
            } else {
                eprintln!("usage: :type <expression>");
            }
        }
        // r[impl cli.subcommand.repl.meta.parse]
        ":parse" => {
            if let Some(expr_str) = arg {
                let parse = gnomon_parser::parse(expr_str);
                println!("{}", parse.debug_tree());
                if !parse.ok() {
                    for err in parse.errors() {
                        eprintln!("error: {}", err.message);
                    }
                }
            } else {
                eprintln!("usage: :parse <expression>");
            }
        }
        // r[impl cli.subcommand.repl.meta.help]
        ":help" => {
            println!("Available commands:");
            println!("  :help          Show this help message");
            println!("  :type <expr>   Show the type of an expression");
            println!("  :parse <expr>  Show the parse tree of an expression");
            println!("  :reset         Clear all let bindings");
            println!("  :quit, :q      Exit the REPL");
        }
        other => {
            eprintln!("unknown command: {other}");
            eprintln!("type :help for a list of commands");
        }
    }
}

fn history_file_path() -> Option<PathBuf> {
    etcetera::base_strategy::choose_base_strategy()
        .ok()
        .map(|strategy| strategy.cache_dir().join("gnomon").join("repl_history"))
}
