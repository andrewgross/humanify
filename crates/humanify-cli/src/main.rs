//! Thin binary: clap `RunConfig`, the ONE env-reading module, wiring
//! (docs/rust-port/02-rust-target-architecture.md §2). Scaffold only until
//! WPB.4; exits 1 so no harness can mistake it for a working pipeline.

fn main() {
    eprintln!(
        "humanify (rust): scaffold only — the TypeScript pipeline is production until phase 6"
    );
    std::process::exit(1);
}
