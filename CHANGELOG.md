# thread_safely changelog

## [v0.1.0]

### New features

- Added reply channel
- impl `Default` for `Context`
- improved cancellation ergonomics

### Breaking Changes

- `context.canncelled()?` replaces `context.clone()?` to check for cancellation
- removed `Cancelled` type
- `Context<R>::Output: Self` - must check for cancellation at end of `try`-block, unless it contains an infinite loop

## [v0.0.1]

### New features

- Initial ability to cancel threads
