# thread_safely changelog

## [v0.1.0]

### New features

- Added reply channel
- impl `Default` for `Context`

### Breaking Changes

- `context.canncelled()?` replaces `context.clone()?` to check for cancellation

## [v0.0.1]

### New features

- Initial ability to cancel threads
