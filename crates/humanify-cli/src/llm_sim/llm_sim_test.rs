use humanify_model::llm::{
    BatchRenameRequest, BatchRenameResponse, LlmCall, LlmError, NameProvider, Renames,
};

use super::{LatencySim, latency_ms};

struct Echo;

impl NameProvider for Echo {
    fn run_wave(&self, calls: Vec<LlmCall>) -> Vec<Result<BatchRenameResponse, LlmError>> {
        calls
            .iter()
            .map(|_| {
                Ok(BatchRenameResponse {
                    renames: Renames::from_entries(Vec::new()),
                    finish_reason: None,
                    usage: None,
                })
            })
            .collect()
    }
}

fn call(i: usize) -> LlmCall {
    LlmCall {
        request: BatchRenameRequest::default(),
        system_prompt: "You are a test".into(),
        user_prompt: format!("call {i}"),
    }
}

#[test]
fn a_wave_costs_its_makespan_on_the_slots() {
    let sim = LatencySim::new(&Echo, 2);
    let calls: Vec<LlmCall> = (0..3).map(call).collect();
    let l: Vec<f64> = calls.iter().map(latency_ms).collect();
    sim.run_wave(calls);
    let r = sim.report();
    // Two slots: calls 0 and 1 start at 0, call 2 on the first to free.
    let expect = (l[0].min(l[1]) + l[2]).max(l[0].max(l[1]));
    assert!(
        (r.wall_ms - expect).abs() < 1e-6,
        "{} vs {expect}",
        r.wall_ms
    );
    assert_eq!(r.calls, 3);
}

#[test]
fn pipelining_beats_the_turn_barrier_on_the_same_chains() {
    // Four chains of three calls each: the barrier pays the max of every
    // turn, the pipeline pays the longest chain.
    let chain = |c: usize, k: usize| call(c * 10 + k);
    let barrier = LatencySim::new(&Echo, 32);
    for k in 0..3 {
        barrier.run_wave((0..4).map(|c| chain(c, k)).collect());
    }
    let pipe = LatencySim::new(&Echo, 32);
    let mut step = [0usize; 4];
    pipe.run_pipelined((0..4).map(|c| (c, chain(c, 0))).collect(), &mut |id, _| {
        step[id] += 1;
        if step[id] < 3 {
            vec![(id, chain(id, step[id]))]
        } else {
            Vec::new()
        }
    });
    let (b, p) = (barrier.report().wall_ms, pipe.report().wall_ms);
    assert!(p <= b + 1e-6, "pipelined {p} > barrier {b}");
    let longest = (0..4)
        .map(|c| (0..3).map(|k| latency_ms(&chain(c, k))).sum::<f64>())
        .fold(0.0, f64::max);
    assert!((p - longest).abs() < 1e-6, "{p} vs longest chain {longest}");
}

#[test]
fn the_draw_is_deterministic_and_in_range() {
    let a = latency_ms(&call(7));
    assert_eq!(a, latency_ms(&call(7)));
    assert!((105.0..=20339.0).contains(&a));
}
