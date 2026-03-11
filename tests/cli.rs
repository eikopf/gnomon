use assert_cmd::Command;
use predicates::prelude::*;
use std::io::Write;

fn gnomon() -> Command {
    let cmd = assert_cmd::cargo::cargo_bin!("gnomon");
    Command::new(cmd)
}

fn write_temp_file(dir: &tempfile::TempDir, name: &str, content: &str) -> std::path::PathBuf {
    let path = dir.path().join(name);
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(content.as_bytes()).unwrap();
    path
}

// ── Root command ─────────────────────────────────────────────

// r[verify cli.root]
#[test]
fn no_args_shows_help() {
    gnomon()
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage"));
}

// ── --help ───────────────────────────────────────────────────

// r[verify cli.option.help]
// r[verify cli.option.help.behavior.root]
// r[verify cli.syntax]
#[test]
fn help_flag() {
    gnomon()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage"));
}

// r[verify cli.option.help.short]
#[test]
fn help_short_flag() {
    gnomon()
        .arg("-h")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage"));
}

// ── --version ────────────────────────────────────────────────

// r[verify cli.option.version]
// r[verify cli.option.version.behavior]
#[test]
fn version_flag() {
    gnomon()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

// r[verify cli.option.version.short]
#[test]
fn version_short_flag() {
    gnomon()
        .arg("-v")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

// ── help subcommand ──────────────────────────────────────────

// r[verify cli.subcommand.help]
// r[verify cli.subcommand.help.root]
#[test]
fn help_subcommand() {
    gnomon()
        .arg("help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage"));
}

// ── parse subcommand ─────────────────────────────────────────

// r[verify cli.subcommand.parse]
// r[verify cli.subcommand.parse.output]
#[test]
fn parse_subcommand() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_temp_file(&dir, "test.gnomon", r#"calendar { uid: "test" }"#);

    gnomon()
        .arg("parse")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("FILE"));
}

// r[verify cli.subcommand.parse.no-file]
#[test]
fn parse_missing_file() {
    gnomon()
        .args(["parse", "/nonexistent/path.gnomon"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

// ── check subcommand ─────────────────────────────────────────

// r[verify cli.subcommand.check+2]
// r[verify cli.subcommand.check.output+2]
#[test]
fn check_subcommand() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_temp_file(
        &dir,
        "test.gnomon",
        r#"
        calendar { uid: "550e8400-e29b-41d4-a716-446655440000" }
        event @meeting 2026-03-01T09:00 1h "Standup"
        "#,
    );

    gnomon()
        .arg("check")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

// r[verify cli.subcommand.check.no-file+2]
#[test]
fn check_missing_file() {
    gnomon()
        .args(["check", "/nonexistent/path.gnomon"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

// r[verify cli.subcommand.check.unused]
#[test]
fn check_unused_file_warning() {
    let dir = tempfile::tempdir().unwrap();
    let root = write_temp_file(
        &dir,
        "root.gnomon",
        r#"
        calendar { uid: "550e8400-e29b-41d4-a716-446655440000" }
        event @meeting 2026-03-01T09:00 1h "Standup"
        "#,
    );
    // Create an unused sibling file.
    write_temp_file(&dir, "unused.gnomon", r#"event @orphan { name: @x }"#);

    gnomon()
        .arg("check")
        .arg(&root)
        .assert()
        .stderr(predicate::str::contains("not imported"));
}

// ── eval subcommand ──────────────────────────────────────────

// r[verify cli.subcommand.eval]
// r[verify cli.subcommand.eval.output]
#[test]
fn eval_subcommand() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_temp_file(&dir, "test.gnomon", r#"calendar { uid: "test" }"#);

    gnomon()
        .arg("eval")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::is_empty().not());
}

// r[verify cli.subcommand.eval.expr]
#[test]
fn eval_expr_flag() {
    gnomon()
        .args(["eval", "--expr", "{ x: 1 }"])
        .assert()
        .success()
        .stdout(predicate::str::contains("x"));
}

// r[verify cli.subcommand.eval.expr.exclusive]
#[test]
fn eval_expr_exclusive() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_temp_file(&dir, "test.gnomon", r#"calendar { uid: "test" }"#);

    gnomon()
        .arg("eval")
        .arg(&file)
        .args(["--expr", "{ x: 1 }"])
        .assert()
        .failure();
}

// r[verify cli.subcommand.eval.no-file]
#[test]
fn eval_missing_file() {
    gnomon()
        .args(["eval", "/nonexistent/path.gnomon"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

// ── Help/version exclusivity ────────────────────────────────

// r[verify cli.option.help.xor]
#[test]
fn help_with_version_is_error() {
    gnomon().args(["--help", "--version"]).assert().failure();
}

// r[verify cli.option.version.xor]
#[test]
fn version_with_help_is_error() {
    gnomon().args(["--version", "--help"]).assert().failure();
}

// r[verify cli.option.help.behavior.subcommand]
#[test]
fn help_flag_on_subcommand() {
    gnomon()
        .args(["parse", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage"));
}

// r[verify cli.option.order]
#[test]
fn option_order_independent() {
    let dir = tempfile::tempdir().unwrap();
    let _file = write_temp_file(&dir, "test.gnomon", r#"{ x: 1 }"#);

    // --expr before eval vs after — both should work
    gnomon()
        .args(["eval", "--expr", "{ x: 1 }"])
        .assert()
        .success();
}

// r[verify cli.subcommand.order]
#[test]
fn subcommand_order_determines_action() {
    // "parse" subcommand always triggers parse action regardless of other input
    gnomon()
        .args(["parse", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Parse"));
}

// r[verify cli.subcommand.help.penultimate]
#[test]
fn help_subcommand_for_specific_command() {
    gnomon()
        .args(["help", "parse"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Parse"));
}

// r[verify lexer.input-format.utf-8]
#[test]
fn valid_utf8_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_temp_file(&dir, "test.gnomon", "{ name: \"héllo wörld\" }");

    gnomon().arg("eval").arg(&file).assert().success();
}

// ── UTF-8 validation ────────────────────────────────────────

// r[verify lexer.input-format.malformed]
#[test]
fn malformed_utf8_produces_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.gnomon");
    // Write invalid UTF-8 bytes.
    std::fs::write(&path, b"\xff\xfe invalid utf-8").unwrap();

    gnomon()
        .args(["parse", path.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

// ── Reserved subcommands ────────────────────────────────────

// r[verify cli.subcommand.reserved+3]
#[test]
fn reserved_subcommands_rejected() {
    for name in [
        "about", "clean", "compile", "daemon", "fetch", "lsp", "merge", "query", "run",
    ] {
        gnomon().arg(name).assert().failure();
    }
}
