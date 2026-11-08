//! `cargo dev stub-llm serve [--port N]`: the ONE stub LLM
//! (humanify-ag-testkit, answering `<id>Renamed` and module-file names)
//! as a process, for by-hand runs (successor of running
//! scripts/lib/stub-llm.ts by hand). Prints the `--endpoint` base URL,
//! then serves until its stdin closes or it is killed — blocking on the
//! stdin read, never a sleep loop.

use std::io::{Read, Write};
use std::time::Duration;

use humanify_ag_testkit::answers;
use humanify_ag_testkit::stub_server::{StubResponse, StubServer};

use crate::HostEnv;
use crate::settings::Parsed;

/// The stub on 127.0.0.1: `port`, or a free one.
pub fn start(port: Option<u64>) -> Result<StubServer, String> {
    let port = match port {
        None => 0,
        Some(p) => u16::try_from(p).map_err(|_| format!("--port {p} is not a port (1-65535)"))?,
    };
    StubServer::serve(
        port,
        Duration::ZERO,
        std::sync::Arc::new(|_, body| {
            StubResponse::ok(answers::completion(Some(&answers::renamed(body))))
        }),
    )
}

/// Read `input` to its end (EOF or an error): the serve verb's lifetime.
pub fn block_until_closed(input: &mut dyn Read) {
    let mut sink = [0u8; 4096];
    while matches!(input.read(&mut sink), Ok(n) if n > 0) {}
}

/// Run the verb.
pub fn run(p: &Parsed, _host: &HostEnv) -> (u8, String, String) {
    let server = match start(p.settings.int("dev.port")) {
        Ok(s) => s,
        Err(e) => return (1, String::new(), format!("stub-llm serve: {e}\n")),
    };
    // Printed at once (the verb's return value prints only at exit).
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", server.base_url);
    let _ = out.flush();
    eprintln!("stub-llm: serving until stdin closes (Ctrl-D) or the process is killed");
    block_until_closed(&mut std::io::stdin());
    (0, String::new(), String::new())
}

#[cfg(test)]
#[path = "stub_llm_test.rs"]
mod stub_llm_test;
