use std::ops::Range;

use thiserror::Error;

#[derive(Debug, Clone, Error)]
#[error("Invalid memory access: attempt to access `0x{addr:x}` when address must be less than `0x{capacity:x}`")]
pub struct OutOfBounds {
    addr: usize,
    capacity: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Memory {
    bytes: Vec<u8>,
}

impl Memory {
    /// Creates a new block of memory with the given size
    pub fn new(size_bytes: usize) -> Self {
        let mut bytes = Vec::with_capacity(size_bytes);
        // Fill with zeros
        bytes.resize_with(size_bytes, Default::default);

        Self {bytes}
    }

    /// Retrieves a single byte at the given memory address
    pub fn get(&self, addr: usize) -> Result<u8, OutOfBounds> {
        let capacity = self.bytes.len();

        self.bytes.get(addr).copied().ok_or_else(|| OutOfBounds {addr, capacity})
    }

    /// Sets a single byte at the given memory address
    pub fn set(&mut self, addr: usize, value: u8) -> Result<(), OutOfBounds> {
        let capacity = self.bytes.len();

        let cell = self.bytes.get_mut(addr).ok_or_else(|| OutOfBounds {addr, capacity})?;
        *cell = value;

        Ok(())
    }

    /// Retrieves a slice of bytes in the given address range
    pub fn slice(&self, addr_range: Range<usize>) -> Result<&[u8], OutOfBounds> {
        let capacity = self.bytes.len();

        self.bytes.get(addr_range.clone()).ok_or_else(|| {
            if addr_range.start >= capacity {
                OutOfBounds {addr: addr_range.start, capacity}

            } else if addr_range.end > capacity {
                OutOfBounds {addr: addr_range.end-1, capacity}

            } else {
                unreachable!("bug: one of the above conditions should have been met")
            }
        })
    }

    /// Retrieves a mutable slice of bytes in the given address range
    pub fn slice_mut(&mut self, addr_range: Range<usize>) -> Result<&mut [u8], OutOfBounds> {
        let capacity = self.bytes.len();

        self.bytes.get_mut(addr_range.clone()).ok_or_else(|| {
            if addr_range.start >= capacity {
                OutOfBounds {addr: addr_range.start, capacity}

            } else if addr_range.end > capacity {
                OutOfBounds {addr: addr_range.end-1, capacity}

            } else {
                unreachable!("bug: one of the above conditions should have been met")
            }
        })
    }

    /// Writes the given value at the given address
    pub fn write_u64(&mut self, addr: usize, value: u64) -> Result<(), OutOfBounds> {
        let value_bytes = value.to_le_bytes();
        let bytes = self.slice_mut(addr..addr+value_bytes.len())?;
        bytes.copy_from_slice(&value_bytes);

        Ok(())
    }

    /// Reads the given value at the given address
    pub fn read_u64(&self, addr: usize) -> Result<u64, OutOfBounds> {
        let mut value_bytes = [0u8; 8];
        let bytes = self.slice(addr..addr+value_bytes.len())?;
        value_bytes.copy_from_slice(bytes);

        Ok(u64::from_le_bytes(value_bytes))
    }
}