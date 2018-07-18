//
// main.rs
// Copyright (C) 2017 Adrian Perez <aperez@igalia.com>
// Distributed under terms of the MIT license.
//

// error_chain! can recurse deeply
#![recursion_limit = "1024"]

#[cfg(test)]
#[macro_use] extern crate pretty_assertions;

#[macro_use] extern crate error_chain;
#[macro_use] extern crate lazy_static;
#[macro_use] extern crate log;
#[macro_use] extern crate structopt_derive;

extern crate blake2_rfc;
extern crate env_logger;
extern crate goblin;
extern crate libflate;
extern crate memmap;
extern crate regex;
extern crate structopt;
extern crate tar;
extern crate xdg;

mod csum;
mod bindep;
mod cache;
mod errors;
mod util;

use libflate::gzip;
use std::convert::AsRef;
use std::io::{ Seek, Write };
use std::path::{ Path, PathBuf };
use structopt::StructOpt;

use bindep::TarBuilderExt;
use errors::*;
quick_main!(run);


fn compiler_binaries<P: AsRef<Path>>(compiler_kind: util::CompilerKind, compiler_path: P) -> Option<Vec<PathBuf>> {
    match compiler_kind {
        util::CompilerKind::Gcc => compiler_binaries_gcc(compiler_path.as_ref()),
        util::CompilerKind::Clang => compiler_binaries_clang(compiler_path.as_ref()),
    }
}

#[inline]
fn compiler_print_file_name(compiler_path: &Path, file_name: &str) -> Option<PathBuf> {
    let output = match std::process::Command::new(compiler_path).arg("--print-file-name").arg(file_name).output() {
        Ok(out) => out,
        Err(err) => {
            warn!("could not run compiler {:?}: {}", compiler_path, err);
            return None;
        },
    };

    let path = std::str::from_utf8(output.stdout.as_slice()).unwrap().trim();
    if path == file_name {
        return None;
    }

    let path: PathBuf = path.into();
    if path.is_absolute() { Some(path) } else { None }
}

// TODO: Propagate errors instead of panicing!
#[inline]
fn compiler_binaries_gcc(compiler_path: &Path) -> Option<Vec<PathBuf>> {
    let mut path_list = Vec::new();

    // cc1 is always needed to compile C code.
    path_list.push(compiler_print_file_name(compiler_path, "cc1").unwrap());  // FIXME: panic!

    // The LTO plug-in may (or may not) be available.
    if let Some(lto_plugin) = compiler_print_file_name(compiler_path, "liblto_plugin.so") {
        path_list.push(lto_plugin);
    }

    // C++ support is optional in GCC.
    if let Some(cc1plus) = compiler_print_file_name(compiler_path, "cc1plus") {
        path_list.push(cc1plus);
        // This means that the g++ executable must be around as well.
        let mut gxx = compiler_path.to_path_buf();
        gxx.set_file_name("g++");
        if gxx.is_file() {
            path_list.push(gxx);
        } else {
            path_list.push(util::find_program("g++", None).unwrap());  // FIXME: panic!
        }
    }

    Some(path_list)
}

#[inline]
fn compiler_binaries_clang(_compiler_path: &Path) -> Option<Vec<PathBuf>> {
    None
}


fn compiler_fixup_tar<W: Write>(compiler_kind: util::CompilerKind, tar: &mut tar::Builder<W>) -> Result<()> {
    match compiler_kind {
        util::CompilerKind::Gcc => compiler_fixup_tar_gcc(tar),
        util::CompilerKind::Clang => compiler_fixup_tar_clang(tar),
    }
}

#[inline]
fn compiler_fixup_tar_gcc<W: Write>(_tar: &mut tar::Builder<W>) -> Result<()> {
    Ok(())
}

#[inline]
fn compiler_fixup_tar_clang<W: Write>(tar: &mut tar::Builder<W>) -> Result<()> {
    // There's always (?) C++ support.
    tar.symlink("clang", "bin/clang++")?;

    // Clang 4.x insists in reading /proc/cpuinfo, but it's used only at link
    // time. Provide the file preventively to silence the storm of warnings.
    tar.empty("proc/cpuinfo")?;

    Ok(())
}


#[derive(StructOpt)]
#[structopt(name="popsicle", about="Creates toolchain tarballs for Icecream")]
struct CliOptions {
    #[structopt(short="f", long="force", help="Always rebuild the toolchain tarball")]
    force_rebuild: bool,

    #[structopt(help="Specify the name of the compiler to package")]
    compiler: String,
}


fn run() -> Result<()> {
    env_logger::init();

    let options = CliOptions::from_args();

    let ccache_path = match util::find_program("ccache", None) {
        Ok(path) => {
            info!("ccache found at {:?}", path);
            Some(path)
        },
        Err(e) => {
            warn!("error finding ccache: {}", e);
            None
        }
    };

    let compiler_path = util::find_program(options.compiler, ccache_path.as_ref())?;
    info!("Compiler executable: {:?}", compiler_path);

    let (kind, name, version) = util::compiler_info(compiler_path.as_os_str())?;
    info!("Detected compiler: {}, version: {}", name, version);

    let mut assembler_path = compiler_path.clone();
    assembler_path.set_file_name("as");
    if !assembler_path.is_file() {
        assembler_path = util::find_program("as", None)
            .chain_err(|| "cannot find assembler executable")?;
    }
    info!("Assembler executable: {:?}", assembler_path);

    let true_path = util::find_program("true", None)
        .chain_err(|| "cannot find \"true\" executable")?;

    let mut cache = cache::Cache::new(name.as_str())
        .chain_err(|| "Could not open cache")?;
    info!("cache: {:?}", cache);

    let old_version = cache.get("compiler-version")?;
    cache.add("compiler-version", version.as_bytes())?;

    // The tar file is temporary, and therefore removed immediately.
    let tar_path = cache.path_for("tar-file")?;
    let tar_file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&tar_path)
        .chain_err(|| format!("cannot open {:?} in reading and writing", tar_path))?;
    std::fs::remove_file(&tar_path)?;

    let writer = csum::CSumWriter::new(std::io::BufWriter::new(tar_file));
    let mut solver = bindep::Solver::new(writer)
         .chain_err(|| format!("cannot create write buffer for {:?}", tar_path))?;

    for binary in &[&compiler_path, &assembler_path, &true_path] {
        solver.scan_file(binary.as_path())?;
    }
    if let Some(binaries) = compiler_binaries(kind, compiler_path) {
        for binary in binaries {
            solver.scan_file(binary.as_path())?;
        }
    }

    let mut tar = solver.into_inner();
    compiler_fixup_tar(kind, &mut tar)?;

    let (mut tar_file, checksum) = {
        let (writer, checksum) = tar.into_inner()?.into_inner();
        (writer.into_inner().unwrap(), checksum)
    };
    assert_eq!(0, tar_file.seek(std::io::SeekFrom::Start(0))?);

    cache.add("checksum", checksum)?;
    debug!("cache valid={}", cache.is_valid());

    let targz_path = cache.path_for(&format!("{}-{}.tar.gz", name, version))?;
    if options.force_rebuild || !(targz_path.is_file() && cache.is_valid()) {
        if let Some(version) = old_version {
            cache.del(format!("{}-{}.tar.gz", name, version))?;
        }
        let mut encoder = gzip::Encoder::new(std::io::BufWriter::new(std::fs::File::create(&targz_path)?))?;
        info!("compressing tarball...");
        std::io::copy(&mut std::io::BufReader::new(tar_file), &mut encoder)
            .chain_err(|| format!("cannot compress data from {:?} into {:?}", tar_path, targz_path))?;
        encoder.finish().into_result()?;
    }

    println!("{}", targz_path.to_str().unwrap());
    Ok(())
}
