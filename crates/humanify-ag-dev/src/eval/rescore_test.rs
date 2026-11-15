use super::*;
use crate::identity::{F, Identity};

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("rescore-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn write(d: &Path, name: &str, text: &str) {
    std::fs::write(d.join(name), text).unwrap();
}

/// Records of one pair as `eval score` writes them (no input on disk).
fn records(d: &Path, v: &str, pair: &str, scorer: u64) {
    write(
        d,
        &format!("{v}.json"),
        &format!(r#"{{"scorerVersion":{scorer},"pair":"{pair}"}}"#),
    );
    write(
        d,
        &format!("{v}-run.json"),
        &format!(
            r#"{{"pair":"{pair}","config":{{"model":"m","resolved":{{"library":{{"matching":"off"}}}}}}}}"#
        ),
    );
}

/// The re-scored label gets an identity beside every new card: the old
/// label's own record with the scorer version this scorer wrote, or —
/// for a label scored before identities — one derived from its records,
/// said out loud.
#[test]
fn rescore_writes_an_identity_beside_every_card() {
    let (cards, out) = (dir("cards"), dir("out"));
    for d in [&cards, &out] {
        write(d, "commit.txt", "abc1234\n");
        write(d, "pipeline.json", r#"{"namingMode":"off"}"#);
    }
    records(&cards, "2.1.86", "2.1.85->2.1.86", 2);
    records(&cards, "2.1.119", "2.1.118->2.1.119", 2);
    records(&out, "2.1.86", "2.1.85->2.1.86", 3);
    records(&out, "2.1.119", "2.1.118->2.1.119", 3);
    let mut old = crate::identity::derive::eval_label(&cards).unwrap();
    old.retain(|i| i.corpus.entry == "2.1.85->2.1.86");
    old[0].llm = F::unknown("a field only the old record knows");
    write(&cards, "2.1.86.identity.json", &old[0].to_text());
    let said = identities(&cards, &out).unwrap();
    let read = |v: &str| {
        Identity::from_text(
            &std::fs::read_to_string(out.join(format!("{v}.identity.json"))).unwrap(),
        )
        .unwrap()
    };
    let kept = read("2.1.86");
    assert_eq!(kept.scorer.version, F::Known("3".into()));
    assert_eq!(kept.llm, old[0].llm, "the old record is the source");
    let derived = read("2.1.119");
    assert_eq!(derived.scorer.version, F::Known("3".into()));
    assert_eq!(derived.naming_mode, F::Known(Some("off".into())));
    assert!(
        said.iter()
            .any(|l| l.contains("2.1.119") && l.contains("derived")),
        "{said:?}"
    );
}
