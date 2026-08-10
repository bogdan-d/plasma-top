//! Deterministic metric scheduling policy.

mod machine;
mod model;

pub(crate) use machine::Scheduler;
pub(crate) use model::*;

#[cfg(test)]
mod tests;
