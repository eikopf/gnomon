use std::path::PathBuf;
use std::process::ExitCode;

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use gnomon_db::{parse, Database, Diagnostic, SourceFile};

// r[impl cli.root]
// r[impl cli.option.help]
// r[impl cli.option.help.short]
#[derive(Parser)]
#[command(name = "gnomon", about = "A calendar language toolkit", arg_required_else_help = true)]
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
                    eprintln!("  {}..{}: {}", u32::from(diag.range.start()), u32::from(diag.range.end()), diag.message);
                }
                return ExitCode::FAILURE;
            }

            ExitCode::SUCCESS
        }
    }
}
