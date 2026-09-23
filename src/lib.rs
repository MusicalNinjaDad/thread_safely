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
}
