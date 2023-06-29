use std::io;

fn main() {
    afl::fuzz!(|data: &[u8]| {
        std::panic::set_hook(Box::new(|panic| {
            eprintln!("{panic}");
            std::process::abort();
        }));
        match mp4san::sanitize(io::Cursor::new(data)) {
            Ok(sanitized) => {
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
                        eprintln!("mp4san returned an io error: {error}\n{error:?}");
                    }
                    _ => panic!(),
                },
                mp4san::Error::Parse(error) => {
                    eprintln!("mp4san returned a parse error: {error}\n{error:?}");
                }
            },
        }
    });
}
