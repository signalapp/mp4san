use std::ffi::CStr;
use std::fmt;
use std::ptr::null_mut;
use std::ptr::NonNull;

use super::bindings::{gf_error_to_string, gf_isom_last_error, GF_Err, GF_ISOFile};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(GF_Err);

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn last() -> Self {
        Self(unsafe { gf_isom_last_error(null_mut()) })
    }

    pub unsafe fn last_for_file(file: NonNull<GF_ISOFile>) -> Self {
        Self(unsafe { gf_isom_last_error(file.as_ptr()) })
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = unsafe { CStr::from_ptr(gf_error_to_string(self.0)) }.to_string_lossy();
        let code = self.0 as u64;
        write!(f, "{code} ({message})")
    }
}
