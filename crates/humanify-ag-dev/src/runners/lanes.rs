//! The ordered transcript of a run whose work goes in LANES at once
//! (`eval score --parallel-pairs N`, exp207): one SLOT per lane, in
//! canonical order (a pair's slot is its place in pairs.json; the
//! self-hop's is after every pair's). A line said into the HEAD slot (the
//! earliest slot not yet finished) is printed at once; a line said into a
//! later slot waits until every slot before it has finished, then prints
//! in its slot's order. So the transcript reads in canonical order however
//! the lanes finish (completion order reaches nothing, docs/concurrency.md
//! rule 6), and with one lane it is printed exactly as it is said.

use std::sync::Mutex;

struct State {
    /// The earliest slot not yet finished.
    head: usize,
    finished: Vec<bool>,
    /// Lines said into a slot behind the head, not printed yet.
    waiting: Vec<Vec<String>>,
    /// Every line printed, in print order (= canonical order).
    printed: Vec<String>,
}

/// The ordered sink. Its lock is held across the println of the lines it
/// releases, so two lanes' lines never interleave (an observer, like the
/// pipeline's log lock: it only orders what is said, it decides nothing).
pub struct Lanes {
    echo: bool,
    state: Mutex<State>,
}

impl Lanes {
    pub fn new(slots: usize, echo: bool) -> Lanes {
        Lanes {
            echo,
            // concurrency: a lane transcript's ordering state, held for one
            // push or one release of finished slots' lines (their println).
            state: Mutex::new(State {
                head: 0,
                finished: vec![false; slots],
                waiting: vec![Vec::new(); slots],
                printed: Vec::new(),
            }),
        }
    }

    fn emit(&self, st: &mut State, line: String) {
        if self.echo {
            println!("{line}");
        }
        st.printed.push(line);
    }

    /// Say `line` into `slot`.
    pub fn say(&self, slot: usize, line: String) {
        let mut st = self.state.lock().expect("the lane transcript");
        // a slot behind the head is already released: never lose a line
        if slot <= st.head {
            self.emit(&mut st, line);
        } else {
            st.waiting[slot].push(line);
        }
    }

    /// `slot` has said everything it will: release every slot behind it
    /// that is now at the head. Finishing a slot twice is harmless.
    pub fn finish(&self, slot: usize) {
        let mut st = self.state.lock().expect("the lane transcript");
        if let Some(f) = st.finished.get_mut(slot) {
            *f = true;
        }
        while st.head < st.finished.len() && st.finished[st.head] {
            st.head += 1;
            if st.head < st.finished.len() {
                let head = st.head;
                let lines = std::mem::take(&mut st.waiting[head]);
                for l in lines {
                    self.emit(&mut st, l);
                }
            }
        }
    }

    /// Finish every slot (the run is over) and hand back every line, in
    /// canonical order.
    pub fn close(&self) -> Vec<String> {
        let n = self
            .state
            .lock()
            .expect("the lane transcript")
            .finished
            .len();
        for slot in 0..n {
            self.finish(slot);
        }
        std::mem::take(&mut self.state.lock().expect("the lane transcript").printed)
    }
}

#[cfg(test)]
#[path = "lanes_test.rs"]
mod lanes_test;
