#![no_main]

use std::io;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    #[cfg_attr(not(fuzzing_repro), allow(unused))]
    match webpsan::sanitize(io::Cursor::new(data)) {
        Ok(()) => {
            #[cfg(fuzzing_repro)]
            eprintln!("webpsan returned ok");
        }
        Err(error) => match error {
            webpsan::Error::Io(error) => match error.kind() {
                io::ErrorKind::InvalidData => {
                    #[cfg(fuzzing_repro)]
                    eprintln!("webpsan returned an io error: {error}\n{error:?}");
                }
                _ => panic!(),
            },
            webpsan::Error::Parse(error) => {
                #[cfg(fuzzing_repro)]
                eprintln!("webpsan returned a parse error: {error}\n{error:?}");
            }
        },
    }
});
