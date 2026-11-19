//! The logic behind the `dagenda` binary.
//!
//! - `dagenda check`: every error and warning in a set of documents (see
//!   [`check`]).
//!
//! The binary is a thin wrapper, so everything here works on strings and can
//! be tested without spawning processes.

pub mod check;
