use std::collections::HashSet;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use gnomon_db::{Database, Diagnostic, RenderWithDb, SourceFile, check_syntax, evaluate, parse, validate_calendar};

// r[impl cli.root]
// r[impl cli.option.help]
// r[impl cli.option.help.short]
#[derive(Parser)]
#[command(
    name = "gnomon",
    about = "A plaintext calendaring language",
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

// r[impl cli.subcommand.help]
#[derive(Subcommand)]
enum Command {
    // r[impl cli.subcommand.parse]
    /// Parse a .gnomon file and print its syntax tree.
    Parse {
        /// Path to the file to parse.
        file: PathBuf,
    },
    // r[impl cli.subcommand.check+2]
    /// Check a .gnomon file as a calendar project root.
    Check {
        /// Path to the root .gnomon file.
        file: PathBuf,
    },
    // r[impl cli.subcommand.eval]
    /// Evaluate a .gnomon file or expression and print its lowered document.
    Eval {
        /// Path to the file to evaluate.
        #[arg(conflicts_with = "expr")]
        file: Option<PathBuf>,

        // r[impl cli.subcommand.eval.expr]
        // r[impl cli.subcommand.eval.expr.exclusive]
        /// Evaluate an inline expression.
        #[arg(long)]
        expr: Option<String>,
    },
}

fn main() -> ExitCode {
    // r[impl cli.option.version]
    // r[impl cli.option.version.short]
    // r[impl cli.option.version.behavior]
    let matches = Cli::command()
        .version(env!("CARGO_PKG_VERSION"))
        .disable_version_flag(true)
        .arg(
            clap::Arg::new("version")
                .short('v')
                .long("version")
                .action(clap::ArgAction::Version)
                .help("Print version"),
        )
        .get_matches();

    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());

    match cli.command {
        Command::Parse { file } => {
            // r[impl cli.subcommand.parse.no-file]
            let text = match std::fs::read_to_string(&file) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("error: could not read {}: {e}", file.display());
                    return ExitCode::FAILURE;
                }
            };

            // r[impl cli.subcommand.parse.output]
            let db = Database::default();
            let source = SourceFile::new(&db, file, text);
            let result = parse(&db, source);
            let syntax = result.syntax_node(&db);
            println!("{syntax:#?}");

            if result.has_errors(&db) {
                eprintln!("errors:");
                for diag in parse::accumulated::<Diagnostic>(&db, source) {
                    eprintln!(
                        "  {}..{}: {}",
                        u32::from(diag.range.start()),
                        u32::from(diag.range.end()),
                        diag.message
                    );
                }
                return ExitCode::FAILURE;
            }

            ExitCode::SUCCESS
        }
        Command::Eval { file, expr } => {
            let db = Database::default();

            let source = match (&file, &expr) {
                (Some(file), None) => {
                    // r[impl cli.subcommand.eval.no-file]
                    let text = match std::fs::read_to_string(file) {
                        Ok(s) => s,
                        Err(e) => {
                            eprintln!("error: could not read {}: {e}", file.display());
                            return ExitCode::FAILURE;
                        }
                    };
                    SourceFile::new(&db, file.clone(), text)
                }
                (None, Some(expr)) => {
                    SourceFile::new(&db, PathBuf::from("<expr>"), expr.clone())
                }
                _ => {
                    eprintln!("error: provide either a file path or --expr");
                    return ExitCode::FAILURE;
                }
            };

            let result = evaluate(&db, source);

            // Collect parse + validation diagnostics.
            let mut diagnostics: Vec<Diagnostic> =
                check_syntax::accumulated::<Diagnostic>(&db, source)
                    .into_iter()
                    .cloned()
                    .collect();
            // Add lowering diagnostics.
            diagnostics.extend(result.diagnostics);
            diagnostics.sort_by_key(|d| d.range.start());

            // r[impl cli.subcommand.eval.output]
            let has_errors = print_diagnostics(&db, &diagnostics);

            if has_errors {
                ExitCode::FAILURE
            } else {
                use std::io::Write;
                let out = std::io::stdout();
                let _ = writeln!(out.lock(), "{}", result.value.render(&db));
                ExitCode::SUCCESS
            }
        }
        // r[impl cli.subcommand.check+2]
        Command::Check { file } => {
            // r[impl cli.subcommand.check.no-file+2]
            let text = match std::fs::read_to_string(&file) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("error: could not read {}: {e}", file.display());
                    return ExitCode::FAILURE;
                }
            };

            let db = Database::default();
            let root_path = file.canonicalize().unwrap_or_else(|_| file.clone());
            let source = SourceFile::new(&db, root_path.clone(), text);

            // Evaluate the root file.
            let eval_result = evaluate(&db, source);
            let imported_files = eval_result.imported_files.clone();

            // Validate the evaluated value as a calendar.
            let check_result = validate_calendar(
                &db,
                source,
                eval_result.value,
                eval_result.diagnostics,
            );

            let mut diagnostics = check_result.diagnostics;

            // r[impl cli.subcommand.check.unused]
            // Detect unused .gnomon files in the project directory.
            if let Some(project_dir) = root_path.parent() {
                let mut known: HashSet<PathBuf> = HashSet::new();
                known.insert(root_path.clone());
                for p in &imported_files {
                    if let Ok(canon) = p.canonicalize() {
                        known.insert(canon);
                    } else {
                        known.insert(p.clone());
                    }
                }

                for found in find_gnomon_files(project_dir) {
                    let canon = found.canonicalize().unwrap_or_else(|_| found.clone());
                    if !known.contains(&canon) {
                        diagnostics.push(Diagnostic {
                            source,
                            range: gnomon_db::TextRange::default(),
                            severity: gnomon_db::Severity::Warning,
                            message: format!(
                                "file {} is not imported (directly or indirectly) by the root file",
                                found.display()
                            ),
                        });
                    }
                }
            }

            diagnostics.sort_by_key(|d| (d.source.path(&db).clone(), d.range.start()));

            // r[impl cli.subcommand.check.output+2]
            let has_errors = print_diagnostics(&db, &diagnostics);

            if has_errors {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
    }
}

/// Recursively find all .gnomon files under a directory.
fn find_gnomon_files(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() {
                result.extend(find_gnomon_files(&path));
            } else if path.extension().is_some_and(|ext| ext == "gnomon") {
                result.push(path);
            }
        }
    }
    result
}

/// Print diagnostics to stderr. Returns true if any errors were found.
fn print_diagnostics(db: &Database, diagnostics: &[Diagnostic]) -> bool {
    let mut has_errors = false;
    for diag in diagnostics {
        let text = diag.source.text(db);
        let offset = u32::from(diag.range.start()) as usize;
        let (line, col) = offset_to_line_col(text, offset);
        let severity = match diag.severity {
            gnomon_db::Severity::Error => {
                has_errors = true;
                "error"
            }
            gnomon_db::Severity::Warning => "warning",
        };
        eprintln!(
            "{}:{}:{}: {}: {}",
            diag.source.path(db).display(),
            line,
            col,
            severity,
            diag.message
        );
    }
    has_errors
}

fn offset_to_line_col(text: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, ch) in text.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

