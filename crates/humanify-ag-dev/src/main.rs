//! `cargo dev <verb>`: the measurement harness's binary (the verb table is
//! `humanify_ag_dev::cli`; the command line is read here, its one owner).

fn main() -> std::process::ExitCode {
    humanify_ag_dev::cli::main(&humanify_ag_dev::env::user_args())
}
