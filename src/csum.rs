//
// csum.rs
// Copyright (C) 2017 Adrian Perez <aperez@igalia.com>
// Distributed under terms of the MIT license.
//

use blake2_rfc::blake2b::{Blake2b, Blake2bResult};
use std::convert::{AsRef, From};
use std::fmt::Write as FmtWrite;
use std::io::{Result as IOResult, Write};

#[derive(Eq, PartialEq)]
pub struct Checksum {
    result: Blake2bResult,
    hexstr: String,
}

impl From<Blake2bResult> for Checksum {
    fn from(result: Blake2bResult) -> Self {
        let mut hexstr = String::new();
        for byte in result.as_bytes() {
            write!(hexstr, "{:02x}", byte).unwrap();
        }
        Self { result, hexstr }
    }
}

impl AsRef<String> for Checksum {
    fn as_ref(&self) -> &String {
        &self.hexstr
    }
}

impl AsRef<str> for Checksum {
    fn as_ref(&self) -> &str {
        self.hexstr.as_ref()
    }
}

impl AsRef<[u8]> for Checksum {
    fn as_ref(&self) -> &[u8] {
        self.result.as_bytes()
    }
}

pub struct CSumWriter<W: Write> {
    inner: W,
    csum: Blake2b,
}

impl<W: Write> CSumWriter<W> {
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            csum: Blake2b::new(64),
        }
    }

    pub fn into_inner(self) -> (W, Checksum) {
        (self.inner, self.csum.finalize().into())
    }
}

impl<W: Write> Write for CSumWriter<W> {
    fn flush(&mut self) -> IOResult<()> {
        self.inner.flush()
    }

    fn write(&mut self, data: &[u8]) -> IOResult<usize> {
        self.csum.write_all(data)?;
        self.inner.write(data)
    }
}
