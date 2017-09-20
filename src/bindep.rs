//
// bindep.rs
// Copyright (C) 2017 Adrian Perez <aperez@igalia.com>
// Distributed under terms of the MIT license.
//

use std::convert::AsRef;
use std::collections::HashSet;
use std::io::{ Write, Result as IoResult };
use std::ops::Deref;
use std::path::{ Path, PathBuf };
use super::goblin::{ Object };
use super::memmap::{ Mmap, Protection };
use super::tar;

use errors::*;


mod elf {
    use super::*;
    use ::goblin::elf::{ dyn, Elf };
    use ::regex::{ Captures, Regex };

    lazy_static! {
        static ref RE: Regex = Regex::new(r"(?:\$\{(ORIGIN|LIB|PLATFORM)\}|\$(ORIGIN|LIB|PLATFORM))").unwrap();
    }

    struct Libraries<'a> {
        run_paths: Vec<String>,
        libraries: ::std::slice::Iter<'a, &'a str>,
    }

    fn get_run_paths<'a>(elf: &'a Elf, base_path: &Path) -> Vec<String> {
        let base_path_str = base_path.to_str().unwrap();
        let mut run_paths = vec!();

        if let Some(ref dynamic) = elf.dynamic {
            for dyn in &dynamic.dyns {
                if dyn.d_tag == dyn::DT_RPATH || dyn.d_tag == dyn::DT_RUNPATH {
                    match elf.dynstrtab.get(dyn.d_val as usize) {
                        Some(Ok(path)) => {
                            // TODO: Expand $LIB and $PLATFORM.
                            debug!("expanding run path \"{}\"", path);
                            let expanded = RE.replace_all(path, |caps: &Captures| {
                                match caps.get(1).or(caps.get(2)) {
                                    Some(m) => match m.as_str() {
                                        "ORIGIN" => String::from(base_path_str),
                                        "PLATFORM" => unimplemented!(),
                                        "LIB" => unimplemented!(),
                                        _ => unreachable!(),
                                    },
                                    None => unreachable!(),
                                }
                            });
                            debug!("run path expanded to \"{}\"", expanded);
                            match ::std::fs::canonicalize(expanded.as_ref()) {
                                Ok(full_path) => {
                                    debug!("run path canonicalized to {:?}", full_path);
                                    run_paths.push(full_path.to_string_lossy().into());
                                },
                                Err(e) => warn!("cannot canonicalize path: {}", e)
                            }
                        },
                        Some(Err(e)) => {
                            // XXX: Should this error bubble up?
                            warn!("error fetching strtab[{}]: {}", dyn.d_val, e);
                        },
                        None => {
                            warn!("failed to find [{:?}] in strtab", dyn);
                            println!("{:?}", elf.strtab);
                        },
                    }
                }
            }
        }

        debug!("run paths: {:?}", run_paths);
        run_paths
    }

    impl<'a> Libraries<'a> {
        fn new(path: &'a Path, elf: &'a Elf) -> Self {
            assert!(path.is_absolute());
            assert!(path.is_file());
            Libraries {
                run_paths: get_run_paths(elf, path.parent().unwrap()),
                libraries: elf.libraries.iter(),
            }
        }

        fn resolve_path(&self, lib: &'a str) -> Option<PathBuf> {
            // XXX: Do we need to handle the lib{32,64} madness? For now rely
            // on the operating system providing the needed symbolic links.
            // Should the environment variable $LD_LIBRARY_PATH be handled?

            static LIBDIRS: &[&'static str] = &["/lib", "/usr/lib"];

            let lib_dirs = LIBDIRS.iter().map(Deref::deref);
            let run_paths = self.run_paths.iter().map(String::as_str);

            for lib_dir in run_paths.chain(lib_dirs) {
                let path: PathBuf = [lib_dir, lib].into_iter().collect();
                if path.exists() {
                    return Some(path);
                }
            }

            None
        }
    }

    impl<'a> Iterator for Libraries<'a> {
        type Item = PathBuf;

        fn next(&mut self) -> Option<Self::Item> {
            loop {
                match self.libraries.next() {
                    None => return None,
                    Some(ref lib_name) => match self.resolve_path(lib_name) {
                        Some(path_buf) => return Some(path_buf),
                        None => {
                            warn!("cannot find path for \"{}\"", lib_name);
                            continue;
                        },
                    }
                }
            }
        }
    }

    pub fn libraries(path: &Path, elf: &Elf) -> Vec<PathBuf> {
        Libraries::new(path, elf).map(|p| p.to_path_buf()).collect()
    }
}


//
// Add some utility methods to tar::Builder, to avoid having to
// deal with Header objects altogether in the rest of the code.
//
pub trait TarBuilderExt {
    fn add<P: AsRef<Path>>(&mut self, file_path: &Path, tar_path: P, data: &[u8]) -> IoResult<()>;
    fn symlink<P: AsRef<Path>>(&mut self, dst: P, src: P) -> IoResult<()>;
    fn empty<P: AsRef<Path>>(&mut self, path: P) -> IoResult<()>;
}

impl<W: Write> TarBuilderExt for tar::Builder<W> {
    fn add<P: AsRef<Path>>(&mut self, file_path: &Path, tar_path: P, data: &[u8]) -> IoResult<()> {
        let mut header = tar::Header::new_gnu();
        header.set_metadata(&file_path.metadata()?);
        header.set_path(tar_path)?;
        header.set_cksum();
        self.append(&header, data)
    }

    fn symlink<P: AsRef<Path>>(&mut self, dst: P, src: P) -> IoResult<()> {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_link_name(dst)?;
        header.set_path(src)?;
        header.set_cksum();
        self.append(&header, &[] as &[u8])
    }

    fn empty<P: AsRef<Path>>(&mut self, path: P) -> IoResult<()> {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Regular);
        header.set_path(path)?;
        header.set_size(0);
        header.set_cksum();
        self.append(&header, &[] as &[u8])
    }
}


pub struct Solver<W: Write> {
    files: HashSet<PathBuf>,
    tar: tar::Builder<W>,
}

impl<W: Write> Solver<W> {
    pub fn new(writer: W) -> Result<Self> {
        let mut tar = tar::Builder::new(writer);
        tar.symlink("bin", "sbin")?;
        tar.symlink(".", "usr")?;
        Ok(Solver {
            files: HashSet::new(),
            tar: tar,
        })
    }

    pub fn into_inner(self) -> tar::Builder<W> {
        self.tar
    }

    pub fn scan_file(&mut self, path: &Path) -> Result<()> {
        let needed_libraries = match self.files.replace(path.to_path_buf()) {
            Some(_) => {
                debug!("file {:?} seen, skipping", path);
                return Ok(());
            }
            None => {
                info!("scanning {:?}", path);
                // TODO: Improve error reporting.
                let file_map = Mmap::open_path(path, Protection::Read)
                    .chain_err(|| format!("cannot create memmap for {:?}", path))?;
                debug!("memmap has {} bytes", file_map.len());
                let file_data = unsafe { file_map.as_slice() };

                self.tar.add(path, path.strip_prefix("/").unwrap(), file_data)
                    .chain_err(|| format!("cannot add {:?} to tar file", path))?;

                match Object::parse(file_data).chain_err(|| format!("cannot parse executable {:?}", path))? {
                    Object::Elf(ref obj) => elf::libraries(path, obj),
                    Object::PE(_)        => bail!("unsupported PE binary: {:?}", path),
                    Object::Mach(_)      => bail!("unsupported Mach-O binary: {:?}", path),
                    Object::Archive(_)   => bail!("unsupported file (archive): {:?}", path),
                    Object::Unknown(m)   => bail!("unsupported file (magic {:#x}): {:?}", m, path),
                }
            },
        };
        for library in needed_libraries {
            self.scan_file(&library)?;
        }
        Ok(())
    }
}

