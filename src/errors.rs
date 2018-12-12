//
// errors.rs
// Copyright (C) 2017 Adrian Perez <aperez@igalia.com>
// Distributed under terms of the MIT license.
//

use error_chain::*;

error_chain!{
    errors {
        CompilerInfoError(t: &'static str) {
            description("compiler information error")
            display("cannot obtain compiler information: {}", t)
        }

        ExternalExeError(name: std::path::PathBuf) {
            description("external program error")
            display("cannot find external program: {:?}", name)
        }
    }

    foreign_links {
        Io(std::io::Error);
        Re(regex::Error);
        Utf8(std::str::Utf8Error);
        Xdg(xdg::BaseDirectoriesError);
    }
}

