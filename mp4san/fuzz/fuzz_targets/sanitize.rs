#![no_main]

use std::io;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    #[cfg_attr(not(fuzzing_repro), allow(unused))]
    match mp4san::sanitize(io::Cursor::new(data)) {
        Ok(sanitized) => {
            #[cfg(fuzzing_repro)]
            eprintln!(
                "mp4san returned ok: metadata len {metadata_len:?} data offset {data_offset} len {data_len}",
                metadata_len = sanitized.metadata.as_ref().map(|metadata| metadata.len()),
                data_offset = sanitized.data.offset,
                data_len = sanitized.data.len,
            );
        }
        Err(error) => match error {
            mp4san::Error::Io(error) => match error.kind() {
                io::ErrorKind::InvalidData => {
                    #[cfg(fuzzing_repro)]
                    eprintln!("mp4san returned an io error: {error}\n{error:?}");
                }
                _ => panic!(),
            },
            mp4san::Error::Parse(error) => {
                #[cfg(fuzzing_repro)]
                eprintln!("mp4san returned a parse error: {error}\n{error:?}");
            }
        },
    }
});
