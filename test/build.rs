fn main() {
    #[cfg(feature = "ffmpeg")]
    {
        use std::env;
        use std::path::PathBuf;

        println!("cargo:rerun-if-changed=src/ffmpeg/log.c");
        println!("cargo:rerun-if-changed=src/ffmpeg/log.h");

        cc::Build::new()
            .file("src/ffmpeg/log.c")
            .compile("libmp4san-test-ffmpeg.a");

        let bindings = bindgen::Builder::default()
            .header("src/ffmpeg/log.h")
            .parse_callbacks(Box::new(bindgen::CargoCallbacks))
            .generate()
            .expect("error generating bindings");
        let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
        bindings
            .write_to_file(out_path.join("ffmpeg-bindings.rs"))
            .expect("Couldn't write bindings!");
    }

    #[cfg(feature = "gpac")]
    {
        use std::env;
        use std::path::PathBuf;

        println!("cargo:rerun-if-changed=src/gpac/bindings.h");
        println!("cargo:rerun-if-changed=src/gpac/log.c");
        println!("cargo:rerun-if-changed=src/gpac/log.h");

        let gpac_include_paths = match pkg_config::Config::new().probe("gpac") {
            Ok(gpac) => gpac.include_paths,
            Err(_) => {
                println!("cargo:rustc-link-lib=gpac");
                vec![]
            }
        };

        cc::Build::new()
            .file("src/gpac/log.c")
            .includes(&gpac_include_paths)
            .compile("libmp4san-test-gpac.a");

        let gpac_include_paths = gpac_include_paths.iter().map(|path| path.display());
        let bindings = bindgen::Builder::default()
            .header("src/gpac/bindings.h")
            .default_enum_style(bindgen::EnumVariation::Rust { non_exhaustive: false })
            .derive_default(true)
            .clang_args(gpac_include_paths.map(|path| format!("-I{path}")))
            .parse_callbacks(Box::new(bindgen::CargoCallbacks))
            .generate()
            .expect("error generating bindings");
        let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
        bindings
            .write_to_file(out_path.join("gpac-bindings.rs"))
            .expect("Couldn't write bindings!");
    }
}
