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

use std::{
    ops::{ControlFlow, FromResidual, Residual, Try},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

pub mod prelude {
    pub use super::{Context, Controller};
}

#[derive(Debug, Clone)]
pub struct Context {
    cancelled: Arc<AtomicBool>,
}

#[derive(Debug, Clone)]
pub struct Controller {
    cancelled: Arc<AtomicBool>,
}

impl Controller {
    pub fn new() -> (Controller, Context) {
        let cancelled = Arc::from(AtomicBool::new(false));
        (
            Controller {
                cancelled: cancelled.clone(),
            },
            Context { cancelled },
        )
    }

    pub fn cancel(&self) {}
}

impl Try for Context {
    type Output = Self;

    type Residual = Cancelled;

    fn from_output(output: Self::Output) -> Self {
        todo!("from output")
    }

    fn branch(self) -> ControlFlow<Self::Residual, Self::Output> {
        match self.cancelled.load(Ordering::Acquire) {
            true => ControlFlow::Break(Cancelled),
            false => ControlFlow::Continue(self),
        }
    }
}

impl FromResidual for Context {
    fn from_residual(residual: <Self as Try>::Residual) -> Self {
        todo!("residual")
    }
}

pub struct Cancelled;

impl Residual<Context> for Cancelled {
    type TryType = Context;
}

#[cfg(test)]
mod tests {
    use std::{thread, time::Duration};

    use super::*;

    #[test]
    fn cancellation() {
        let (workerthreads, keepalive) = Controller::new();
        let worker = thread::Builder::new()
            .name("worker".to_string())
            .spawn(move || {
                try {
                    loop {
                        keepalive.clone()?;
                    }
                };
                true
            })
            .unwrap();
        let asserter = thread::Builder::new()
            .name("asserter".to_string())
            .spawn(move || {
                let work = worker.join().unwrap();
                assert!(work);
            })
            .unwrap();
        workerthreads.cancel();
        thread::sleep(Duration::from_secs(1));
        assert!(asserter.is_finished());
    }
}
