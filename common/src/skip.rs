//! Utility functions for the [`Skip`] trait.

use std::io;
use std::io::{BufRead, BufReader, Read};

use crate::Skip;

/// Skip `amount` bytes in a [`BufReader`] implementing [`Read`] + [`Skip`].
pub fn buf_skip<R: Read + Skip>(reader: &mut BufReader<R>, amount: u64) -> io::Result<()> {
    let buf_len = reader.buffer().len();
    if let Some(skip_amount) = amount.checked_sub(buf_len as u64) {
        if skip_amount != 0 {
            reader.get_mut().skip(skip_amount)?;
        }
    }
    reader.consume(buf_len.min(amount as usize));
    Ok(())
}

/// Return the stream position for a [`BufReader`] implementing [`Read`] + [`Skip`].
pub fn buf_stream_position<R: Read + Skip>(reader: &mut BufReader<R>) -> io::Result<u64> {
    let stream_pos = reader.get_mut().stream_position()?;
    Ok(stream_pos.saturating_sub(reader.buffer().len() as u64))
}

/// Return the stream length for a [`BufReader`] implementing [`Read`] + [`Skip`].
pub fn buf_stream_len<R: Read + Skip>(reader: &mut BufReader<R>) -> io::Result<u64> {
    reader.get_mut().stream_len()
}
