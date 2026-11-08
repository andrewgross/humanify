//! `index build`: the dry run, every chunk at once, then one merge — each
//! a `humanify-ag index build` launch of the binary built from this
//! checkout; a failed chunk stops the merge.

use super::index::{IndexBuild, run};
use crate::runners::recording_test::{Commit, Rec, cargo_toml, sandbox_repo, write_exec};

fn build(rec: &Rec, chunks: u32) -> IndexBuild {
    let r = &rec.root;
    IndexBuild {
        spec: r.join("spec.toml"),
        registry: r.join("registry"),
        store: r.join("store"),
        work: r.join("work"),
        out: r.join("out"),
        chunks,
        jobs: 4,
        merge_jobs: 2,
    }
}

#[test]
fn the_dry_run_then_every_chunk_then_one_merge() {
    let rec = Rec::new("index");
    let repo = sandbox_repo(
        &rec,
        &[Commit {
            files: vec![(
                "crates/humanify-ag-cli/Cargo.toml",
                cargo_toml("humanify-ag-cli", "humanify-ag"),
            )],
            branch: None,
        }],
    );
    let ctx = rec.ctx(&repo);
    assert_eq!(run(&ctx, &build(&rec, 3)), 0, "{}", ctx.transcript());
    let calls = rec.normalise(&rec.calls(), &repo);
    let lines: Vec<&str> = calls.lines().collect();
    let common = "index build --spec <TMP>/spec.toml --registry <TMP>/registry --store <TMP>/store --work <TMP>/work";
    assert_eq!(
        lines[0],
        "cargo :: build --release --locked -p humanify-ag-cli (in <REPO>)"
    );
    assert!(
        lines[1].ends_with(&format!(":: {common} --dry-run ")),
        "{}",
        lines[1]
    );
    let mut chunks: Vec<&str> = lines[2..5].to_vec();
    chunks.sort();
    for (i, l) in chunks.iter().enumerate() {
        assert!(
            l.ends_with(&format!(":: {common} --jobs 4 --chunk {i}/3 --offline ")),
            "{l}"
        );
    }
    assert!(
        lines[5].ends_with(&format!(
            ":: {common} --merge-jobs 2 --merge-only --out <TMP>/out "
        )),
        "{}",
        lines[5]
    );
    assert!(ctx.transcript().contains("index written to"));
}

#[test]
fn a_failed_chunk_stops_the_merge() {
    let rec = Rec::new("index-fail");
    let failing = crate::runners::recording_test::BIN_RECORDER.replace(
        "cfg=\"\" out=\"\"",
        "[[ \"$*\" == *\"--chunk 1/2\"* ]] && exit 3\ncfg=\"\" out=\"\"",
    );
    write_exec(&rec.root.join("bin-recorder"), &failing);
    let repo = sandbox_repo(
        &rec,
        &[Commit {
            files: vec![(
                "crates/humanify-ag-cli/Cargo.toml",
                cargo_toml("humanify-ag-cli", "humanify-ag"),
            )],
            branch: None,
        }],
    );
    let ctx = rec.ctx(&repo);
    assert_eq!(run(&ctx, &build(&rec, 2)), 1);
    assert!(
        ctx.transcript()
            .contains("1 of 2 chunks failed (1); not merging"),
        "{}",
        ctx.transcript()
    );
    assert!(!rec.calls().contains("--merge-only"));
}

/// Two builds must not write one store (a shard's temp name is fixed, so
/// two writers of one shard collide) or one work dir: a second build on a
/// claimed store is refused at once, launching nothing — never queued.
#[test]
fn a_store_another_live_build_writes_is_refused() {
    let rec = Rec::new("index-claimed");
    let repo = sandbox_repo(
        &rec,
        &[Commit {
            files: vec![(
                "crates/humanify-ag-cli/Cargo.toml",
                cargo_toml("humanify-ag-cli", "humanify-ag"),
            )],
            branch: None,
        }],
    );
    let ctx = rec.ctx(&repo);
    let b = build(&rec, 2);
    let _other = crate::runners::claim::claim_folder(&b.store, "index build other-spec").unwrap();
    assert_eq!(run(&ctx, &b), 1);
    let said = ctx.transcript();
    assert!(said.contains("index build other-spec"), "{said}");
    assert!(
        !rec.calls().contains("index build --spec"),
        "nothing launched"
    );
}
