use super::*;

#[test]
fn an_edited_pair_splits_into_name_only_and_edited_lines() {
    let d = std::env::temp_dir().join(format!("dev-realledger-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    for (side, body) in [
        (
            "prior",
            "function compute(alpha, beta) {\n  const total = alpha + beta;\n  log(total);\n  return total;\n}\n",
        ),
        (
            "fresh",
            "function compute(alpha, beta) {\n  const total = alpha * beta;\n  note(total);\n  return total;\n}\n",
        ),
    ] {
        std::fs::create_dir_all(d.join(side)).unwrap();
        std::fs::write(d.join(side).join("a.js"), body).unwrap();
    }
    let text = real_ledger(&d.join("prior"), &d.join("fresh"), "t").unwrap();
    assert!(text.contains("ROW|t|real|4|2|2|0|0"), "{text}");
    assert!(text.contains("ROW|t|accounting|4|4|0|"), "{text}");
}
