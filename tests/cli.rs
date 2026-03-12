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

// r[verify cli.subcommand.reserved+5]
#[test]
fn reserved_subcommands_rejected() {
    for name in [
        "about", "daemon", "fetch", "lsp", "merge", "query", "run",
    ] {
        gnomon().arg(name).assert().failure();
    }
}

// ── REPL subcommand ─────────────────────────────────────────

// r[verify cli.subcommand.repl]
// r[verify cli.subcommand.repl.eval]
#[test]
fn repl_evaluates_expression() {
    gnomon()
        .arg("repl")
        .write_stdin("42\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("42"));
}

// r[verify cli.subcommand.repl.let-persist]
#[test]
fn repl_let_bindings_persist() {
    gnomon()
        .arg("repl")
        .write_stdin("let x = 10\nx\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("10"));
}

// r[verify cli.subcommand.repl.diagnostics]
#[test]
fn repl_errors_on_stderr() {
    gnomon()
        .arg("repl")
        .write_stdin("{ name: }\n")
        .assert()
        .success()
        .stderr(predicate::str::contains("error"));
}

// r[verify cli.subcommand.repl.multiline]
#[test]
fn repl_multiline_input() {
    gnomon()
        .arg("repl")
        .write_stdin("{\n  name: \"test\"\n}\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("name"));
}

// r[verify cli.subcommand.repl.meta.help]
#[test]
fn repl_meta_help() {
    gnomon()
        .arg("repl")
        .write_stdin(":help\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("Available commands"));
}

// r[verify cli.subcommand.repl.meta.type]
#[test]
fn repl_meta_type() {
    gnomon()
        .arg("repl")
        .write_stdin(":type 42\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("integer"));
}

// r[verify cli.subcommand.repl.meta.parse]
#[test]
fn repl_meta_parse() {
    gnomon()
        .arg("repl")
        .write_stdin(":parse { x: 1 }\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("RECORD_EXPR"));
}

// r[verify cli.subcommand.repl.meta.reset]
#[test]
fn repl_meta_reset() {
    gnomon()
        .arg("repl")
        .write_stdin("let x = 1\n:reset\nx\n")
        .assert()
        .success()
        .stderr(predicate::str::contains("error"));
}

// r[verify cli.subcommand.repl.meta.quit]
#[test]
fn repl_meta_quit() {
    gnomon()
        .arg("repl")
        .write_stdin(":quit\n")
        .assert()
        .success();
}

// ── Clean subcommand ───────────────────────────────────────

// r[verify cli.subcommand.clean]
#[test]
fn clean_subcommand() {
    gnomon()
        .arg("clean")
        .assert()
        .success()
        .stdout(predicates::str::contains("cached URI import(s) removed"));
}

// ── Compile subcommand ──────────────────────────────────────

// r[verify cli.subcommand.compile]
// r[verify cli.subcommand.compile.output]
#[test]
fn compile_ical_output() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_temp_file(
        &dir,
        "project.gnomon",
        r#"
calendar {
    uid: "550e8400-e29b-41d4-a716-446655440000"
}
event @standup 2026-03-15T09:00 1h "Standup"
"#,
    );
    gnomon()
        .args(["compile", file.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("BEGIN:VCALENDAR"))
        .stdout(predicate::str::contains("BEGIN:VEVENT"))
        .stdout(predicate::str::contains("SUMMARY:Standup"))
        .stdout(predicate::str::contains("END:VEVENT"))
        .stdout(predicate::str::contains("END:VCALENDAR"));
}

// r[verify cli.subcommand.compile.format]
#[test]
fn compile_jscal_output() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_temp_file(
        &dir,
        "project.gnomon",
        r#"
calendar {
    uid: "550e8400-e29b-41d4-a716-446655440000"
}
event @meeting 2026-03-15T14:00 1h "Meeting"
"#,
    );
    gnomon()
        .args(["compile", file.to_str().unwrap(), "--format", "jscal"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"@type\": \"Event\""))
        .stdout(predicate::str::contains("Meeting"));
}

// r[verify cli.subcommand.compile.no-file]
#[test]
fn compile_missing_file() {
    gnomon()
        .args(["compile", "/nonexistent/path.gnomon"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("could not read"));
}

// r[verify cli.subcommand.compile.validate]
#[test]
fn compile_validation_error_exits_nonzero() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_temp_file(
        &dir,
        "project.gnomon",
        r#"
event @orphan 2026-03-15T09:00 1h "No Calendar"
"#,
    );
    gnomon()
        .args(["compile", file.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

// r[verify cli.subcommand.compile.format]
#[test]
fn compile_invalid_format_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_temp_file(&dir, "project.gnomon", "calendar { uid: \"abc\" }");
    gnomon()
        .args(["compile", file.to_str().unwrap(), "--format", "xml"])
        .assert()
        .failure();
}

// r[verify cli.subcommand.compile.unused]
#[test]
fn compile_unused_file_warning() {
    let dir = tempfile::tempdir().unwrap();
    let _root = write_temp_file(
        &dir,
        "project.gnomon",
        r#"
calendar {
    uid: "550e8400-e29b-41d4-a716-446655440000"
}
event @test 2026-03-15T09:00 1h "Test"
"#,
    );
    let _unused = write_temp_file(&dir, "unused.gnomon", "event @orphan 2026-01-01T00:00 1h \"Orphan\"");
    gnomon()
        .args(["compile", _root.to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("not imported"));
}
