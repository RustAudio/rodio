use std::io::{Read, Seek};

pub mod buffer;
pub mod source;

#[cfg(feature = "http")]
pub trait Cache: Read + Seek {
	fn available(&self) -> usize;

	fn position(&self) -> usize;

	fn get(&mut self, index: usize) -> Option<&u8>;

	fn slice(&mut self, from: usize, to: usize) -> &[u8];

	fn cache_to_index(&mut self, index: usize);

	fn cache_to_end(&mut self);
}
