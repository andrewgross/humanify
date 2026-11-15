use super::*;

/// Scorer 16 scores exactly as scorer 15 and writes plain JSON: the
/// summary's `bars` block for one passing set.
#[test]
fn scorer_16_writes_the_bars_block_in_plain_json() {
    assert_eq!(SCORER_VERSION, "exp096-score/16");
    let set = dict(vec![kv("appClaimedBytes", int(0))]);
    let agg = vec![("bun-dev-bun".to_string(), set, 0.999)];
    assert_eq!(
        crate::json::text(&bars(&agg)),
        "{\n  \"appBarPassed\": true,\n  \"appClaimedBytes\": 0,\n  \"precisionBar\": 0.998,\n  \"precisionBarPassed\": {\n    \"bun-dev-bun\": true\n  }\n}\n"
    );
}
