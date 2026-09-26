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
//! let (controller, context) = Controller::<!>::new();
//!
//! // set up a load of threads
//! let worker = thread::spawn(move || {
//!     let mut counter = 0;
//!     try {
//!         for _ in 0.. {
//!             counter += 1;
//!             assert_eq!(counter % 2, 1); // odd
//!             context.cancelled()?;
//!             counter += 1;
//!             assert_eq!(counter % 2, 0); // even
//!         };
//!         // always check for cancellation at end of try-block to avoid type errors
//!         context.cancelled()?
//!     };
//!     counter
//! });
//!
//! // cancel the workers when something happens
//! controller.cancel();
//!
//! // You still need to join your threads before you finish for soundness reasons
//! # thread::sleep(Duration::from_secs(1));
//! let count = worker.join().unwrap();
//! assert_eq!(count % 2, 1); // we exited mid loop
//! ```
//!
//! # Limitations
//!
//! - The enclosing `try` block must return `()`. In most cases where long-running loops need
//!   repeatedly to check for cancellation this shouldn't cause an issue as most significant IO
//!   writes to a `buf: &mut [u8]` and infinite loops return `!` which coerces to `()`.
//! - The `Context` which results from the try block is unusable and should not be assigned to a
//!   variable as it will not include the valid cancellation flag.

use std::{
    io::{self, ErrorKind},
    ops::{ControlFlow, FromResidual, Residual, Try},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use crossbeam_channel::{Receiver, RecvError, SendError, Sender, unbounded};

pub mod prelude {
    pub use super::{Context, Controller};
}

#[derive(Debug, Clone)]
pub struct Context<T> {
    cancelled: Option<Arc<AtomicBool>>,
    reply: Option<Sender<T>>,
}

/// A default [`Context`] will ignore any data sent via [`.reply()`][Self::reply] and is
/// uncancellable (calls to [`cancelled()?`][Self::cancelled] will never abort)
impl<T> Default for Context<T> {
    fn default() -> Self {
        Self {
            cancelled: None,
            reply: None,
        }
    }
}

impl<T> Context<T> {
    pub fn cancelled(&self) -> Cancellation {
        Cancellation {
            cancelled: self.cancelled.clone(),
        }
    }

    pub fn reply(&self, t: T) -> Result<(), SendError<T>> {
        match &self.reply {
            Some(tx_channel) => tx_channel.send(t),
            None => Ok(()),
        }
    }
}

pub struct Cancellation {
    cancelled: Option<Arc<AtomicBool>>,
}

#[derive(Debug, Clone)]
pub struct Controller<T> {
    cancelled: Arc<AtomicBool>,
    replies: Receiver<T>,
}

impl<T> Controller<T> {
    pub fn new() -> (Controller<T>, Context<T>) {
        let cancelled = Arc::from(AtomicBool::new(false));
        let (reply, replies) = unbounded::<T>();
        (
            Controller {
                cancelled: cancelled.clone(),
                replies,
            },
            Context {
                cancelled: Some(cancelled),
                reply: Some(reply),
            },
        )
    }

    pub fn receiver(&self) -> Receiver<T> {
        self.replies.clone()
    }

    pub fn replies(&self) -> Result<T, RecvError> {
        self.replies.recv()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

impl Try for Cancellation {
    type Output = Self;

    type Residual = Cancelled;

    fn from_output(output: Self::Output) -> Self {
        output
    }

    fn branch(self) -> ControlFlow<Self::Residual, Self::Output> {
        match self.cancelled {
            Some(flag) if flag.load(Ordering::Acquire) => {
                ControlFlow::Break(Cancelled { cancelled: flag })
            }
            _ => ControlFlow::Continue(self),
        }
    }
}

impl FromResidual for Cancellation {
    fn from_residual(residual: Cancelled) -> Self {
        Self {
            cancelled: Some(residual.cancelled),
        }
    }
}

impl<T, E: From<Cancelled>> FromResidual<Cancelled> for Result<T, E> {
    fn from_residual(residual: Cancelled) -> Self {
        Err(residual.into())
    }
}

impl From<Cancelled> for io::Error {
    fn from(_: Cancelled) -> Self {
        io::Error::new(
            ErrorKind::Interrupted,
            "thread cancellation requested by controller",
        )
    }
}

pub struct Cancelled {
    cancelled: Arc<AtomicBool>,
}

impl Residual<Cancellation> for Cancelled {
    type TryType = Cancellation;
}

#[cfg(test)]
mod tests {
    use std::{thread, time::Duration};

    use super::*;

    #[test]
    fn cancellation() {
        let (workerthreads, keepalive) = Controller::<!>::new();
        let worker = thread::Builder::new()
            .name("worker".to_string())
            .spawn(move || {
                try {
                    loop {
                        keepalive.cancelled()?;
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

    #[test]
    fn not_cancelled() {
        let (_workerthreads, keepalive) = Controller::<!>::new();
        let worker = thread::Builder::new()
            .name("worker".to_string())
            .spawn(move || {
                let mut counter = 0;
                try {
                    for _ in 0..5 {
                        counter += 1;
                        assert_eq!(counter % 2, 1); // odd
                        keepalive.cancelled()?;
                        counter += 1;
                        assert_eq!(counter % 2, 0); // even
                    }
                    // homogeneity requires calling `cancelled()?` WITHOUT `;` at end of `try`-block
                    // TODO: is there a way to help the compiler to hint this solution on type mismatch?
                    keepalive.cancelled()?
                };
                counter
            })
            .unwrap();
        let count = worker.join().unwrap();
        assert_eq!(count, 10);
    }

    #[test]
    fn reply() {
        let (workerthreads, keepalive) = Controller::<i32>::new();
        let _worker = thread::Builder::new()
            .name("worker".to_string())
            .spawn(move || {
                for i in 0..=5 {
                    keepalive.reply(i).unwrap();
                }
            })
            .unwrap();
        let mut sum = 0;
        while let Ok(n) = workerthreads.replies() {
            sum += n;
        }
        assert_eq!(sum, 15);
    }
}
