//! The `dagenda` binary as a user runs it: exit codes, where output goes,
//! and the outputs it refuses to write.

use std::{
    env, fs, io,
    io::Write as _,
    path::{Path, PathBuf},
    process::{self, Command, Output, Stdio},
};
use testresult::TestResult;

const CLEAN: &str = "roadmap\n[x] parser\n[ ] resolver\n  needs parser\n";
const WARNS: &str = "roadmap\n[~] dropped\nmilestone m\n  dropped\n";
const BROKEN: &str = "roadmap\n[ ] a\n  needs ghost\n";

/// A scratch directory, removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> io::Result<Self> {
        let path = env::temp_dir().join(format!("dagenda-cli-{}-{name}", process::id()));
        // A leftover from an earlier run with the same pid
        drop(fs::remove_dir_all(&path));
        fs::create_dir_all(&path)?;
        Ok(Self(path))
    }

    fn write(&self, name: &str, text: &str) -> io::Result<PathBuf> {
        let path = self.0.join(name);
        fs::write(&path, text)?;
        Ok(path)
    }

    fn read(&self, name: &str) -> io::Result<String> {
        fs::read_to_string(self.0.join(name))
    }

    fn has(&self, name: &str) -> bool {
        self.0.join(name).exists()
    }

    fn run(&self, args: &[&str]) -> io::Result<Output> {
        Command::new(env!("CARGO_BIN_EXE_dagenda"))
            .current_dir(&self.0)
            .args(args)
            .output()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        drop(fs::remove_dir_all(&self.0));
    }
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn exit_codes_follow_the_problems() -> TestResult {
    let dir = Scratch::new("exit")?;
    dir.write("clean.dagenda", CLEAN)?;
    dir.write("warns.dagenda", WARNS)?;
    dir.write("broken.dagenda", BROKEN)?;

    let clean = dir.run(&["check", "clean.dagenda"])?;
    assert_eq!(clean.status.code(), Some(0));
    assert!(clean.stderr.is_empty(), "{}", stderr(&clean));

    let warns = dir.run(&["check", "warns.dagenda"])?;
    assert_eq!(warns.status.code(), Some(0), "warnings alone pass");
    assert!(stderr(&warns).starts_with("warns.dagenda:4:3: warning: "));

    let denied = dir.run(&["check", "warns.dagenda", "--deny-warnings"])?;
    assert_eq!(denied.status.code(), Some(1));

    let broken = dir.run(&["check", "broken.dagenda"])?;
    assert_eq!(broken.status.code(), Some(1));
    assert_eq!(
        stderr(&broken),
        "broken.dagenda:3:9: error: nothing in the plan is called `ghost`\n"
    );

    let missing = dir.run(&["check", "missing.dagenda"])?;
    assert_eq!(missing.status.code(), Some(1));
    assert!(stderr(&missing).starts_with("dagenda: can't read missing.dagenda: "));

    let usage = dir.run(&["check"])?;
    assert_eq!(usage.status.code(), Some(2));
    Ok(())
}

#[test]
fn render_never_writes_over_its_own_documents() -> TestResult {
    let dir = Scratch::new("overwrite")?;
    let plan = dir.write("plan.dagenda", CLEAN)?;
    dir.write("private.dagenda", "roadmap\nextend resolver\n  dri @bob\n")?;

    for args in [
        &["render", "plan.dagenda", "-o", "plan.dagenda"][..],
        &["render", "plan.dagenda", "-o", "./plan.dagenda"],
        &[
            "render",
            "plan.dagenda",
            "--format",
            "text",
            "-o",
            "plan.dagenda",
        ],
        &[
            "render",
            "plan.dagenda",
            "private.dagenda",
            "-o",
            "private.dagenda",
        ],
    ] {
        let output = dir.run(args)?;
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(
            stderr(&output).contains("won't write over"),
            "{args:?}: {}",
            stderr(&output)
        );
    }
    assert_eq!(dir.read("plan.dagenda")?, CLEAN, "the plan is untouched");

    // Spelled as an absolute path, from somewhere else
    let elsewhere = Command::new(env!("CARGO_BIN_EXE_dagenda"))
        .current_dir(env::temp_dir())
        .args([Path::new("render"), &plan, Path::new("-o"), &plan])
        .output()?;
    assert_eq!(elsewhere.status.code(), Some(2));

    // Through a symlink
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&plan, dir.0.join("alias.svg"))?;
        let output = dir.run(&["render", "plan.dagenda", "-o", "alias.svg"])?;
        assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    }
    assert_eq!(dir.read("plan.dagenda")?, CLEAN, "the plan is untouched");
    Ok(())
}

#[test]
fn render_refuses_an_output_it_cant_name() -> TestResult {
    let dir = Scratch::new("unknown")?;
    dir.write("plan.dagenda", CLEAN)?;
    for output in ["out.png", "out", "out.dagenda"] {
        let run = dir.run(&["render", "plan.dagenda", "-o", output])?;
        assert_eq!(run.status.code(), Some(2), "{output}");
        assert!(
            stderr(&run).contains("--format"),
            "says how: {}",
            stderr(&run)
        );
        assert!(!dir.has(output), "{output} isn't written");
    }
    Ok(())
}

#[test]
fn render_takes_the_format_from_the_extension_in_any_case() -> TestResult {
    let dir = Scratch::new("format")?;
    dir.write("plan.dagenda", CLEAN)?;
    for (output, svg) in [("out.svg", true), ("OUT.SVG", true), ("out.Txt", false)] {
        let run = dir.run(&["render", "plan.dagenda", "-o", output])?;
        assert_eq!(run.status.code(), Some(0), "{output}: {}", stderr(&run));
        assert_eq!(dir.read(output)?.starts_with("<svg"), svg, "{output}");
    }

    let forced = dir.run(&["render", "plan.dagenda", "--format", "svg", "-o", "out.png"])?;
    assert_eq!(forced.status.code(), Some(0));
    assert!(dir.read("out.png")?.starts_with("<svg"), "--format wins");

    let stdout = dir.run(&["render", "plan.dagenda"])?;
    assert!(String::from_utf8(stdout.stdout)?.contains("[ ] resolver"));
    Ok(())
}

#[test]
fn a_dash_reads_stdin() -> TestResult {
    let mut child = Command::new(env!("CARGO_BIN_EXE_dagenda"))
        .args(["check", "-"])
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .ok_or("no stdin")?
        .write_all(BROKEN.as_bytes())?;
    let output = child.wait_with_output()?;
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).starts_with("<stdin>:3:9: error: "));
    Ok(())
}
