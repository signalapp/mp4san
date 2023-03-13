use std::ffi::CStr;
use std::ptr::{null_mut, NonNull};

use super::bindings::{gf_blob_register, gf_blob_unregister, gf_free, GF_Blob};

pub struct Blob<'a> {
    data: &'a [u8],
    gf_blob: NonNull<GF_Blob>,
    gf_url: NonNull<i8>,
}

impl<'a> Blob<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        let gf_blob =
            Box::new(GF_Blob { data: data.as_ptr() as *mut u8, size: data.len() as u32, flags: 0, mx: null_mut() });
        let gf_blob = unsafe { NonNull::new_unchecked(Box::into_raw(gf_blob)) };
        let gf_url =
            NonNull::new(unsafe { gf_blob_register(gf_blob.as_ptr()) }).expect("gf_blob_register is infallible");
        Self { data, gf_blob, gf_url }
    }

    pub fn url(&self) -> &CStr {
        unsafe { CStr::from_ptr(self.gf_url.as_ptr()) }
    }
}

impl std::ops::Deref for Blob<'_> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.data
    }
}

impl Drop for Blob<'_> {
    fn drop(&mut self) {
        unsafe {
            gf_free(self.gf_url.as_ptr() as *mut _);
            gf_blob_unregister(self.gf_blob.as_ptr());
            drop(Box::from_raw(self.gf_blob.as_ptr()));
        }
    }
}
