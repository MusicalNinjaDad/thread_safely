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

pub mod prelude {
    pub use super::{Context, Controller};
}

pub struct Context {}

pub struct Controller {}

impl Controller {
    pub fn new() -> (Controller, Context) {
        (Controller {}, Context {})
    }

    pub fn cancel() {}
}
