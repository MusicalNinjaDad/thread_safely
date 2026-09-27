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
//!         // always check for cancellation at end of try-block
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
//! # Libraries
//!
//! ```
//! #![cfg_attr(unstable_try_blocks, feature(try_blocks))]
//! // Libraries usually won't need a Controller
//! use thread_safely::Context;
//!
//! pub struct Thing {
//!     // all the stuff we need
//!     data: usize,
//!     // No need to store the context in an Option, just use Default
//!     // Our example reply channel accepts a f32 for %-completion
//!     cx: Context<f32>,
//! }
//!
//! impl Thing {
//!     // new constructor doesn't require a Context.
//!     // The default context is designed to produce no-ops
//!     fn new(data: usize) -> Self {
//!         Self { data, cx: Context::default() }
//!     }
//!
//!     // dedicated constructor for those who wish to use a Context
//!     fn with_context(data: usize, cx: Context<f32>) -> Self {
//!         // assuming new has a load of logic we don't want to reproduce here
//!         let mut this = Self::new(data);
//!         this.add_context(cx);
//!         this
//!     }
//!
//!     // may as well allow users to add a Context to an existing Thing,
//!     // that way they can construct via `From` etc if they want or pass on the
//!     // optionality of using Contexts
//!     fn add_context(&mut self, cx: Context::<f32>) {
//!         self.cx = cx;
//!         // if you have a chaining API, then you can return
//!         // self
//!     }
//!
//!     // Check the context in long-running functions
//!     fn work(self, repetitions: usize) -> Option<usize> {
//!         let mut product = 0;
//!         let mut needs_cleaning = true;
//!
//!         // We can't exit the function immediately without clean-up,
//!         // so wrap the loop in a try-block
//!         try {
//!             for i in 0..repetitions {
//!                 // don't do work if we've been cancelled
//!                 self.cx.cancelled()?;
//!                 // do some blocking work
//!                 product += self.data;
//!                 // let them know how we are progressing
//!                 self.cx.reply(i as f32 / repetitions as f32);
//!             };
//!             
//!             // check for cancellation after last loop before doing anything else ...
//!             self.cx.cancelled()?;
//!
//!             // a few final steps, which we should skip if cancelled
//!             debug_assert_eq!(product, self.data * repetitions);
//!
//!             // final check to please the compiler (see Context::cancelled for details)
//!             self.cx.cancelled()? // no `;`
//!         };
//!
//!         // perform the mandatory clean up
//!         needs_cleaning = false;
//!         // now we can exit the function safely if cancelled
//!         self.cx.cancelled()?;
//!         // otherwise we return
//!         Some(product)
//!     }
//! }
//! ```
//!
//! # Limitations
//!
//! - The enclosing `try` block must return `()`. In most cases where long-running loops need
//!   repeatedly to check for cancellation this shouldn't cause an issue as most significant IO
//!   writes to a `buf: &mut [u8]` and infinite loops return `!` which coerces to `()`.

use std::{
    hint::cold_path,
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

#[derive(Debug)]
pub struct Context<R> {
    cancelled: Option<Arc<AtomicBool>>,
    reply: Option<Sender<R>>,
}

impl<R> Clone for Context<R> {
    fn clone(&self) -> Self {
        Self {
            cancelled: self.cancelled.clone(),
            reply: self.reply.clone(),
        }
    }
}

/// A default [`Context`] will ignore any data sent via [`.reply()`][Self::reply] and is
/// uncancellable (calls to [`cancelled()?`][Self::cancelled] will never abort)
impl<R> Default for Context<R> {
    fn default() -> Self {
        Self {
            cancelled: None,
            reply: None,
        }
    }
}

impl<R> Context<R> {
    #[inline]
    /// # IMPORTANT - avoiding type mismatch error [E0271]
    ///
    /// When used inside a `try { for { ... } }` loop, always check for cancellation
    /// at the end of the try block and leave off a semi-colon.
    ///
    /// This is both deliberate good practice, to ensure cancellation occurs if requested
    /// AND avoids a compiler error.
    ///
    /// Without this final check you will receive a compiler error.
    ///
    /// **To avoid**
    ///
    /// ```text
    ///     error[E0271]: type mismatch resolving `<Context<_> as Try>::Output == ()`
    /// ```
    ///
    /// **do this**
    ///
    /// ```ignore snippet
    ///     try {
    ///         for chunk in work {
    ///             cx.cancelled()?;
    ///             ... do some work ...
    ///             cx.cancelled()?;
    ///             ... do some more work ...
    ///         };
    ///         cx.cancelled()? // <- NO `;` - the try block returns a clone of the context
    ///     }
    /// ```
    pub fn cancelled(&self) -> Self {
        self.clone()
    }

    pub fn reply(&self, reply: R) -> Result<(), SendError<R>> {
        match &self.reply {
            Some(tx_channel) => tx_channel.send(reply),
            None => Ok(()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Controller<R> {
    cancelled: Arc<AtomicBool>,
    replies: Receiver<R>,
}

impl<R> Controller<R> {
    pub fn new() -> (Controller<R>, Context<R>) {
        let cancelled = Arc::from(AtomicBool::new(false));
        let (reply, replies) = unbounded::<R>();
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

    pub fn receiver(&self) -> Receiver<R> {
        self.replies.clone()
    }

    pub fn replies(&self) -> Result<R, RecvError> {
        self.replies.recv()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

impl<R> Try for Context<R> {
    type Output = Self;

    type Residual = Self;

    #[inline]
    fn from_output(output: Self::Output) -> Self {
        output
    }

    #[inline]
    fn branch(self) -> ControlFlow<Self::Residual, Self::Output> {
        match &self.cancelled {
            Some(flag) if flag.load(Ordering::Acquire) => {
                cold_path();
                ControlFlow::Break(self)
            }
            _ => ControlFlow::Continue(self),
        }
    }
}

impl<R> FromResidual for Context<R> {
    #[inline]
    fn from_residual(residual: Self) -> Self {
        residual
    }
}

impl<R, T, E: From<Context<R>>> FromResidual<Context<R>> for Result<T, E> {
    #[inline]
    fn from_residual(residual: Context<R>) -> Self {
        Err(residual.into())
    }
}

impl<R> From<Context<R>> for io::Error {
    #[inline]
    fn from(_: Context<R>) -> Self {
        io::Error::new(
            ErrorKind::Interrupted,
            "thread cancellation requested by controller",
        )
    }
}

impl<R, T> FromResidual<Context<R>> for Option<T> {
    #[inline]
    fn from_residual(_residual: Context<R>) -> Self {
        None
    }
}

impl<R, B: Default, C> FromResidual<Context<R>> for ControlFlow<B, C> {
    #[inline]
    fn from_residual(_residual: Context<R>) -> Self {
        ControlFlow::Break(B::default())
    }
}

impl<R> Residual<Context<R>> for Context<R> {
    type TryType = Context<R>;
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
    fn option() {
        let (_controller, cx) = Controller::<!>::new();

        fn maybe<T>(value: T, cx: Context<!>) -> Option<T> {
            cx.cancelled()?;
            Some(value)
        }

        let worker = thread::Builder::new()
            .name("worker".to_string())
            .spawn(move || maybe(5, cx))
            .unwrap();
        thread::sleep(Duration::from_secs(1));
        assert!(worker.is_finished());
        let work = worker.join().unwrap();
        assert_eq!(work, Some(5));
    }

    #[test]
    fn control_flow_continue() {
        let (_controller, cx) = Controller::<!>::new();

        fn maybe<T: Default>(value: T, cx: Context<!>) -> ControlFlow<T, T> {
            cx.cancelled()?;
            ControlFlow::Continue(value)
        }

        let worker = thread::Builder::new()
            .name("worker".to_string())
            .spawn(move || maybe(5, cx))
            .unwrap();
        thread::sleep(Duration::from_secs(1));
        assert!(worker.is_finished());
        let work = worker.join().unwrap();
        assert_eq!(work, ControlFlow::Continue(5));
    }

    #[test]
    fn control_flow_break() {
        let (controller, cx) = Controller::<!>::new();

        fn maybe<T: Default>(_value: T, cx: Context<!>) -> ControlFlow<T, T> {
            loop {
                cx.cancelled()?;
            }
        }

        let worker = thread::Builder::new()
            .name("worker".to_string())
            .spawn(move || maybe(5, cx))
            .unwrap();
        controller.cancel();
        thread::sleep(Duration::from_secs(1));
        assert!(worker.is_finished());
        let work = worker.join().unwrap();
        assert_eq!(work, ControlFlow::Break(0));
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
