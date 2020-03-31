use std::io::{Error, ErrorKind, Read, Seek, SeekFrom};

use reqwest::blocking::{get, Response};

use crate::utils::Cache;

pub struct SeekableRequest {}

impl SeekableRequest {
	pub fn get(url: &str) -> SeekableResponse {
		SeekableResponse::from(get(url).unwrap())
	}
}

pub struct SeekableResponse {
	inner: Response,
	position: usize,
	buffer: Vec<u8>,
}

impl From<Response> for SeekableResponse {
	fn from(inner: Response) -> Self {
		SeekableResponse {
			inner,
			position: 0,
			buffer: Vec::default(),
		}
	}
}

impl Cache for SeekableResponse {
	fn available(&self) -> usize {
		self.buffer.len()
	}

	fn position(&self) -> usize {
		self.position
	}

	fn get(&mut self, index: usize) -> Option<&u8> {
		if self.buffer.len() <= index {
			self.cache_to_index(index);
		}
		self.buffer.get(index)
	}

	fn slice(&mut self, from: usize, to: usize) -> &[u8] {
		if self.buffer.len() <= to {
			self.cache_to_index(to);
		}
		if self.buffer.len() <= from {
			return &[];
		}
		if self.buffer.len() <= to {
			return &self.buffer[from..];
		}
		&self.buffer[from..to]
	}

	#[allow(unused_must_use)]
	fn cache_to_index(&mut self, index: usize) {
		let available = self.buffer.len();
		if index >= available {
			self.read(&mut vec![0u8; index - available]);
		}
	}

	#[allow(unused_must_use)]
	fn cache_to_end(&mut self) {
		self.read_to_end(&mut Vec::default());
	}
}

impl Read for SeekableResponse {
	fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
		Ok(self.inner.read(buf).map(|len| {
			self.buffer.extend(&buf[..len]);
			len
		})?)
	}
}

impl Seek for SeekableResponse {
	fn seek(&mut self, pos: SeekFrom) -> Result<u64, Error> {
		let (position, offset) = match pos {
			SeekFrom::Start(position) => (0, position as i64),
			SeekFrom::Current(position) => (self.position, position),
			SeekFrom::End(position) => (self.buffer.len(), position),
		};
		let position = if offset < 0 {
			position.checked_sub(offset.wrapping_neg() as usize)
		} else {
			position.checked_add(offset as usize)
		};
		match position {
			Some(position) => {
				self.position = position;
				Ok(position as u64)
			}
			None => Err(Error::new(
				ErrorKind::InvalidInput,
				"invalid seek to a negative or overflowing position",
			)),
		}
	}
}
