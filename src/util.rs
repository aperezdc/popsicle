//
// util.rs
// Copyright (C) 2017 Adrian Perez <aperez@igalia.com>
// Distributed under terms of the MIT license.
//

use std::convert::AsRef;
use std::os::unix::prelude::MetadataExt;
use std::path::{ Path, PathBuf };
use std::process::Command;
use regex::bytes::Regex;
use errors::*;


#[derive(Debug, Clone, Copy)]
pub enum CompilerKind {
    Gcc,
    Clang,
}

const NL: u8 = 0x0A;


pub fn compiler_info(path: &::std::ffi::OsStr) -> Result<(CompilerKind, String, String)> {
    lazy_static! {
        static ref RE: Regex = Regex::new(r"^(clang|gcc)\s+version\s+([\d\.]+)").unwrap();
    }

    let output = Command::new(path).arg("-v").output()?;
    let out = output.stdout.as_slice();
    let err = output.stderr.as_slice();

    for line in out.split(|&c| c == NL).chain(err.split(|&c| c == NL)) {
        if let Some(cap) = RE.captures(line) {
            let name = ::std::str::from_utf8(&cap[1]).unwrap();
            let version = ::std::str::from_utf8(&cap[2]).unwrap();
            let kind = match name {
                "gcc" => CompilerKind::Gcc,
                "clang" => CompilerKind::Clang,
                _ => unreachable!()
            };
            return Ok((kind, name.to_string(), version.to_string()));
        }
    }

    Err(ErrorKind::CompilerInfoError("no version information").into())
}


pub fn find_program<P: AsRef<Path>>(name: P, symlink_target: Option<&PathBuf>) -> Result<PathBuf> {
    let name_path = name.as_ref();
    if name_path.is_absolute() {
        return Ok(name_path.to_path_buf());
    }

    // Resolve device+inode of the file pointed to by the symlink.
    let target_dev_ino = symlink_target.map(|path| {
        path.metadata().ok().map(|meta| (meta.dev(), meta.ino())).unwrap()
    });

    let search_paths = ::std::env::var("PATH")
        .unwrap_or_else(|_| "/bin:/usr/bin:/usr/local/bin".to_string());

    for path in ::std::env::split_paths(search_paths.as_str()) {
        if path.is_absolute() {
            let full_path: PathBuf = [&path, name_path].into_iter().collect();
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
    bail!(ErrorKind::ExternalExeError(name_path.to_path_buf()))
}
