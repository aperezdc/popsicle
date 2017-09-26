# Popsicle 🍧

[![Build Status](https://travis-ci.org/aperezdc/popsicle.svg?branch=master)](https://travis-ci.org/aperezdc/popsicle)

Popsicle creates toolchain tarballs for
[Icecream](https://github.com/icecc/icecream) (also known as IceCC).

If you have ever been frustrated by the slowness and feebleness of the
`icecc-create-env` script included as part of Icecream, then Popsicle is for
you:

- Popsicle is smart enough to detect compilers that point to `ccache`, and it
  will figure out by itself where to find the actual location of the program.
- Generated toolchain tarballs will be cached and reused. When any file from
  the tarball is changed (or any of its dependencies), it will be recreated on
  demand.
- The `popsicle` program is stand-alone and does not have external dependencies
  other than the compilers it will package.

Both GCC and Clang are supported.


## Building

You will need a stable version of the [Rust](https://www.rust-lang.org/)
compiler and tools, including Cargo:

```sh
git clone https://github.com/aperezdc/popsicle && cd $_
cargo build --release
```


## Using

Popsicle assembles and caches a tarball with all the tools needed for
compilation, including —but not limited to— the dependency libraries.
Generated tarballs will be cached under `$XDG_CACHE_HOME` (typically
`~/.cache`), and reused whenever possible:

```
aperez@momiji ~ % popsicle gcc
/home/aperez/.cache/popsicle/gcc/gcc-7.2.0.tar.gz
```

As shown above, the full path to the toolchain tarball is printed. The
output from Popsicle can be used to set `$ICECC_VERSION` directly:

```sh
# Create a toolchain tarball (if needed) and get its location.
export ICECC_VERSION=$(popsicle gcc)

# Now compile some big project.
PATH="/usr/lib/ccache:${PATH}" CCACHE_PREFIX=/usr/bin/icecc make CC=gcc -j50
```

This is indeed the kind of usage for which Popsicle was designed.


## Licensing

Distributed under the terms of the [MIT
license](https://opensource.org/licenses/MIT).

