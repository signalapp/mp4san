use std::io;
use std::io::{Read, Seek};
use std::mem::take;
use std::num::NonZeroU64;

use bytes::Buf;
use bytes::BytesMut;

use crate::util::checked_add_signed;
use crate::Error;

/// A buffer of input data which can also keep track of [skipped](Self::skip_input) input.
///
/// [Appended](Self::append_input) input data can be read or seeked over using the [`Read` adapter](Self::reader), while
/// skipped input data can only be seeked over.
#[derive(Clone, Debug)]
pub struct Buffer {
    /// The buffer of data, which can be in one of two [states](BufferState).
    state: BufferState,

    /// The starting input position of the beginning of [`buffer`](Self::buffer).
    input_pos: u64,

    /// The known total length of the input.
    input_len: u64,
}

/// A [`Buffer`] adapter implementing [`Read`] and [`Seek`].
///
/// This struct can be constructed using [`Buffer::reader`]. See its documentation for more details.
pub struct Reader<'a> {
    /// The unmodified [`Buffer`] this `Reader` was created from.
    buffer: &'a mut Buffer,

    /// A modified [`BufferState`] cloned from [`buffer`](Self::buffer) with any bytes [read](Read::read) from this
    /// `Reader` [`advance`d](BufferState::advance) over.
    new_state: BufferState,
}

#[derive(Clone, Debug)]
enum BufferState {
    Reading {
        /// The buffered data.
        data: BytesMut,
    },
    Skipping {
        /// The buffered data.
        data: BytesMut,

        /// Number of bytes of input coming _after_ [`data`](Self::Skipping::data) skipped instead of appended to it.
        skipped: NonZeroU64,
    },
}

//
// Buffer impls
//

impl Buffer {
    /// Construct a new `Buffer` with a given known total input length.
    pub fn new(input_len: u64) -> Self {
        Self { state: Default::default(), input_len, input_pos: 0 }
    }

    /// Append a chunk of input data.
    pub fn append_input(&mut self, new_data: &[u8]) -> Result<(), Error> {
        self.validate_input_len(new_data.len() as u64)?;
        match &mut self.state {
            BufferState::Reading { data } => data.extend_from_slice(new_data),
            BufferState::Skipping { skipped, .. } => {
                let skip_amount = new_data.len() as u64;
                *skipped = skipped
                    .checked_add(skip_amount)
                    .expect("unexpected input length overflow");
            }
        }
        Ok(())
    }

    /// Signal that `amount` bytes of input were skipped instead of being [appended](Self::append_input).
    ///
    /// As currently implemented, once this method is called, any data [appended](Self::append_input) will be skipped
    /// over as well, until the buffer is [exhausted](Self::is_empty).
    pub fn skip_input(&mut self, amount: u64) -> Result<(), Error> {
        self.validate_input_len(amount)?;
        match &mut self.state {
            BufferState::Reading { data } => {
                if let Some(skipped) = NonZeroU64::new(amount) {
                    self.state = BufferState::Skipping { data: take(data), skipped };
                }
            }
            BufferState::Skipping { skipped, .. } => {
                *skipped = skipped.checked_add(amount).expect("unexpected input length overflow");
            }
        };
        Ok(())
    }

    /// Return the input position of the end of this buffer, i.e. the starting input position of the buffer plus the
    /// buffer length.
    pub fn end_input_pos(&self) -> u64 {
        self.input_pos
            .checked_add(self.remaining())
            .expect("unexpected input length overflow")
    }

    /// The known total length of the input, including input has not yet been appended to the buffer.
    pub fn input_len(&self) -> u64 {
        self.input_len
    }

    /// Return whether the buffer has no data [remaining](Self::remaining) to be [read](Read::read) or [seeked](Seek::seek) over.
    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    /// Create an adapter which implements [`Read`] and [`Seek`] for `self`.
    ///
    /// The returned [`Reader`] will only consume data from this buffer once [`commit`](Reader::commit) is called. If
    /// the `Reader` is instead simply dropped, `self` will be unmodified.
    ///
    /// The returned adapter will return an [`io::Error`] with a [kind](io::Error::kind) of
    /// [`UnexpectedEof`](io::ErrorKind::UnexpectedEof) if a [read](Read::read) or [seek](Seek::seek) is attempted
    /// beyond the extent of the buffer.
    pub fn reader(&mut self) -> Reader<'_> {
        Reader { new_state: self.state.clone(), buffer: self }
    }

    /// Return how much data the buffer has remaining to be either [read](Read::read) or [seeked](Seek::seek) over.
    pub fn remaining(&self) -> u64 {
        self.state.remaining()
    }

    fn validate_input_len(&self, append_amount: u64) -> Result<(), Error> {
        match self.end_input_pos().checked_add(append_amount) {
            None => Err(Error::InvalidInput("input length overflow")),
            Some(new_end_input_pos) if new_end_input_pos > self.input_len => {
                Err(Error::InvalidInput("initial input length exceeded"))
            }
            Some(_) => Ok(()),
        }
    }
}

//
// Reader impls
//

impl Reader<'_> {
    /// Remove any data read from this `Reader` so far from the underlying [`Buffer`].
    pub fn commit(self) {
        self.buffer.input_pos += self.read_bytes();
        self.buffer.state = self.new_state;
    }

    /// Return a reference to the [`Buffer`] which this `Reader` reads from.
    pub fn get_ref(&self) -> &Buffer {
        self.buffer
    }

    /// Return the current input position of this `Reader'.
    fn input_pos(&self) -> Result<u64, Error> {
        let remaining_bytes = self.new_state.remaining();
        let total_bytes = self.buffer.state.remaining();
        let currently_read_bytes = total_bytes - remaining_bytes;
        self.buffer
            .input_pos
            .checked_add(currently_read_bytes)
            .ok_or(Error::InvalidInput("input length overflow"))
    }

    /// Return the number of bytes read from this `Reader` so far.
    fn read_bytes(&self) -> u64 {
        let remaining_bytes = self.new_state.remaining();
        let total_bytes = self.buffer.state.remaining();
        total_bytes - remaining_bytes
    }
}

impl Read for Reader<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        match &mut self.new_state {
            BufferState::Skipping { data, .. } if data.is_empty() => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                Error::InvalidInput("skipped input which sanitizer required for parsing"),
            )),

            BufferState::Skipping { data, .. } | BufferState::Reading { data } => data.reader().read(out),
        }
    }
}

impl Seek for Reader<'_> {
    fn seek(&mut self, seek_from: io::SeekFrom) -> io::Result<u64> {
        let input_pos = self.input_pos()?;
        let absolute_seek_pos = match seek_from {
            io::SeekFrom::Start(absolute_seek_pos) => absolute_seek_pos,
            io::SeekFrom::End(relative_seek_amount) => {
                let end_pos = self.buffer.end_input_pos();
                if end_pos < self.buffer.input_len {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "sanitizer buffer seek from end before input finished",
                    ));
                }
                checked_add_signed(end_pos, relative_seek_amount).ok_or(Error::InvalidInput("input length overflow"))?
            }
            io::SeekFrom::Current(relative_seek_amount) => checked_add_signed(input_pos, relative_seek_amount)
                .ok_or(Error::InvalidInput("input length overflow"))?,
        };

        if let Some(advance_amount) = absolute_seek_pos.checked_sub(input_pos) {
            self.new_state.advance(advance_amount)?;
            Ok(absolute_seek_pos)
        } else if let Some(advance_amount) = absolute_seek_pos.checked_sub(self.buffer.input_pos) {
            self.new_state = self.buffer.state.clone();
            self.new_state.advance(advance_amount)?;
            Ok(absolute_seek_pos)
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "seek backward across box boundary",
            ))
        }
    }
}

//
// BufferState impls
//

impl BufferState {
    fn remaining(&self) -> u64 {
        match self {
            BufferState::Reading { data } => data.len() as u64,
            BufferState::Skipping { data, skipped } => skipped.checked_add(data.len() as u64).unwrap().get(),
        }
    }

    fn advance(&mut self, amount: u64) -> Result<(), io::Error> {
        if amount == 0 {
            return Ok(());
        }

        match self {
            BufferState::Skipping { data, skipped } => {
                let advance_data_amount = (amount as usize).min(data.len());
                let skip_amount = amount.saturating_sub(data.len() as u64);
                let new_skipped = skipped.get().checked_sub(skip_amount).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::UnexpectedEof, "advanced past end of sanitizer buffer")
                })?;

                data.advance(advance_data_amount);
                match NonZeroU64::new(new_skipped) {
                    Some(new_skipped) => *skipped = new_skipped,
                    None => *self = BufferState::Reading { data: take(data) },
                }
            }

            BufferState::Reading { data } => {
                let advance_data_amount = usize::try_from(amount).map_err(|_| {
                    io::Error::new(io::ErrorKind::UnexpectedEof, "advanced past end of sanitizer buffer")
                })?;
                if advance_data_amount > data.len() {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "advanced past end of sanitizer buffer",
                    ));
                }

                data.advance(advance_data_amount);
            }
        }
        Ok(())
    }
}

impl Default for BufferState {
    fn default() -> Self {
        Self::Reading { data: BytesMut::with_capacity(1024 * 1024) }
    }
}
