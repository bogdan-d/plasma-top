#![deny(unsafe_code)]
#![warn(
    missing_docs,
    unreachable_pub,
    unused_lifetimes,
    unused_macro_rules,
    unused_qualifications
)]
#![deny(clippy::correctness, clippy::suspicious)]
#![warn(clippy::style, clippy::complexity, clippy::perf)]
#![deny(
    clippy::dbg_macro,
    clippy::todo,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::unimplemented
)]

//! PlasmaTop process entry point.

use std::process::ExitCode;

fn main() -> ExitCode {
    match plasma_top::run(std::env::args_os()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(plasma_top::Error::Cli(error)) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
