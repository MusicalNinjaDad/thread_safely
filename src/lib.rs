#![cfg_attr(unstable_try_blocks, feature(try_blocks))]
#![cfg_attr(unstable_try_trait_v2, feature(try_trait_v2))]
#![cfg_attr(unstable_try_trait_v2_residual, feature(try_trait_v2_residual))]
//! Provides a sound way to allow for long-running threads to be cancelled without resorting
//! to extreme measures
//!
//! # Usage
//!
//! ## Binaries
//!
//! ```
//! use thread_safely::prelude::*;
//! // Set up a [Controller] and (clonable) [Context]
//! let (workerthreads, keepalive): (Controller, Context) = Controller::new();
//!
//! // set up a load of threads
//! // ...
//!
//! // cancel the workers when something happens
//! workerthreads.cancel();
//!
//! // You still need to join your threads before you finish
//!
//! ```

use std::ops::{FromResidual, Residual, Try};

pub mod prelude {
    pub use super::{Context, Controller};
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Context {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Controller {}

impl Controller {
    pub fn new() -> (Controller, Context) {
        (Controller {}, Context {})
    }

    pub fn cancel() {}
}

impl Try for Context {
    type Output = ();

    type Residual = Cancelled;

    fn from_output(output: Self::Output) -> Self {
        todo!()
    }

    fn branch(self) -> std::ops::ControlFlow<Self::Residual, Self::Output> {
        todo!()
    }
}

impl FromResidual for Context {
    fn from_residual(residual: <Self as Try>::Residual) -> Self {
        todo!()
    }
}

pub struct Cancelled;

impl Residual<()> for Cancelled {
    type TryType = Context;
}

#[cfg(test)]
mod tests {
    use std::thread;

    use super::*;

    #[test]
    fn cancellation() {
        let (workerthreads, keepalive) = Controller::new();
        let _worker = thread::spawn(move || try {
            loop {
                keepalive?
            }
        });
    }
}
