//! The seeded-shuffled fake name provider. The real provider
//! (humanify-ag-llm `provider.rs`) completes pipelined calls OUT OF ORDER;
//! the trait's default `run_pipelined` hands results back in id order, so
//! a test fake that inherits it never exercises the order the pipeline
//! must be immune to (docs/concurrency.md rule 6). This fake completes
//! in-flight calls in a seeded random order — same seed, same order — so
//! a test can run one input under several seeds and require the same
//! output.

use std::cell::{Cell, RefCell};

use humanify_ag_model::llm::{
    BatchRenameResponse, LlmCall, LlmError, NameProvider, OnCallDone, Renames,
};

/// The answer to one call.
pub type Answer = Box<dyn Fn(&LlmCall) -> Result<BatchRenameResponse, LlmError>>;

/// A fake provider answering from `answer`, completing pipelined calls in
/// the order a seeded xorshift draws.
pub struct ShuffledFake {
    answer: Answer,
    state: Cell<u64>,
    /// The ids of completed calls, in completion order.
    completed: RefCell<Vec<usize>>,
}

/// Every identifier of the request renamed to `<id>Renamed` (the stub
/// server's default policy, as a provider).
pub fn renamed(call: &LlmCall) -> Result<BatchRenameResponse, LlmError> {
    Ok(BatchRenameResponse {
        renames: Renames::from_entries(
            call.request
                .identifiers
                .iter()
                .map(|id| (id.clone(), Some(format!("{id}Renamed")))),
        ),
        finish_reason: Some("stop".to_string()),
        usage: None,
    })
}

impl ShuffledFake {
    pub fn new(seed: u64, answer: Answer) -> ShuffledFake {
        ShuffledFake {
            answer,
            // xorshift has one fixed point, zero.
            state: Cell::new(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1),
            completed: RefCell::new(Vec::new()),
        }
    }

    /// The `<id>Renamed` fake.
    pub fn renaming(seed: u64) -> ShuffledFake {
        ShuffledFake::new(seed, Box::new(renamed))
    }

    /// The ids of every call `run_pipelined` completed, in order.
    pub fn completion_order(&self) -> Vec<usize> {
        self.completed.borrow().clone()
    }

    /// xorshift64*.
    fn next(&self) -> u64 {
        let mut x = self.state.get();
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state.set(x);
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

impl NameProvider for ShuffledFake {
    /// One result per call, in call order (the trait's contract).
    fn run_wave(&self, calls: Vec<LlmCall>) -> Vec<Result<BatchRenameResponse, LlmError>> {
        calls.iter().map(|c| (self.answer)(c)).collect()
    }

    /// Every in-flight call may finish next: one is drawn, answered and
    /// handed to `on_done`, and its follow-ups join the in-flight set.
    fn run_pipelined(&self, initial: Vec<(usize, LlmCall)>, on_done: &mut OnCallDone<'_>) {
        let mut in_flight = initial;
        while !in_flight.is_empty() {
            let pick = (self.next() % in_flight.len() as u64) as usize;
            let (id, call) = in_flight.swap_remove(pick);
            let result = (self.answer)(&call);
            self.completed.borrow_mut().push(id);
            in_flight.extend(on_done(id, result));
        }
    }
}
