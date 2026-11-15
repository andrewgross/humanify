//! `cargo dev <verb>`: the measurement harness's binary (the verb table is
//! `humanify_ag_dev::cli`).

fn main() -> std::process::ExitCode {
    humanify_ag_dev::cli::main()
}
