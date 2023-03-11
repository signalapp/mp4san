#[cfg(test)]
pub mod test;

use std::io;

/// Checked addition with a signed integer. Computes `lhs + rhs`, returning `None` if overflow occurred.
///
/// This is the unstable `<int>::checked_add_signed`.
pub fn checked_add_signed<Lhs: CheckedAddSigned>(lhs: Lhs, rhs: Lhs::Rhs) -> Option<Lhs> {
    lhs.checked_add_signed(rhs)
}

pub trait CheckedAddSigned: Sized {
    type Rhs;
    fn checked_add_signed(self, rhs: Self::Rhs) -> Option<Self>;
}

pub trait IoResultExt: Sized {
    type Ok;

    fn map_eof<E: From<io::Error>, F: FnOnce(io::Error) -> E>(self, map: F) -> Result<Self::Ok, E>;
}

macro_rules! impl_checked_add_signed {
    ($lhs:ty, $rhs:ty) => {
        impl CheckedAddSigned for $lhs {
            type Rhs = $rhs;

            fn checked_add_signed(self, rhs: Self::Rhs) -> Option<Self> {
                let (result, overflowed) = self.overflowing_add(rhs as Self);
                if overflowed ^ (rhs < 0) {
                    None
                } else {
                    Some(result)
                }
            }
        }
    };
}

impl_checked_add_signed!(u8, i8);
impl_checked_add_signed!(u16, i16);
impl_checked_add_signed!(u32, i32);
impl_checked_add_signed!(u64, i64);
impl_checked_add_signed!(u128, i128);
impl_checked_add_signed!(usize, isize);

impl<T> IoResultExt for Result<T, io::Error> {
    type Ok = T;

    fn map_eof<E: From<io::Error>, F: FnOnce(io::Error) -> E>(self, map: F) -> Result<T, E> {
        match self {
            Ok(ok) => Ok(ok),
            Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => Err(map(err)),
            Err(err) => Err(err.into()),
        }
    }
}
