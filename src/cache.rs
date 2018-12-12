//
// cache.rs
// Copyright (C) 2017 Adrian Perez <aperez@igalia.com>
// Distributed under terms of the MIT license.
//

#[cfg(debug)]
use pretty_assertions::{ assert, assert_eq };

use std::convert::AsRef;
use std::fmt;
use std::fs::File;
use std::io::{ BufReader, BufWriter };
use std::io::prelude::*;
use std::path::PathBuf;
use xdg;

use crate::errors::*;


pub struct Cache {
    xdg: xdg::BaseDirectories,
    valid: bool,
}

impl Cache {
    pub fn new<S: AsRef<str>>(profile: S) -> Result<Self> {
        Ok(Self{
            xdg: xdg::BaseDirectories::with_profile("popsicle", profile.as_ref())?,
            valid: true,
        })
    }

    pub fn is_valid(&self) -> bool {
        self.valid
    }

    #[cfg(test)]
    pub fn has<S: AsRef<str>>(&self, key: S) -> bool {
        match self.xdg.find_cache_file(key.as_ref()) {
            Some(_) => true,
            None => false,
        }
    }

    pub fn get<S: AsRef<str>>(&self, key: S) -> Result<Option<String>> {
        match self.xdg.find_cache_file(key.as_ref()) {
            Some(ref path) => {
                let mut bytes = String::new();
                BufReader::new(File::open(path)?).read_to_string(&mut bytes)?;
                Ok(Some(bytes))
            },
            None => Ok(None)
        }
    }

    pub fn add<S: AsRef<str>, D: AsRef<[u8]>>(&mut self, key: S, data: D) -> Result<()> {
        let path = self.xdg.place_cache_file(key.as_ref())?;

        let must_write_contents = !path.is_file() || {
            let mut file_bytes = BufReader::new(File::open(&path)?).bytes();
            let mut data_bytes = data.as_ref().iter();
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
            BufWriter::new(File::create(path)?).write_all(data.as_ref())?;
        }
        Ok(())
    }

    pub fn del<S: AsRef<str>>(&mut self, key: S) -> Result<()> {
        if let Some(ref path) = self.xdg.find_cache_file(key.as_ref()) {
            ::std::fs::remove_file(path)?;
        }
        Ok(())
    }

    pub fn path_for<S: AsRef<str>>(&mut self, key: S) -> Result<PathBuf> {
        Ok(self.xdg.place_cache_file(key.as_ref())?)
    }
}

impl fmt::Debug for Cache {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Cache({:?}, valid={})", self.xdg.get_cache_home(), self.valid)
    }
}


#[cfg(test)]
mod tests {
    extern crate tempdir;

    use super::*;
    use self::tempdir::TempDir;

    fn make_tempdir() -> TempDir {
        let tmpdir = TempDir::new("popsicle-test").unwrap();
        ::std::env::set_var("XDG_CACHE_HOME", tmpdir.path());
        tmpdir
    }

    #[test]
    fn has_non_existing() {
        let _tmpdir = make_tempdir();
        let cache = Cache::new("test").unwrap();
        assert!(!cache.has("non-existing"));
    }

    #[test]
    fn has_existing() {
        let _tmpdir = make_tempdir();
        let mut cache = Cache::new("test").unwrap();
        cache.add("existing", "this key exists".as_bytes()).unwrap();
        assert!(cache.has("existing"));
    }

    #[test]
    fn get_non_existing() {
        let _tmpdir = make_tempdir();
        let cache = Cache::new("test").unwrap();
        assert_eq!(None, cache.get("non-existing").unwrap());
    }

    #[test]
    fn get_existing() {
        let _tmpdir = make_tempdir();
        let mut cache = Cache::new("test").unwrap();
        cache.add("existing", "this key exists".as_bytes()).unwrap();
        assert_eq!("this key exists", cache.get("existing").unwrap().unwrap());
    }

    #[test]
    fn profiles_dont_clash() {
        let _tmpdir = make_tempdir();
        let mut cache1 = Cache::new("test1").unwrap();
        let mut cache2 = Cache::new("test2").unwrap();
        cache1.add("key", "key in cache 1".as_bytes()).unwrap();
        assert!(cache1.has("key"));
        assert!(!cache2.has("key"));
        cache2.add("key", "key in cache 2".as_bytes()).unwrap();
        assert!(cache2.has("key"));
        assert_eq!("key in cache 1", cache1.get("key").unwrap().unwrap());
        assert_eq!("key in cache 2", cache2.get("key").unwrap().unwrap());
    }
}
