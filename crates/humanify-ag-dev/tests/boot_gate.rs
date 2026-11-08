//! The boot gate's launches, recorded from the real `humanify-ag-dev
//! boot-gate` under a clean environment holding only seeded session state
//! (the exp191 method): the `timeout` deadline, the `env -u` scrub list,
//! bun's argv and cwd, and which variables reach the walked CLI must equal
//! `launch-goldens/boot-gate.golden.txt`.
//!
//! One normalisation, stated: the golden's `bun-env|` line also lists
//! `OLDPWD` and `SHLVL`, which bash itself created when the shell gate ran
//! `cd "$1" && timeout ...` (`cd` sets OLDPWD; a shell exports SHLVL). The
//! Rust gate starts `timeout` with no shell in between, so neither exists;
//! both are removed from the golden's line before comparing. Every other
//! byte is compared.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

fn write_exec(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

const SEEDED: &[(&str, &str)] = &[
    ("ANTHROPIC_MODEL", "m"),
    ("ANTHROPIC_DEFAULT_OPUS_MODEL", "m"),
    ("ANTHROPIC_DEFAULT_SONNET_MODEL", "m"),
    ("ANTHROPIC_DEFAULT_HAIKU_MODEL", "m"),
    ("ANTHROPIC_DEFAULT_FABLE_MODEL", "m"),
    ("ANTHROPIC_SMALL_FAST_MODEL", "m"),
    ("CLAUDE_CODE_SUBAGENT_MODEL", "m"),
    ("ANTHROPIC_BASE_URL", "http://example.invalid"),
    ("ANTHROPIC_API_KEY", "k"),
    ("ANTHROPIC_AUTH_TOKEN", "t"),
    ("CLAUDECODE", "1"),
    ("CLAUDE_CODE_ENTRYPOINT", "cli"),
    ("CLAUDE_CODE_CHILD_SESSION", "1"),
    ("CLAUDE_CODE_SESSION_ID", "s"),
    ("CLAUDE_CODE_MESSAGING_SOCKET", "s"),
    ("CLAUDE_CODE_MESSAGING_TOKEN", "t"),
    ("CLAUDE_CODE_BRIDGE_SESSION_ID", "s"),
    ("CLAUDE_CONFIG_DIR", "/cfg"),
    ("KEEP_ME", "1"),
];

#[test]
fn the_boot_gate_launches_and_scrub_equal_the_golden() {
    let root = std::env::temp_dir().join(format!("l3-boot-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (shims, home, tree) = (root.join("shims"), root.join("home"), root.join("tree"));
    std::fs::create_dir_all(&tree).unwrap();
    std::fs::write(root.join("calls.log"), "").unwrap();
    for tool in ["timeout", "env"] {
        write_exec(
            &shims.join(tool),
            &format!(
                "#!/bin/bash\necho \"{tool} :: $(printf '%q ' \"$@\")\" >> \"$RECORDER_ROOT/calls.log\"\nexec /usr/bin/{tool} \"$@\"\n"
            ),
        );
    }
    write_exec(
        &home.join(".bun/bin/bun"),
        "#!/bin/bash\necho \"bun (in $PWD) :: $(printf '%q ' \"$@\")\" >> \"$RECORDER_ROOT/calls.log\"\necho \"bun-env| $(/usr/bin/env | sed 's/=.*//' | grep -v '^_$' | sort | tr '\\n' ' ')\" >> \"$RECORDER_ROOT/calls.log\"\necho \"boot-ok\"",
    );
    let mut cmd = Command::new("/usr/bin/env");
    cmd.arg("-i")
        .arg(format!("PATH={}:/usr/bin:/bin", shims.display()))
        .arg(format!("HOME={}", home.display()))
        .arg(format!("RECORDER_ROOT={}", root.display()));
    for (k, v) in SEEDED {
        cmd.arg(format!("{k}={v}"));
    }
    cmd.arg(env!("CARGO_BIN_EXE_humanify-ag-dev"))
        .arg("boot-gate")
        .arg(&tree)
        .arg("2.1.86");
    let out = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("prompt: boot-ok"),
        "{stdout}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let calls = std::fs::read_to_string(root.join("calls.log"))
        .unwrap()
        .replace(&root.display().to_string(), "<TMP>");
    let golden_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../experiments/034-eval-harness/launch-goldens/boot-gate.golden.txt");
    let golden = std::fs::read_to_string(golden_path)
        .unwrap()
        .replace(" OLDPWD ", " ")
        .replace(" SHLVL ", " ");
    let calls = calls.replace(" SHLVL ", " ");
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(calls, golden);
    assert!(
        !calls.contains("bun-env| ANTHROPIC"),
        "a session model override reached the walked CLI"
    );
}
