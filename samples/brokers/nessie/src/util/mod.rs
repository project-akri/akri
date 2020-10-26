pub mod nessie;
pub mod nessie_service;

use arraydeque::{ArrayDeque, Wrapping};
// Create a wrapping (non-blocking) ring buffer with a capacity of 10
pub type FrameBuffer = ArrayDeque<[Vec<u8>; 10], Wrapping>;
