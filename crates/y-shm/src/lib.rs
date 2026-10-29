#![forbid(unsafe_code)]
pub mod ring;

pub use ring::{ShmHeader, ShmRingBuffer};
