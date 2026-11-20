//! The logic behind the `dagenda` binary.
//!
//! - `dagenda check`: every error and warning in a set of documents (see
//!   [`check`]).
//! - `dagenda render`: the plan as text, or its tech tree as SVG (see
//!   [`render`]).
//!
//! The binary is a thin wrapper, so everything here works on strings and can
//! be tested without spawning processes.

pub mod check;
pub mod render;
