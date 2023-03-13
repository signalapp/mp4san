use std::ffi::CStr;
use std::ptr::{null, NonNull};
use std::slice;

use super::bindings::{
    gf_isom_close, gf_isom_get_sample, gf_isom_get_sample_count, gf_isom_last_error, gf_isom_open, gf_isom_sample_del,
    GF_Err, GF_ISOFile, GF_ISOOpenMode, GF_ISOSample,
};

pub struct IsoFile {
    gf_isofile: NonNull<GF_ISOFile>,
}

pub struct IsoSample {
    gf_isosample: NonNull<GF_ISOSample>,
}

impl IsoFile {
    pub fn new(url: &CStr) -> Result<Self, ()> {
        let gf_isofile = unsafe { gf_isom_open(url.as_ptr(), GF_ISOOpenMode::GF_ISOM_OPEN_READ, null()) };
        let gf_isofile = NonNull::new(gf_isofile).ok_or(())?;

        Ok(Self { gf_isofile })
    }

    pub fn samples(&mut self, track_number: u32) -> impl Iterator<Item = Result<IsoSample, GF_Err>> + '_ {
        let sample_count = unsafe { gf_isom_get_sample_count(self.gf_isofile.as_ptr(), track_number) };
        (1..=sample_count).map(move |sample_number| self.sample(track_number, sample_number))
    }

    pub fn sample(&mut self, track_number: u32, sample_number: u32) -> Result<IsoSample, GF_Err> {
        let mut sample_description_index = Default::default();
        let gf_isosample = unsafe {
            gf_isom_get_sample(
                self.gf_isofile.as_ptr(),
                track_number,
                sample_number,
                &mut sample_description_index,
            )
        };
        match NonNull::new(gf_isosample) {
            Some(gf_isosample) => Ok(IsoSample { gf_isosample }),
            None => Err(unsafe { gf_isom_last_error(self.gf_isofile.as_ptr()) }),
        }
    }
}

impl std::ops::Deref for IsoSample {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        match unsafe { NonNull::new(self.gf_isosample.as_ref().data) } {
            None => &[],
            Some(data) => unsafe {
                slice::from_raw_parts(data.as_ptr(), self.gf_isosample.as_ref().dataLength as usize)
            },
        }
    }
}

impl Drop for IsoSample {
    fn drop(&mut self) {
        unsafe {
            gf_isom_sample_del(&mut self.gf_isosample.as_ptr());
        }
    }
}

macro_rules! isofile_method {
    ($($name:ident => $gf_name:ident() -> $return:ty),* $(,)?) => {
        impl IsoFile {
            $(pub fn $name(&mut self) -> $return {
                unsafe {
                    super::bindings::$gf_name(self.gf_isofile.as_ptr()).into()
                }
            })*
        }
    };
}

isofile_method! {
    has_movie => gf_isom_has_movie() -> bool,
    moov_first => gf_isom_moov_first() -> bool,
    track_count => gf_isom_get_track_count() -> u32,
}

impl Drop for IsoFile {
    fn drop(&mut self) {
        unsafe {
            gf_isom_close(self.gf_isofile.as_ptr());
        }
    }
}
