//! Utility functions for the [`Skip`] trait.

use std::fs::File;
use std::io;
use std::io::{BufRead, BufReader, Read, Seek};
use std::io::{Cursor, Empty};

use crate::{SeekSkipAdapter, Skip};

//
// Skip impls
//

macro_rules! deref_skip {
    () => {
        fn skip(&mut self, amount: u64) -> io::Result<()> {
            (**self).skip(amount)
        }

        fn stream_position(&mut self) -> io::Result<u64> {
            (**self).stream_position()
        }

        fn stream_len(&mut self) -> io::Result<u64> {
            (**self).stream_len()
        }
    };
}

impl<T: Skip + ?Sized> Skip for &mut T {
    deref_skip!();
}

impl<T: Skip + ?Sized> Skip for Box<T> {
    deref_skip!();
}

macro_rules! skip_via_adapter {
    () => {
        fn skip(&mut self, amount: u64) -> io::Result<()> {
            SeekSkipAdapter(self).skip(amount)
        }

        fn stream_position(&mut self) -> io::Result<u64> {
            SeekSkipAdapter(self).stream_position()
        }

        fn stream_len(&mut self) -> io::Result<u64> {
            SeekSkipAdapter(self).stream_len()
        }
    };
}

impl<T: AsRef<[u8]>> Skip for Cursor<T> {
    skip_via_adapter!();
}

impl Skip for Empty {
    skip_via_adapter!();
}

impl Skip for File {
    skip_via_adapter!();
}

impl Skip for &File {
    skip_via_adapter!();
}

impl<T: Read + Skip + ?Sized> Skip for BufReader<T> {
    fn skip(&mut self, amount: u64) -> io::Result<()> {
        let buf_len = self.buffer().len();
        if let Some(skip_amount) = amount.checked_sub(buf_len as u64) {
            if skip_amount != 0 {
                self.get_mut().skip(skip_amount)?;
            }
        }
        self.consume(buf_len.min(amount as usize));
        Ok(())
    }

    /// Return the stream position for a [`BufReader`] implementing [`Read`] + [`Skip`].
    fn stream_position(&mut self) -> io::Result<u64> {
        let stream_pos = self.get_mut().stream_position()?;
        Ok(stream_pos.saturating_sub(self.buffer().len() as u64))
    }

    /// Return the stream length for a [`BufReader`] implementing [`Read`] + [`Skip`].
    fn stream_len(&mut self) -> io::Result<u64> {
        self.get_mut().stream_len()
    }
}

//
// SeekSkipAdapter impls
//

impl<T: Seek> Skip for SeekSkipAdapter<T> {
    fn skip(&mut self, amount: u64) -> io::Result<()> {
        match amount.try_into() {
            Ok(0) => (),
            Ok(amount) => {
                self.seek(io::SeekFrom::Current(amount))?;
            }
            Err(_) => {
                let stream_pos = self.stream_position()?;
                let seek_pos = stream_pos
                    .checked_add(amount)
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "seek past u64::MAX"))?;
                self.seek(io::SeekFrom::Start(seek_pos))?;
            }
        }
        Ok(())
    }

    fn stream_position(&mut self) -> io::Result<u64> {
        self.0.stream_position()
    }

    fn stream_len(&mut self) -> io::Result<u64> {
        // This is the unstable Seek::stream_len
        let stream_pos = self.stream_position()?;
        let len = self.0.seek(io::SeekFrom::End(0))?;

        if stream_pos != len {
            self.0.seek(io::SeekFrom::Start(stream_pos))?;
        }

        Ok(len)
    }
}

impl<T: Read + ?Sized> Read for SeekSkipAdapter<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}
