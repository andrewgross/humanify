use super::*;

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("dev-e2e-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn a_missing_binary_fails_loudly_with_the_build_command() {
    let repo = scratch("nobin");
    let (code, _, stderr) = e2e(&repo, None, "");
    assert_eq!(code, 1);
    assert!(stderr.starts_with("E2E FAILED: no binary at "), "{stderr}");
    assert!(stderr.contains("cargo build --release --locked -p humanify-ag-cli"));
    std::fs::remove_dir_all(&repo).unwrap();
}

#[test]
fn tree_files_are_relative_sorted_and_files_only() {
    let d = scratch("tree");
    for f in ["b.js", "a/z.js", "a/b/c.js", "B.js", ".humanify/x.json"] {
        let p = d.join(f);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, f).unwrap();
    }
    std::fs::create_dir_all(d.join("empty")).unwrap();
    assert_eq!(
        tree_files(&d),
        vec![".humanify/x.json", "B.js", "a/b/c.js", "a/z.js", "b.js"]
    );
    assert!(tree_files(&d.join("missing")).is_empty());
    assert_eq!(
        dir_texts(&d, "a"),
        vec![
            ("a/b/c.js".to_string(), "a/b/c.js".to_string()),
            ("a/z.js".to_string(), "a/z.js".to_string())
        ]
    );
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn two_trees_differing_in_one_byte_or_one_file_fail_determinism() {
    let d = scratch("ident");
    let (a, b) = (d.join("a"), d.join("b"));
    for t in [&a, &b] {
        std::fs::create_dir_all(t).unwrap();
        std::fs::write(t.join("index.js"), "x").unwrap();
    }
    assert!(assert_identical_trees(&a, &b, "L").is_ok());
    std::fs::write(b.join("index.js"), "y").unwrap();
    let Err(Stop::Failure(m)) = assert_identical_trees(&a, &b, "L") else {
        panic!("a differing byte must fail");
    };
    assert_eq!(m, "L: index.js differs between two identical runs");
    std::fs::write(b.join("extra.js"), "").unwrap();
    let Err(Stop::Failure(m)) = assert_identical_trees(&a, &b, "L") else {
        panic!("a differing file set must fail");
    };
    assert_eq!(m, "L: the two runs wrote different file sets");
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn a_bun_cjs_build_is_known_by_its_first_line() {
    let d = scratch("buncjs");
    let f = d.join("index.js");
    for (first, want) in [
        ("// @bun @bun-cjs\n(function(){})", true),
        ("// @bun @bytecode @bun-cjs", true),
        ("// @bun\n", false),
        ("// @bunx @bun-cjs", false),
        ("// @bun @bun-cjsx", false),
        ("x\n// @bun @bun-cjs", false),
    ] {
        std::fs::write(&f, first).unwrap();
        assert_eq!(is_bun_cjs(&f), want, "{first:?}");
    }
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn the_next_release_prior_is_read_off_its_line() {
    let stderr = "x\nNext release: --prior-version /t/fresh\ny\n";
    assert!(matches!(next_release_prior(stderr, "L"), Ok(p) if p == "/t/fresh"));
    let Err(Stop::Failure(m)) = next_release_prior("nothing", "L") else {
        panic!("no line must fail");
    };
    assert_eq!(m, "L: the run named no prior for the next release");
}

#[test]
fn the_skip_line_names_its_kind_only_for_this_store() {
    let line =
        "no library index for esbuild bundles in /r/store — skipping library name matching (x)";
    assert_eq!(
        skip_kind(&format!("a\n{line}\n"), "/r/store"),
        Some("esbuild")
    );
    assert_eq!(skip_kind(line, "/other"), None);
    assert_eq!(
        skip_kind(
            "no library index for  bundles in /r/store — skipping library name matching (",
            "/r/store"
        ),
        None
    );
}

#[test]
fn the_launch_log_is_normalized_as_the_golden_stores_it() {
    let mut log = LaunchLog::default();
    let argv: Vec<String> = [
        "/repo/test/in.js",
        "-o",
        "/tmp/humanify-e2e-1/x/fresh",
        "--endpoint",
        "http://127.0.0.1:4242/v1",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    log.record("/repo/target/release/humanify-ag", "e2e", &argv)
        .unwrap();
    assert_eq!(
        normalize_launches(
            &log,
            Path::new("/tmp/humanify-e2e-1"),
            Path::new("/repo"),
            "4242"
        ),
        "<REPO>/target/release/humanify-ag HUMANIFY_API_KEY=e2e :: <REPO>/test/in.js -o \
         <TMP>/humanify-e2e-<RUN>/x/fresh --endpoint http://127.0.0.1:<PORT>/v1 \n"
    );
}

#[test]
fn a_tail_keeps_the_last_characters() {
    assert_eq!(tail("abcdef", 3), "def");
    assert_eq!(tail("ab", 3), "ab");
    assert_eq!(tail("ééé", 2), "éé");
}

#[test]
fn an_unsplit_leg_holding_a_runnable_tree_fails() {
    let d = scratch("unsplit");
    let legs = Legs {
        fresh: d.join("f"),
        prior: d.join("p"),
        sequential: d.join("s"),
    };
    assert!(check_unsplit("L", &legs).is_ok());
    std::fs::create_dir_all(d.join("p")).unwrap();
    std::fs::write(d.join("p/run.cjs"), "").unwrap();
    let Err(Stop::Failure(m)) = check_unsplit("L", &legs) else {
        panic!("a runnable tree must fail");
    };
    assert!(m.ends_with("p holds run.cjs but no runnable tree"), "{m}");
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn every_committed_fixture_pair_is_read() {
    let pairs = fixture_pairs(&crate::e2e::repo()).unwrap();
    assert!(pairs.len() >= 19, "{}", pairs.len());
    assert!(pairs.iter().any(|p| p.name == "esm-exports" && !p.bundle));
    assert!(pairs.iter().any(|p| p.name == "bun-bundle" && p.bundle));
}
