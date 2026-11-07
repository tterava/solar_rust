//! Simulation core: bodies, integrators and the engine thread that runs them.
//! The GUI lives in the binary (`main.rs`) on top of this.

pub mod astronomy;
pub mod engine;
pub mod ephemeris;
pub mod forces;
mod ias15;
pub mod integration;
