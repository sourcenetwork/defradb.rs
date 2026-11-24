//! The records a lens module and its host exchange in the module's memory:
//! `[type_id: i8][len: u32 LE][bytes]`, where type 1 carries JSON, a negative
//! type an error message, 0 a nil and 127 the end of the stream (type alone).

use wasmtime::{AsContext, AsContextMut, Memory, TypedFunc};

use super::CallData;

const NIL: i8 = 0;
const JSON: i8 = 1;
const EOS: i8 = 127;
const HEADER: usize = 5;

/// A record the host writes for the module: a JSON document, or the end of
/// the stream when `None`.
pub(super) type Input<'a> = Option<&'a [u8]>;

/// A record the module returns to the host.
pub(super) enum Output {
    Nil,
    Json(Vec<u8>),
    Error(String),
    Eos,
}

/// The exports the host reads and writes the module's memory through.
#[derive(Clone)]
pub(super) struct Guest {
    memory: Memory,
    alloc: Option<TypedFunc<i32, i32>>,
}

impl Guest {
    pub(super) fn new(memory: Memory, alloc: Option<TypedFunc<i32, i32>>) -> Self {
        Self { memory, alloc }
    }

    /// Writes `input` into memory the module allocates for it, and returns its
    /// address. The module owns that memory from then on and frees it once
    /// read, so every record gets an allocation of its own.
    pub(super) async fn write(
        &self,
        mut store: impl AsContextMut<Data = CallData>,
        input: Input<'_>,
    ) -> wasmtime::Result<i32> {
        let alloc = self
            .alloc
            .as_ref()
            .ok_or_else(|| wasmtime::Error::msg("the lens module exports no alloc function"))?;
        let size = input.map_or(1, |json| HEADER + json.len());
        let too_large = || wasmtime::Error::msg("a document is too large for a lens module");
        let offset = alloc
            .call_async(&mut store, i32::try_from(size).map_err(|_| too_large())?)
            .await?;
        let at = address(offset);
        match input {
            None => self.memory.write(&mut store, at, &EOS.to_le_bytes())?,
            Some(json) => {
                let len = u32::try_from(json.len()).map_err(|_| too_large())?;
                self.memory.write(&mut store, at, &JSON.to_le_bytes())?;
                self.memory.write(&mut store, at + 1, &len.to_le_bytes())?;
                self.memory.write(&mut store, at + HEADER, json)?;
            }
        }
        Ok(offset)
    }

    /// Reads the record the module returned at `offset`.
    pub(super) fn read(
        &self,
        store: impl AsContext<Data = CallData>,
        offset: i32,
    ) -> wasmtime::Result<Output> {
        let at = address(offset);
        let mut type_id = [0u8; 1];
        self.memory.read(&store, at, &mut type_id)?;
        match i8::from_le_bytes(type_id) {
            NIL => Ok(Output::Nil),
            EOS => Ok(Output::Eos),
            JSON => Ok(Output::Json(self.payload(&store, at)?)),
            negative if negative < 0 => Ok(Output::Error(
                String::from_utf8_lossy(&self.payload(&store, at)?).into_owned(),
            )),
            other => Err(wasmtime::Error::msg(format!("unexpected type_id: {other}"))),
        }
    }

    /// The bytes of the record at `at`. Its length is checked against the
    /// module's memory before the host allocates for it.
    fn payload(
        &self,
        store: &impl AsContext<Data = CallData>,
        at: usize,
    ) -> wasmtime::Result<Vec<u8>> {
        let mut len = [0u8; 4];
        self.memory.read(store, at + 1, &mut len)?;
        let len = u32::from_le_bytes(len) as usize;
        let start = at + HEADER;
        if start.saturating_add(len) > self.memory.data_size(store) {
            return Err(wasmtime::Error::msg(
                "a lens module returned a record past the end of its memory",
            ));
        }
        let mut payload = vec![0u8; len];
        self.memory.read(store, start, &mut payload)?;
        Ok(payload)
    }
}

/// A module address: an `i32` holding an unsigned 32-bit offset.
fn address(offset: i32) -> usize {
    offset as u32 as usize
}
