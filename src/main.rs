use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "gnomon", about = "A calendar language toolkit")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Parse a .gnomon file and print its syntax tree.
    Parse {
        /// Path to the file to parse.
        file: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Command::Parse { file } => {
            let source = match std::fs::read_to_string(&file) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("error: could not read {}: {e}", file.display());
                    return ExitCode::FAILURE;
                }
            };

            let parse = gnomon_parser::parse(&source);
            println!("{}", parse.debug_tree());

            if !parse.ok() {
                eprintln!("errors:");
                for err in parse.errors() {
                    eprintln!("  {err:?}");
                }
                return ExitCode::FAILURE;
            }

            ExitCode::SUCCESS
        }
    }
}
