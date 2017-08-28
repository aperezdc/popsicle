//
// util.rs
// Copyright (C) 2017 Adrian Perez <aperez@igalia.com>
// Distributed under terms of the MIT license.
//

use std::fmt::{ self, Write };
use std::os::unix::prelude::MetadataExt;
use std::path::PathBuf;
use std::process::Command;
use regex::Regex;

use super::blake2_rfc::blake2s;

use errors::*;


#[derive(Debug, Clone, Copy)]
pub enum CompilerKind {
    Gcc,
    Clang,
}


pub fn compiler_info(path: &::std::ffi::OsStr) -> Result<(CompilerKind, String, String)> {
    lazy_static! {
        static ref RE: Regex = Regex::new(r"^(clang|gcc)\s+version\s+([\d\.]+)").unwrap();
    }

    let output = Command::new(path).arg("-v").output()?;
    let out = ::std::str::from_utf8(output.stdout.as_slice())?;
    let err = ::std::str::from_utf8(output.stderr.as_slice())?;

    for line in out.lines().chain(err.lines()) {
        if let Some(cap) = RE.captures(line) {
            let name = cap.get(1).unwrap().as_str().to_lowercase();
            let version = cap.get(2).unwrap().as_str();
            let kind = match name.as_str() {
                "gcc" => CompilerKind::Gcc,
                "clang" => CompilerKind::Clang,
                _ => unreachable!()
            };
            return Ok((kind, name, version.to_string()));
        }
    }

    Err(ErrorKind::CompilerInfoError("no version information").into())
}


pub fn find_program(name: &str, symlink_target: Option<&PathBuf>) -> Result<PathBuf> {
    let name_path = PathBuf::from(name);
    if name_path.is_absolute() {
        return Ok(name_path);
    }

    // Resolve device+inode of the file pointed to by the symlink.
    let target_dev_ino = symlink_target.map(|path| {
        path.metadata().ok().map(|meta| (meta.dev(), meta.ino())).unwrap()
    });

    let search_paths = ::std::env::var("PATH")
        .unwrap_or("/bin:/usr/bin:/usr/local/bin".to_string());

    for path in ::std::env::split_paths(search_paths.as_str()) {
        if path.is_absolute() {
            let full_path: PathBuf = [&path, &name_path].into_iter().collect();
            // TODO: Also check that the file is executable (st_mode?)
            if let Some(target_dev_ino) = target_dev_ino {
                let is_symlink = full_path.symlink_metadata().ok()
                    .map(|meta| meta.file_type().is_symlink()).unwrap_or(false);
                if is_symlink {
                    let path_dev_ino = full_path.metadata().ok()
                        .map(|meta| (meta.dev(), meta.ino())).unwrap_or((0, 0));
                    if path_dev_ino == target_dev_ino {
                        continue;
                    }
                }
            }
            if full_path.is_file() {
                return Ok(full_path);
            }
        } else {
            warn!("path '{:?}' (from $PATH) is not absolute, skipping", path);
        }
    }
    bail!(ErrorKind::ExternalExeError(name.to_string()))
}


#[derive(Eq, PartialEq)]
pub struct Checksum {
    result: blake2s::Blake2sResult,
    hexstr: String,
}

pub struct Checksummer(blake2s::Blake2s);

impl Checksummer {
    #[inline]
    pub fn new() -> Self {
        Checksummer(blake2s::Blake2s::new(32))
    }

    #[inline]
    pub fn update(&mut self, data: &[u8]) {
        self.0.update(data)
    }

    #[inline]
    pub fn finalize(self) -> Checksum {
        Checksum::from_result(self.0.finalize())
    }
}

impl Checksum {
    #[inline]
    fn from_result(result: blake2s::Blake2sResult) -> Self {
        let mut hexstr = String::new();
        for byte in result.as_bytes() {
            write!(hexstr, "{:02x}", byte).unwrap();
        }
        Checksum {
            result: result,
            hexstr: hexstr,
        }
    }

    #[inline]
    pub fn as_string(&self) -> &String {
        &self.hexstr
    }
}

impl fmt::LowerHex for Checksum {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.hexstr)
    }
}
