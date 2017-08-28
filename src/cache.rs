//
// cache.rs
// Copyright (C) 2017 Adrian Perez <aperez@igalia.com>
// Distributed under terms of the MIT license.
//

use std::fmt;
use std::fs::File;
use std::io::{ BufReader, BufWriter };
use std::io::prelude::*;
use std::path::PathBuf;

use super::xdg;

use errors::*;


pub trait Cache: ::std::fmt::Debug {
    fn is_valid(&self) -> bool;
    fn get(&self, key: &str) -> Result<Option<String>>;
    fn add(&mut self, key: &str, data: &[u8]) -> Result<()>;
    fn del(&mut self, key: &str) -> Result<()>;
    fn path_for(&mut self, key: &str) -> Result<PathBuf>;

    fn has(&self, key: &str) -> bool {
        match self.get(key) {
            Ok(Some(_)) => true,
            _ => false
        }
    }
}


pub struct DummyCache { }

impl DummyCache {
    pub fn new() -> Self {
        DummyCache {}
    }
}

impl Cache for DummyCache {
    fn is_valid(&self) -> bool { false }
    fn get(&self, _key: &str) -> Result<Option<String>> { Ok(None) }
    fn add(&mut self, _key: &str, _data: &[u8]) -> Result<()> { Ok(()) }
    fn del(&mut self, _key: &str,) -> Result<()> { Ok(()) }

    fn path_for(&mut self, _key: &str) -> Result<PathBuf> {
        unimplemented!()
    }
}

impl fmt::Debug for DummyCache {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "None/Dummy")
    }
}


pub struct XdgCache {
    xdg: xdg::BaseDirectories,
    valid: bool,
}

impl XdgCache {
    pub fn new(profile: &str) -> Result<Self> {
        Ok(Self{
            xdg: xdg::BaseDirectories::with_profile("popsicle", profile)?,
            valid: true,
        })
    }
}

impl Cache for XdgCache {
    fn is_valid(&self) -> bool {
        self.valid
    }

    fn has(&self, key: &str) -> bool {
        match self.xdg.find_cache_file(key) {
            Some(_) => true,
            None => false,
        }
    }

    fn get(&self, key: &str) -> Result<Option<String>> {
        match self.xdg.find_cache_file(key) {
            Some(ref path) => {
                let mut bytes = String::new();
                BufReader::new(File::open(path)?).read_to_string(&mut bytes)?;
                Ok(Some(bytes))
            },
            None => Ok(None)
        }
    }

    fn add(&mut self, key: &str, data: &[u8]) -> Result<()> {
        let path = self.xdg.place_cache_file(key)?;

        let must_write_contents = !path.is_file() || {
            let mut file_bytes = BufReader::new(File::open(&path)?).bytes();
            let mut data_bytes = data.iter();
            loop {
                match (file_bytes.next(), data_bytes.next()) {
                    (Some(Ok(a)), Some(b)) if a == *b => continue,
                    (Some(Err(e)), _) => Err(e)?,
                    (None, None) => break false,
                    (_, _) => break true,
                }
            }
        };

        if must_write_contents {
            self.valid = false;
            BufWriter::new(File::create(path)?).write_all(data)?;
        }
        Ok(())
    }

    fn del(&mut self, key: &str) -> Result<()> {
        match self.xdg.find_cache_file(key) {
            Some(ref path) if path.is_file() => Ok(::std::fs::remove_file(path)?),
            _ => Ok(())
        }
    }

    fn path_for(&mut self, key: &str) -> Result<PathBuf> {
        Ok(self.xdg.place_cache_file(key)?)
    }
}

impl fmt::Debug for XdgCache {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "XDG({:?}, valid={})", self.xdg.get_cache_home(), self.valid)
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use ::tempdir::TempDir;

    fn make_tempdir() -> TempDir {
        let tmpdir = TempDir::new("popsicle-test").unwrap();
        ::std::env::set_var("XDG_CACHE_HOME", tmpdir.path());
        tmpdir
    }

    #[test]
    fn has_non_existing() {
        let _tmpdir = make_tempdir();
        let cache = XdgCache::new("test").unwrap();
        assert!(!cache.has("non-existing"));
    }

    #[test]
    fn has_existing() {
        let _tmpdir = make_tempdir();
        let mut cache = XdgCache::new("test").unwrap();
        cache.add("existing", "this key exists".as_bytes()).unwrap();
        assert!(cache.has("existing"));
    }

    #[test]
    fn get_non_existing() {
        let _tmpdir = make_tempdir();
        let cache = XdgCache::new("test").unwrap();
        assert_eq!(None, cache.get("non-existing").unwrap());
    }

    #[test]
    fn get_existing() {
        let _tmpdir = make_tempdir();
        let mut cache = XdgCache::new("test").unwrap();
        cache.add("existing", "this key exists".as_bytes()).unwrap();
        assert_eq!("this key exists", cache.get("existing").unwrap().unwrap());
    }

    #[test]
    fn profiles_dont_clash() {
        let _tmpdir = make_tempdir();
        let mut cache1 = XdgCache::new("test1").unwrap();
        let mut cache2 = XdgCache::new("test2").unwrap();
        cache1.add("key", "key in cache 1".as_bytes()).unwrap();
        assert!(cache1.has("key"));
        assert!(!cache2.has("key"));
        cache2.add("key", "key in cache 2".as_bytes()).unwrap();
        assert!(cache2.has("key"));
        assert_eq!("key in cache 1", cache1.get("key").unwrap().unwrap());
        assert_eq!("key in cache 2", cache2.get("key").unwrap().unwrap());
    }
}
