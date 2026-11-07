use super::*;

#[test]
fn the_pin_and_the_msrv_are_read_from_their_files() {
    let toolchain = "# c\n[toolchain]\nchannel = \"1.98.1\"\ncomponents = [\"clippy\"]\n";
    assert_eq!(pin(toolchain).as_deref(), Some("1.98.1"));
    let cargo =
        "[workspace.package]\nedition = \"2024\"\nrust-version = \"1.98.1\" # equals the pin\n";
    assert_eq!(msrv(cargo).as_deref(), Some("1.98.1"));
    assert_eq!(pin("[toolchain]\n"), None);
}

#[test]
fn any_failed_line_fails_the_check_and_says_so() {
    let ok = vec![
        Line::ok("rustc", "1.98.1 (= pin)"),
        Line::info("nextest", "absent"),
    ];
    let (code, text) = report(&ok);
    assert_eq!(code, 0);
    assert!(text.ends_with("RUST ENV OK\n"), "{text}");
    let bad = vec![Line::ok("rustc", "x"), Line::fail("bun", "absent")];
    let (code, text) = report(&bad);
    assert_eq!(code, 1);
    assert!(
        text.contains("bun                          FAIL — absent"),
        "{text}"
    );
    assert!(text.ends_with("RUST ENV NOT OK\n"), "{text}");
}

#[test]
fn this_repos_pin_equals_its_msrv() {
    let repo = crate::paths::repo_root();
    let t = std::fs::read_to_string(repo.join("rust-toolchain.toml")).unwrap();
    let c = std::fs::read_to_string(repo.join("Cargo.toml")).unwrap();
    assert!(pin(&t).is_some());
    assert_eq!(pin(&t), msrv(&c));
}
