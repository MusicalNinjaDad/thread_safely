#![cfg_attr(unstable_try_blocks, feature(try_blocks))]
#![cfg_attr(unstable_try_trait_v2, feature(try_trait_v2))]
#![cfg_attr(unstable_try_trait_v2_residual, feature(try_trait_v2_residual))]
//! Provides a sound way to allow for long-running threads to be cancelled without resorting
//! to extreme measures. Primarily designed to be used with long-running loops / chunked IO.
//!
//! # Usage
//!
//! ## Binaries
//!
//! ```
//! #![cfg_attr(unstable_try_blocks, feature(try_blocks))]
//! # use std::time::Duration;
//! use std::thread;
//! use thread_safely::prelude::*;
//! // Set up a [Controller] and (clonable) [Context]
//! let (workerthreads, keepalive): (Controller, Context) = Controller::new();
//!
//! // set up a load of threads
//! let worker = thread::spawn(move || {
//!     let mut counter = 0;
//!     try {
//!         for _ in 0.. {
//!             counter += 1;
//!             assert_eq!(counter % 2, 1); // odd
//!             keepalive.clone()?;
//!             counter += 1;
//!             assert_eq!(counter % 2, 0); // even
//!         };
//!     };
//!     counter
//! });
//!
//! // cancel the workers when something happens
//! workerthreads.cancel();
//!
//! // You still need to join your threads before you finish for soundness reasons
//! # thread::sleep(Duration::from_secs(1));
//! let count = worker.join().unwrap();
//! assert_eq!(count % 2, 1); // we exited mid loop
//! ```
//!
//! # Limitations
//!
//! The enclosing `try` block must return `()`. In most cases where long-running loops need
//! repeatedly to check for cancellation this shouldn't cause an issue as most significant IO
//! writes to a `buf: &mut [u8]` and infinite loops return `!` which coerces to `()`

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

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

impl Try for Context {
    type Output = ();

    type Residual = Cancelled;

    fn from_output(_output: Self::Output) -> Self {
        unimplemented!("from_output")
    }

    fn branch(self) -> ControlFlow<Self::Residual, Self::Output> {
        match self.cancelled.load(Ordering::Acquire) {
            true => ControlFlow::Break(Cancelled {
                cancelled: self.cancelled,
            }),
            false => ControlFlow::Continue(()),
        }
    }
}

impl FromResidual for Context {
    fn from_residual(residual: Cancelled) -> Self {
        Self {
            cancelled: residual.cancelled,
        }
    }
}

pub struct Cancelled {
    cancelled: Arc<AtomicBool>,
}

impl Residual<()> for Cancelled {
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
        workerthreads.cancel();
        thread::sleep(Duration::from_secs(1));
        assert!(worker.is_finished());
        let work = worker.join().unwrap();
        assert!(work);
    }
}
