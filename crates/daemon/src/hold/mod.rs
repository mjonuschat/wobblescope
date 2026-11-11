//! Held excitation: making the printer vibrate at one frequency until it is stopped.
//!
//! The service sends short pieces of plain G-code moves over the Moonraker connection, paced on
//! the host clock, under the same one-test slot a sweep uses.

pub mod pieces;
