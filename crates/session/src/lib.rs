//! The session format: sample files, metadata and Klipper CSV import. Pure codec modules build
//! natively and to wasm32 from one source.

pub mod bridge;
pub mod codec;
pub mod error;
pub mod meta;

pub use error::SessionError;
