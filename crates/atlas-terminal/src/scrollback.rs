#![allow(unused)]

const DEFAULT_CAPACITY: usize = 1024 * 1024; // 1MB

pub struct RingBuffer {
    buf: Vec<u8>,
    capacity: usize,
    write_pos: usize,
    len: usize,
}

impl RingBuffer {
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buf: vec![0u8; capacity],
            capacity,
            write_pos: 0,
            len: 0,
        }
    }

    pub fn write(&mut self, data: &[u8]) {
        for &byte in data {
            self.buf[self.write_pos] = byte;
            self.write_pos = (self.write_pos + 1) % self.capacity;
            if self.len < self.capacity {
                self.len += 1;
            }
        }
    }

    pub fn to_vec(&self) -> Vec<u8> {
        if self.len < self.capacity {
            self.buf[..self.len].to_vec()
        } else {
            let start = self.write_pos;
            let mut result = Vec::with_capacity(self.capacity);
            result.extend_from_slice(&self.buf[start..]);
            result.extend_from_slice(&self.buf[..start]);
            result
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl Default for RingBuffer {
    fn default() -> Self {
        Self::new()
    }
}
