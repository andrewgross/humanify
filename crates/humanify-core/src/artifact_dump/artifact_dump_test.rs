//! The artifact dump writer's red tests.

use humanify_model::llm::CacheKeyParams;

use super::dispatch_rows;

/// `writePrompts` / `writeCacheKeys` join the rows with "\n" and add a
/// trailing "\n" — so a run that dispatched nothing writes ONE newline.
#[test]
fn no_dispatches_write_one_newline() {
    let params = CacheKeyParams {
        model: "m".into(),
        temperature: Some(0.0),
        max_tokens: None,
        reasoning_effort: None,
    };
    let (prompts, keys) = dispatch_rows(&[], &params);
    assert_eq!((prompts.as_str(), keys.as_str()), ("\n", "\n"));
}
