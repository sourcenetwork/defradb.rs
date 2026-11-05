//! The snapshot stream Go Shinzo's `writeKVEntry` writes:
//! `[key length u32 BE][key][value length u32 BE][value]` frames, ended by a
//! zero key length. `proofs/lean/KvSnapshot/Frames.lean` proves the reader
//! returns the entries Go wrote.

use std::io::{ErrorKind, Read};

use super::error::KvSnapshotError;

/// A key's first byte: the store it belongs to.
pub const DATASTORE: u8 = b'd';
pub const HEADSTORE: u8 = b'h';
pub const BLOCKSTORE: u8 = b'b';

/// One frame read back: the store byte, the key after it, the value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub store: u8,
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}

/// Read frames one at a time until the end mark or a clean end of input.
pub struct Entries<R> {
    input: R,
    done: bool,
}

/// Frames read from `input`.
pub fn read_entries<R>(input: R) -> Entries<R>
where
    R: Read,
{
    Entries { input, done: false }
}

impl<R> Entries<R>
where
    R: Read,
{
    /// The next frame, `None` at the end mark or a clean end between frames.
    /// A frame cut part-way is an error.
    pub fn next_entry(&mut self) -> Result<Option<Entry>, KvSnapshotError> {
        if self.done {
            return Ok(None);
        }
        let Some(key_len) = self.length(true)? else {
            self.done = true;
            return Ok(None);
        };
        if key_len == 0 {
            self.done = true;
            return Ok(None);
        }
        let mut key = self.bytes(key_len)?;
        let value_len = self.length(false)?.ok_or(KvSnapshotError::Truncated)?;
        let value = self.bytes(value_len)?;
        let store = key.remove(0);
        Ok(Some(Entry { store, key, value }))
    }

    /// A length; `None` when the input ends before its first byte and
    /// `at_frame` says that is a clean end.
    fn length(&mut self, at_frame: bool) -> Result<Option<usize>, KvSnapshotError> {
        let mut buf = [0u8; 4];
        let mut read = 0;
        while read < 4 {
            match self.input.read(&mut buf[read..]) {
                Ok(0) => break,
                Ok(n) => read += n,
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(error) => return Err(error.into()),
            }
        }
        match read {
            0 if at_frame => Ok(None),
            4 => Ok(Some(u32::from_be_bytes(buf) as usize)),
            _ => Err(KvSnapshotError::Truncated),
        }
    }

    fn bytes(&mut self, len: usize) -> Result<Vec<u8>, KvSnapshotError> {
        let mut buf = Vec::new();
        (&mut self.input).take(len as u64).read_to_end(&mut buf)?;
        if buf.len() == len {
            Ok(buf)
        } else {
            Err(KvSnapshotError::Truncated)
        }
    }
}
