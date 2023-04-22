//! A minimal binary for the `mp4san` crate.
//!
//! MP4 input is consumed from `stdin`. Sanitized metadata is provided over `stdout`.

use std::io;
use std::io::{Cursor, Read, Write};

pub fn main() -> Result<(), io::Error> {
    let mut input = Vec::with_capacity(100 * 1024);
    io::stdin().read_to_end(&mut input)?;

    let sanitized = match mp4san::sanitize(Cursor::new(&input)) {
        Ok(sanitized) => sanitized,
        Err(error) => match error {
            mp4san::Error::Io(error) => return Err(error),
            _ => return Err(io::Error::new(io::ErrorKind::Other, "sanitizer error")),
        },
    };

    if let Some(metadata) = sanitized.metadata {
        io::stdout().write_all(&metadata)?;
        io::stdout().write_all(&input[sanitized.data.offset as usize..][..sanitized.data.len as usize])?;
    } else {
        io::stdout().write_all(&input)?;
    }

    Ok(())
}
