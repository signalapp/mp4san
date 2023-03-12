fn main() {
    #[cfg(feature = "ffmpeg")]
    {
        use std::env;
        use std::path::PathBuf;

        println!("cargo:rerun-if-changed=src/log.c");
        println!("cargo:rerun-if-changed=src/log.h");

        cc::Build::new().file("src/log.c").compile("libmp4san-test-ffmpeg.a");

        let bindings = bindgen::Builder::default()
            .header("src/log.h")
            .parse_callbacks(Box::new(bindgen::CargoCallbacks))
            .generate()
            .expect("error generating bindings");
        let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
        bindings
            .write_to_file(out_path.join("bindings.rs"))
            .expect("Couldn't write bindings!");
    }
}
