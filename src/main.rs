mod repl;

use std::collections::HashSet;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use gnomon_db::eval::EvalOptions;
use gnomon_db::{
    Database, Diagnostic, RenderWithDb, SourceFile, calendar_to_import_values, check_syntax,
    evaluate_with_options, parse, validate_calendar,
};
use gnomon_export::{emit_icalendar, emit_jscalendar};

// r[impl cli.root]
// r[impl cli.syntax]
// r[impl cli.option.help]
// r[impl cli.option.help.short]
// r[impl cli.option.help.behavior.root]
// r[impl cli.option.help.behavior.subcommand]
// r[impl cli.option.help.xor]
// r[impl cli.option.order]
#[derive(Parser)]
#[command(name = "gnomon", about = "A plaintext calendaring language")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

// r[impl cli.subcommand.help]
// r[impl cli.subcommand.help.root]
// r[impl cli.subcommand.help.penultimate]
// r[impl cli.subcommand.order]
// r[impl cli.subcommand.reserved+5]
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

        // r[impl cli.subcommand.check.refresh]
        /// Force re-fetching all URI imports, bypassing the cache.
        #[arg(long)]
        refresh: bool,
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

        // r[impl cli.subcommand.eval.refresh]
        /// Force re-fetching all URI imports, bypassing the cache.
        #[arg(long)]
        refresh: bool,
    },
    // r[impl cli.subcommand.compile]
    /// Compile a .gnomon calendar project to iCalendar or JSCalendar.
    Compile {
        /// Path to the root .gnomon file.
        file: PathBuf,

        // r[impl cli.subcommand.compile.format]
        /// Output format: icalendar (default) or jscalendar.
        #[arg(long, default_value = "icalendar")]
        format: ExportFormat,

        // r[impl cli.subcommand.compile.refresh]
        /// Force re-fetching all URI imports, bypassing the cache.
        #[arg(long)]
        refresh: bool,
    },
    // r[impl cli.subcommand.clean]
    /// Remove all cached URI imports.
    Clean,
    // r[impl cli.subcommand.repl]
    /// Start an interactive REPL session.
    Repl,
}

#[derive(Clone, Debug)]
enum ExportFormat {
    Icalendar,
    Jscalendar,
}

impl std::str::FromStr for ExportFormat {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "icalendar" => Ok(ExportFormat::Icalendar),
            "jscalendar" => Ok(ExportFormat::Jscalendar),
            _ => Err(format!(
                "unknown format '{s}': expected 'icalendar' or 'jscalendar'"
            )),
        }
    }
}

fn main() -> ExitCode {
    // r[impl cli.option.version]
    // r[impl cli.option.version.short]
    // r[impl cli.option.version.behavior]
    // r[impl cli.option.version.xor]
    let mut cmd = Cli::command()
        .disable_help_flag(true)
        .disable_version_flag(true)
        .arg(
            clap::Arg::new("version")
                .short('v')
                .long("version")
                .action(clap::ArgAction::SetTrue)
                .conflicts_with("help")
                .help("Print version"),
        )
        .arg(
            clap::Arg::new("help")
                .short('h')
                .long("help")
                .action(clap::ArgAction::SetTrue)
                .conflicts_with("version")
                .help("Print help"),
        );

    // Re-add --help to subcommands with the standard early-exit behavior.
    // (disable_help_flag propagates to subcommands, so we must restore it.)
    for sub in cmd.get_subcommands_mut() {
        *sub = sub.clone().arg(
            clap::Arg::new("help")
                .short('h')
                .long("help")
                .action(clap::ArgAction::Help)
                .help("Print help"),
        );
    }

    let matches = cmd.clone().get_matches();

    if matches.get_flag("version") {
        println!("gnomon {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    if matches.get_flag("help") {
        cmd.print_help().unwrap();
        println!();
        return ExitCode::SUCCESS;
    }

    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());

    let Some(command) = cli.command else {
        let _ = cmd.write_help(&mut std::io::stderr());
        eprintln!();
        return ExitCode::FAILURE;
    };

    match command {
        Command::Parse { file } => {
            // r[impl cli.subcommand.parse.no-file]
            // r[impl lexer.input-format.malformed]
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
        Command::Eval {
            file,
            expr,
            refresh,
        } => {
            let db = Database::default();
            let options = EvalOptions {
                force_refresh: refresh,
            };

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
                (None, Some(expr)) => SourceFile::new(&db, PathBuf::from("<expr>"), expr.clone()),
                _ => {
                    eprintln!("error: provide either a file path or --expr");
                    return ExitCode::FAILURE;
                }
            };

            let result = evaluate_with_options(&db, source, &options);

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
        Command::Check { file, refresh } => {
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
            let options = EvalOptions {
                force_refresh: refresh,
            };

            // Evaluate the root file.
            let eval_result = evaluate_with_options(&db, source, &options);
            let imported_files = eval_result.imported_files.clone();

            // Validate the evaluated value as a calendar.
            let check_result =
                validate_calendar(&db, source, eval_result.value, eval_result.diagnostics);

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
        // r[impl cli.subcommand.compile]
        Command::Compile {
            file,
            format,
            refresh,
        } => {
            // r[impl cli.subcommand.compile.no-file]
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
            let options = EvalOptions {
                force_refresh: refresh,
            };

            // r[impl cli.subcommand.compile.validate]
            // Run the full check pipeline.
            let eval_result = evaluate_with_options(&db, source, &options);
            let imported_files = eval_result.imported_files.clone();

            let check_result =
                validate_calendar(&db, source, eval_result.value, eval_result.diagnostics);

            let mut diagnostics = check_result.diagnostics;

            // r[impl cli.subcommand.compile.unused]
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

            let has_errors = print_diagnostics(&db, &diagnostics);

            if has_errors {
                return ExitCode::FAILURE;
            }

            // r[impl cli.subcommand.compile.output]
            // Compile each calendar and write output to a buffer.
            let mut outputs: Vec<String> = Vec::new();
            let mut export_warnings: Vec<String> = Vec::new();
            for calendar in &check_result.calendars {
                let (cal_record, entries) = calendar_to_import_values(&db, calendar);
                let mut buf = String::new();
                let result = match format {
                    ExportFormat::Icalendar => {
                        emit_icalendar(&mut buf, &cal_record, &entries, &mut export_warnings)
                    }
                    ExportFormat::Jscalendar => emit_jscalendar(&mut buf, &cal_record, &entries),
                };
                if let Err(e) = result {
                    eprintln!("error: export failed: {e}");
                    return ExitCode::FAILURE;
                }
                outputs.push(buf);
            }
            for w in &export_warnings {
                eprintln!("warning: {w}");
            }

            use std::io::Write;
            let out = std::io::stdout();
            let mut lock = out.lock();
            match format {
                ExportFormat::Icalendar => {
                    // For iCalendar: concatenate VCALENDAR outputs.
                    for s in &outputs {
                        let _ = write!(lock, "{s}");
                    }
                }
                ExportFormat::Jscalendar => {
                    // Each output is a JSCalendar Group. Single group → emit
                    // directly; multiple groups → wrap in a JSON array.
                    if outputs.len() == 1 {
                        let _ = writeln!(lock, "{}", outputs[0]);
                    } else {
                        let mut groups: Vec<serde_json::Value> = Vec::new();
                        for s in &outputs {
                            match serde_json::from_str::<serde_json::Value>(s) {
                                Ok(val) => groups.push(val),
                                Err(e) => {
                                    eprintln!("error: invalid JSON output: {e}");
                                    return ExitCode::FAILURE;
                                }
                            }
                        }
                        let combined = serde_json::to_string_pretty(&groups).unwrap();
                        let _ = writeln!(lock, "{combined}");
                    }
                }
            }

            ExitCode::SUCCESS
        }
        Command::Repl => repl::run_repl(),
        // r[impl cli.subcommand.clean]
        Command::Clean => match gnomon_db::eval::cache::clean() {
            Ok(n) => {
                println!("{n} cached URI import(s) removed");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("error: failed to clean cache: {e}");
                ExitCode::FAILURE
            }
        },
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
