use super::*;

#[test]
fn every_listed_shim_is_tracked_and_parses() {
    let repo = crate::paths::repo_root();
    let (code, out, err) = check(&repo, SHIMS, &HostEnv::default());
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("scripts/webcrack-shim.ts"), "{out}");
}

#[test]
fn a_shim_that_does_not_parse_or_is_missing_fails() {
    let dir = std::env::temp_dir().join(format!("humanify-shims-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("bad.ts"), "const x: number = ;\n").unwrap();
    std::fs::write(dir.join("good.mjs"), "export const x = 1;\n").unwrap();
    // An ES-module TypeScript file: `node --check` alone passes it green
    // whatever it holds (found by the planted red, 2026-11-07).
    std::fs::write(
        dir.join("esm.ts"),
        "import fs from \"node:fs\";\nconst x: number = ;\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("esm-ok.ts"),
        "import fs from \"node:fs\";\nexport const x: number = 1;\n",
    )
    .unwrap();
    let (code, _, err) = check(&dir, &["esm-ok.ts"], &HostEnv::default());
    assert_eq!(code, 0, "{err}");
    let (code, _, err) = check(&dir, &["esm.ts"], &HostEnv::default());
    assert_eq!(code, 1);
    assert!(err.contains("esm.ts: does not parse"), "{err}");
    let (code, _, err) = check(&dir, &["good.mjs", "bad.ts"], &HostEnv::default());
    assert_eq!(code, 1);
    assert!(err.contains("bad.ts"), "{err}");
    let (code, _, err) = check(&dir, &["gone.ts"], &HostEnv::default());
    assert_eq!(code, 1);
    assert!(err.contains("gone.ts: missing"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}
