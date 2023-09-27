/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::env;
use std::path::PathBuf;

fn main() {
    let outdir = PathBuf::from(env::var("DEP_MOZJS_OUTDIR").unwrap());

    let include_dir = outdir.join("dist/include");
    let confdefs_path = outdir.join("js/src/js-confdefs.h");

    let include_dir = include_dir.to_str().expect("UTF-8");
    let confdefs = confdefs_path.to_str().expect("UTF-8");

    let target = env::var("TARGET").unwrap();

    build_jsglue(include_dir, confdefs, &target);
    build_jsglue_bindings(include_dir, confdefs, &target);
}

fn cc_flags(target: &str, bindgen: bool) -> Vec<&'static str> {
    let mut result = vec!["-DSTATIC_JS_API"];

    let windows = target.contains("windows");
    let msvc = windows && !bindgen;

    if env::var("CARGO_FEATURE_DEBUGMOZJS").is_ok() {
        result.push("-DDEBUG");

        if msvc {
            result.push("-Od");
        } else {
            result.push("-g");
            result.push("-O0");
        }
    }

    if msvc {
        result.push("-std:c++17");
        result.push("-Zi");
        result.push("-GR-");
    } else {
        result.push("-std=c++17");
        result.push("-fno-rtti");

        if !windows {
            result.push("-fms-compatibility");
            result.push("-fPIC");
        }
    }

    if env::var("CARGO_FEATURE_PROFILEMOZJS").is_ok() {
        result.push("-fno-omit-frame-pointer");
    }

    result.push("-Wno-c++0x-extensions");
    result.push("-Wno-return-type-c-linkage");
    result.push("-Wno-invalid-offsetof");
    result.push("-Wno-unused-parameter");

    result
}

fn build_jsglue(include_dir: &str, confdefs: &str, target: &str) {
    let mut build = cc::Build::new();
    build.cpp(true).file("src/jsglue.cpp");

    build.include(include_dir);

    for flag in cc_flags(target, false) {
        build.flag_if_supported(flag);
    }

    if build.get_compiler().is_like_msvc() {
        build.define("WIN32", "");
        build.flag(&format!("-FI{}", confdefs));
    } else {
        build.flag("-include");
        build.flag(confdefs);
    };

    build.compile("jsglue");
}

fn build_jsglue_bindings(include_dir: &str, confdefs: &str, target: &str) {
    println!("cargo:rerun-if-changed=src/jsglue.cpp");
    let mut builder = bindgen::Builder::default()
        .header("./src/jsglue.cpp")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .size_t_is_usize(true)
        .enable_cxx_namespaces()
        .formatter(bindgen::Formatter::Rustfmt);

    builder = builder.clang_args(["-x", "c++"]);

    builder = builder.clang_args(cc_flags(target, true));

    builder = builder.clang_args(["-I", include_dir]);
    builder = if target.contains("windows") {
        builder.clang_arg("-DWIN32")
            .clang_arg(format!("-FI{}", confdefs))
    } else {
        builder.clang_args(["-include", confdefs])
    };

    for ty in BLACKLIST_TYPES {
        builder = builder.blocklist_type(ty);
    }

    for ty in OPAQUE_TYPES {
        builder = builder.opaque_type(ty);
    }

    for &(module, raw_line) in MODULE_RAW_LINES {
        builder = builder.module_raw_line(module, raw_line);
    }

    builder = builder.allowlist_file("./src/jsglue.cpp")
        .allowlist_recursively(false);

    let bindings = builder
        .generate()
        .expect("Unable to generate bindings to jsglue");

    // Write the bindings to the $OUT_DIR/gluebindings.rs file.
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap()).join("gluebindings.rs");
    bindings
        .write_to_file(out_path)
        .expect("Couldn't write bindings!");
}

/// Types that have generic arguments must be here or else bindgen does not generate <T>
/// as it treats them as opaque types
const BLACKLIST_TYPES: &'static [&'static str] = &[
    "JS::.*",
    "already_AddRefed",
    // we don't want it null
    "EncodedStringCallback",
];

/// Types that should be treated as an opaque blob of bytes whenever they show
/// up within a whitelisted type.
///
/// These are types which are too tricky for bindgen to handle, and/or use C++
/// features that don't have an equivalent in rust, such as partial template
/// specialization.
const OPAQUE_TYPES: &'static [&'static str] = &[
    "JS::Auto.*Impl",
    "JS::StackGCVector.*",
    "JS::PersistentRooted.*",
    "JS::detail::CallArgsBase.*",
    "js::detail::UniqueSelector.*",
    "mozilla::BufferList",
    "mozilla::Maybe.*",
    "mozilla::UniquePtr.*",
    "mozilla::Variant",
    "mozilla::Hash.*",
    "mozilla::detail::Hash.*",
    "RefPtr_Proxy.*",
];

/// Map mozjs_sys mod namespaces to bindgen mod namespaces
const MODULE_RAW_LINES: &'static [(&'static str, &'static str)] = &[
    ("root", "pub(crate) use mozjs_sys::jsapi::*;"),
    ("root", "pub use crate::glue::EncodedStringCallback;"),
    ("root::js", "pub(crate) use mozjs_sys::jsapi::js::*;"),
    (
        "root::mozilla",
        "pub(crate) use mozjs_sys::jsapi::mozilla::*;",
    ),
    ("root::JS", "pub(crate) use mozjs_sys::jsapi::JS::*;"),
];
