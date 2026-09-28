//! Native tests for the naming passes' re-ask policy. The WP4.4/4.5
//! TS-harvested corpus (test/parity/wp445-fixtures.json — every top-level
//! call the passes' own TS unit tests make, replayed against the Rust
//! port) was retired 2026-09-28 with the other TS-capture replays; the
//! re-ask policy it exercised end to end through the sweep is pinned here
//! natively, and the reconcile/permutation units keep their own module
//! tests.

use humanify_model::llm::{BatchRenameResponse, LlmCall, LlmError, NameProvider, Renames};
use oxc_allocator::Allocator;

use super::sweep::sweep_minted_names;
use crate::ingest::Ingest;
use crate::modules::soundness::collect_eval_with_taint;
use crate::naming::waves::render::render_program;
use crate::rename::eligibility::Eligibility;
use crate::rename::validated::RenameState;
use crate::trail::Anchor;

/// The coverage-sweep collision retry (2026-09-28 fix): a sweep suggestion
/// rejected for a name-collision class (`target-in-scope` here — the
/// sibling `used` holds the name) gets ONE re-ask that DISCLOSES the
/// previous suggestion, and the re-asked suggestion applies through the
/// same validation. Before the fix this exact shape of sweep recorded
/// the drop — the case the retired wp445 corpus (kind "sweep") carried:
/// `Kq_` kept its minted name forever.
#[test]
fn a_sweep_collision_gets_one_disclosed_reask() {
    let text = "function f() {\n  var used = one();\n  var Kq_ = two();\n  return used + Kq_;\n}";
    let eligible = Eligibility::new(Some("bun"), Some("bun"));
    let allocator = Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, text);
    let semantic = ingest.semantic();
    let taint = collect_eval_with_taint(semantic);
    let params = humanify_model::llm::CacheKeyParams::default();

    // First ask: decline `f`, suggest the sibling's name for `Kq_` (the
    // collision). Re-ask: a fresh name — and the re-ask's prompt must
    // disclose the collision.
    struct CollisionSweep {
        asks: std::cell::Cell<usize>,
    }
    impl NameProvider for CollisionSweep {
        fn run_wave(&self, calls: Vec<LlmCall>) -> Vec<Result<BatchRenameResponse, LlmError>> {
            calls
                .into_iter()
                .map(|c| {
                    let n = self.asks.get();
                    self.asks.set(n + 1);
                    if n > 0 {
                        assert_eq!(c.request.identifiers, ["Kq_"], "only the rejected id re-asks");
                        assert!(
                            c.user_prompt.contains(
                                "\"Kq_\" was suggested as \"used\" but that conflicts with an existing name"
                            ),
                            "the re-ask discloses the collision: {}",
                            c.user_prompt
                        );
                        assert!(
                            c.user_prompt.contains("DO NOT suggest these names: used"),
                            "the re-ask blocklists the rejected name: {}",
                            c.user_prompt
                        );
                    }
                    let suggestion = if n == 0 { "used" } else { "usedRunner" };
                    Ok(BatchRenameResponse {
                        renames: Renames::from_entries(vec![(
                            "Kq_".to_string(),
                            Some(suggestion.to_string()),
                        )]),
                        finish_reason: None,
                        usage: None,
                    })
                })
                .collect()
        }
    }

    let provider = CollisionSweep {
        asks: std::cell::Cell::new(0),
    };
    let mut state = RenameState::new(semantic, Anchor::Fresh);
    let r = sweep_minted_names(semantic, &mut state, &eligible, &taint, &provider, &params);
    assert_eq!(provider.asks.get(), 2, "exactly one re-ask");
    assert_eq!(r.reasked, 1, "the retry is recorded");
    assert_eq!(r.named, 1, "the re-asked suggestion applied");
    assert_eq!(r.skipped, 1, "the declined `f` stays skipped");
    let code = render_program(semantic, &state);
    assert!(
        code.contains("var usedRunner = two();"),
        "the retried name landed: {code}"
    );

    // Bounded: answer the SAME colliding name twice and the sweep gives up
    // after the ONE re-ask — never a loop, and the give-up is recorded.
    struct Stubborn {
        asks: std::cell::Cell<usize>,
    }
    impl NameProvider for Stubborn {
        fn run_wave(&self, calls: Vec<LlmCall>) -> Vec<Result<BatchRenameResponse, LlmError>> {
            self.asks.set(self.asks.get() + calls.len());
            calls
                .into_iter()
                .map(|_| {
                    Ok(BatchRenameResponse {
                        renames: Renames::from_entries(vec![(
                            "Kq_".to_string(),
                            Some("used".to_string()),
                        )]),
                        finish_reason: None,
                        usage: None,
                    })
                })
                .collect()
        }
    }
    let stubborn = Stubborn {
        asks: std::cell::Cell::new(0),
    };
    let mut state = RenameState::new(semantic, Anchor::Fresh);
    let r = sweep_minted_names(semantic, &mut state, &eligible, &taint, &stubborn, &params);
    assert_eq!(stubborn.asks.get(), 2, "one re-ask, then give up");
    assert_eq!(r.reasked, 1);
    assert_eq!(r.named, 0);
    assert_eq!(r.reask_dropped, 1, "the give-up is recorded");
    assert_eq!(render_program(semantic, &state), text, "nothing applied");
}
