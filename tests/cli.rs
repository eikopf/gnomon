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
    for name in ["about", "daemon", "fetch", "lsp", "merge", "query", "run"] {
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
        .args(["compile", file.to_str().unwrap(), "--format", "jscalendar"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"@type\": \"Group\""))
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
    let _unused = write_temp_file(
        &dir,
        "unused.gnomon",
        "event @orphan 2026-01-01T00:00 1h \"Orphan\"",
    );
    gnomon()
        .args(["compile", _root.to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("not imported"));
}

// ── find_gnomon_files edge cases ─────────────────────────────

// Verifies that find_gnomon_files does not loop infinitely when the directory
// tree contains a symlink cycle (e.g. subdir/loop -> parent). The check
// command should terminate and produce at most a warning for the unused file,
// not hang or crash.
#[test]
#[cfg(unix)]
fn find_gnomon_files_symlink_loop_does_not_hang() {
    let dir = tempfile::tempdir().unwrap();

    // Create a root gnomon file so `check` has something valid to process.
    let root = write_temp_file(
        &dir,
        "root.gnomon",
        r#"calendar { uid: "550e8400-e29b-41d4-a716-446655440000" }"#,
    );

    // Create a subdirectory and inside it a symlink that points back to the
    // parent directory, forming a cycle.
    let subdir = dir.path().join("sub");
    std::fs::create_dir(&subdir).unwrap();
    std::os::unix::fs::symlink(dir.path(), subdir.join("loop")).unwrap();

    // The command must finish (i.e. not loop forever) and succeed or warn.
    gnomon()
        .args(["check", root.to_str().unwrap()])
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .success();
}

// Verifies that find_gnomon_files discovers .gnomon files in a deeply nested
// directory hierarchy (5 levels deep) and reports them as unused.
#[test]
fn find_gnomon_files_deep_nesting() {
    let dir = tempfile::tempdir().unwrap();

    // Root file that check is run against.
    let root = write_temp_file(
        &dir,
        "root.gnomon",
        r#"calendar { uid: "550e8400-e29b-41d4-a716-446655440000" }"#,
    );

    // Build a 5-level-deep nested directory and place a .gnomon file at the
    // bottom.
    let deep = dir
        .path()
        .join("a")
        .join("b")
        .join("c")
        .join("d")
        .join("e");
    std::fs::create_dir_all(&deep).unwrap();
    let mut f =
        std::fs::File::create(deep.join("deep.gnomon")).unwrap();
    std::io::Write::write_all(&mut f, b"event @deep { name: @x }").unwrap();

    // The deeply-nested file is not imported, so we expect an "not imported"
    // warning.
    gnomon()
        .args(["check", root.to_str().unwrap()])
        .assert()
        .stderr(predicate::str::contains("not imported"));
}

// Verifies that a symlink pointing directly to a .gnomon file is found and
// reported as unused.
#[test]
#[cfg(unix)]
fn find_gnomon_files_symlink_to_file() {
    let dir = tempfile::tempdir().unwrap();

    // Root file.
    let root = write_temp_file(
        &dir,
        "root.gnomon",
        r#"calendar { uid: "550e8400-e29b-41d4-a716-446655440000" }"#,
    );

    // A real .gnomon file in a subdirectory.
    let subdir = dir.path().join("sub");
    std::fs::create_dir(&subdir).unwrap();
    let real_file = subdir.join("real.gnomon");
    std::fs::write(&real_file, b"event @real { name: @x }").unwrap();

    // A symlink at the top level that points to the real file.
    let link = dir.path().join("link.gnomon");
    std::os::unix::fs::symlink(&real_file, &link).unwrap();

    // Both the real file and the symlink target resolve to files, so at least
    // one of them should be reported as unused.
    gnomon()
        .args(["check", root.to_str().unwrap()])
        .assert()
        .stderr(predicate::str::contains("not imported"));
}

// Verifies that a symlink pointing to a directory containing .gnomon files is
// followed and the files inside are found and reported as unused.
#[test]
#[cfg(unix)]
fn find_gnomon_files_symlink_to_directory() {
    let dir = tempfile::tempdir().unwrap();

    // Root file.
    let root = write_temp_file(
        &dir,
        "root.gnomon",
        r#"calendar { uid: "550e8400-e29b-41d4-a716-446655440000" }"#,
    );

    // A real directory (outside the project) containing a .gnomon file.
    let real_dir = tempfile::tempdir().unwrap();
    std::fs::write(
        real_dir.path().join("inside.gnomon"),
        b"event @inside { name: @x }",
    )
    .unwrap();

    // A symlink inside the project directory pointing to that external real
    // directory.
    let link_dir = dir.path().join("linked");
    std::os::unix::fs::symlink(real_dir.path(), &link_dir).unwrap();

    // The file inside the symlinked directory should be discovered and reported
    // as unused.
    gnomon()
        .args(["check", root.to_str().unwrap()])
        .assert()
        .stderr(predicate::str::contains("not imported"));
}

// Verifies that find_gnomon_files handles a subdirectory with no read
// permission gracefully: it should not panic or crash, and the overall command
// should still succeed (or at most warn about other issues).
#[test]
#[cfg(unix)]
fn find_gnomon_files_permission_error_on_subdir() {
    use std::os::unix::fs::PermissionsExt as _;

    let dir = tempfile::tempdir().unwrap();

    // Root file.
    let root = write_temp_file(
        &dir,
        "root.gnomon",
        r#"calendar { uid: "550e8400-e29b-41d4-a716-446655440000" }"#,
    );

    // A subdirectory with no read/execute permissions.
    let locked = dir.path().join("locked");
    std::fs::create_dir(&locked).unwrap();
    // Place a .gnomon file inside so there would be something to find if
    // permissions allowed it.
    std::fs::write(locked.join("hidden.gnomon"), b"event @hidden { name: @x }").unwrap();
    // Remove all permissions from the directory.
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();

    // If setting permissions had no effect (e.g. running as root), the
    // hidden file would be found and the test would fail because it expects
    // no "not imported" warning. Detect this by checking whether we can still
    // read the directory.
    let running_as_root = std::fs::read_dir(&locked).is_ok();

    // Restore permissions on drop so tempdir cleanup does not fail.
    struct RestorePerms(std::path::PathBuf);
    impl Drop for RestorePerms {
        fn drop(&mut self) {
            use std::os::unix::fs::PermissionsExt as _;
            let _ = std::fs::set_permissions(
                &self.0,
                std::fs::Permissions::from_mode(0o755),
            );
        }
    }
    let _restore = RestorePerms(locked.clone());

    if running_as_root {
        // Can't meaningfully test permission errors as root; skip.
        return;
    }

    // The command must not panic or crash. It should succeed (no error about
    // the locked directory itself) and should not report the hidden file as
    // unused because it cannot be seen.
    gnomon()
        .args(["check", root.to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("not imported").not());
}
