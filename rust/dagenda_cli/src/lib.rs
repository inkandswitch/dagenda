//! The logic behind the `dagenda` binary.
//!
//! - `dagenda check`: every error and warning in a set of documents (see
//!   [`dagenda_core::check`], which the Wasm bindings share).
//! - `dagenda render`: the plan as text, or its tech tree as SVG (see
//!   [`render`]).
//! - `dagenda edit`: change a plan in place, the smallest change that says it
//!   (see [`edit`], over [`dagenda_core::edit`]).
//!
//! The binary is a thin wrapper, so everything here works on strings and can
//! be tested without spawning processes.

pub mod edit;
pub mod render;
