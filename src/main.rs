use std::path::PathBuf;
use std::process::ExitCode;

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use gnomon_db::{Database, Diagnostic, RenderWithDb, SourceFile, check_syntax, evaluate, merge, parse};

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
    // r[impl cli.subcommand.check]
    /// Check a .gnomon file for errors.
    Check {
        /// Path to the file to check.
        file: PathBuf,
    },
    /// Evaluate a .gnomon file and print its lowered document.
    Eval {
        /// Path to the file to evaluate.
        file: PathBuf,
    },
    /// Merge .gnomon files into a single calendar.
    Merge {
        /// Paths to .gnomon files or directories. If empty, merges all
        /// .gnomon files in the current directory.
        paths: Vec<PathBuf>,
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
        Command::Eval { file } => {
            let text = match std::fs::read_to_string(&file) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("error: could not read {}: {e}", file.display());
                    return ExitCode::FAILURE;
                }
            };

            let db = Database::default();
            let source = SourceFile::new(&db, file.clone(), text);
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

            let has_errors = print_diagnostics(&db, &diagnostics);

            println!("{}", result.value.render(&db));

            if has_errors {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Command::Check { file } => {
            let text = match std::fs::read_to_string(&file) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("error: could not read {}: {e}", file.display());
                    return ExitCode::FAILURE;
                }
            };

            let db = Database::default();
            let source = SourceFile::new(&db, file.clone(), text);
            check_syntax(&db, source);
            let mut diagnostics: Vec<Diagnostic> = check_syntax::accumulated::<Diagnostic>(&db, source)
                .into_iter()
                .cloned()
                .collect();
            diagnostics.sort_by_key(|d| d.range.start());

            if diagnostics.is_empty() {
                ExitCode::SUCCESS
            } else {
                print_diagnostics(&db, &diagnostics);
                ExitCode::FAILURE
            }
        }
        Command::Merge { paths } => {
            let db = Database::default();

            // File discovery: expand directories to *.gnomon files.
            let paths = if paths.is_empty() {
                vec![std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))]
            } else {
                paths
            };

            let mut files = Vec::new();
            for path in &paths {
                if path.is_dir() {
                    let mut entries: Vec<_> = std::fs::read_dir(path)
                        .unwrap_or_else(|e| {
                            eprintln!("error: could not read directory {}: {e}", path.display());
                            std::process::exit(1);
                        })
                        .filter_map(|e| e.ok())
                        .map(|e| e.path())
                        .filter(|p| p.extension().is_some_and(|ext| ext == "gnomon"))
                        .collect();
                    entries.sort();
                    files.extend(entries);
                } else {
                    files.push(path.clone());
                }
            }

            if files.is_empty() {
                eprintln!("error: no .gnomon files found");
                return ExitCode::FAILURE;
            }

            let mut source_files = Vec::new();
            for file in &files {
                let text = match std::fs::read_to_string(file) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("error: could not read {}: {e}", file.display());
                        return ExitCode::FAILURE;
                    }
                };
                source_files.push(SourceFile::new(&db, file.clone(), text));
            }

            let result = merge(&db, &source_files);
            let mut diagnostics = result.diagnostics;
            diagnostics.sort_by_key(|d| d.range.start());

            let has_errors = print_diagnostics(&db, &diagnostics);

            println!("{}", result.calendar.render(&db));

            if has_errors {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
    }
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
