//! The roadmap model behind dagenda.
//!
//! A roadmap is a directed acyclic graph: its nodes are the work (milestones,
//! features, and tasks, each in a track), and its edges say what needs what.
//! Parsing, scheduling, and laying out the tech tree, Gantt, and skill tree
//! views are pure functions of that graph, so they live here, `no_std` and
//! free of I/O. The CLI, the Wasm bindings, and the collaborative editor then
//! share one implementation.

#![no_std]
