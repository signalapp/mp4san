#![cfg(not(doctest))]

mod bindings;
mod blob;
pub mod error;
mod iso_file;

use std::ffi::{c_char, CStr};
use std::ptr::null_mut;

use crate::VerifyError;

use self::bindings::{
    gf_log_set_callback, gf_log_set_tool_level, mp4san_test_gpac_log_callback, GF_LOG_Level, GF_LOG_Tool,
};
use self::blob::Blob;
use self::error::Error;
use self::iso_file::IsoFile;

pub fn verify_gpac(data: &[u8], expected_media_data: Option<&[u8]>) -> Result<(), VerifyError<Error>> {
    #[no_mangle]
    unsafe extern "C" fn mp4san_test_gpac_log(level: GF_LOG_Level, tool: GF_LOG_Tool, message: *const c_char) {
        let message = CStr::from_ptr(message).to_string_lossy();
        let message = message.trim();

        let level = match level {
            GF_LOG_Level::GF_LOG_QUIET | GF_LOG_Level::GF_LOG_ERROR => log::Level::Error,
            GF_LOG_Level::GF_LOG_WARNING => log::Level::Warn,
            GF_LOG_Level::GF_LOG_INFO => log::Level::Info,
            GF_LOG_Level::GF_LOG_DEBUG => log::Level::Debug,
        };

        log::log!(target: "gpac", level, "[{tool:?}] {message}");
    }

    unsafe {
        gf_log_set_callback(null_mut(), Some(mp4san_test_gpac_log_callback));
        gf_log_set_tool_level(GF_LOG_Tool::GF_LOG_ALL, GF_LOG_Level::GF_LOG_DEBUG);
    }

    let blob = Blob::new(data);
    let mut file = IsoFile::new(blob.url())?;
    let mut tracks = (1..=file.track_count())
        .map(|track_number| file.samples(track_number).peekable())
        .collect::<Vec<_>>();
    let mut unverified_media_data = expected_media_data;
    loop {
        let next_track_idx = tracks
            .iter_mut()
            .enumerate()
            .flat_map(|(track_idx, track)| track.peek().map(|sample| (track_idx, sample)))
            .min_by_key(|(_track_idx, sample)| sample.as_ref().map(|sample| sample.data_offset()).map_err(drop))
            .map(|(track_idx, _sample)| track_idx)
            .unwrap_or_default();
        let Some(sample) = tracks[next_track_idx].next() else { break };
        let sample = sample?;

        if let Some(unverified_media_data) = &mut unverified_media_data {
            if sample.len() > unverified_media_data.len() {
                return Err(VerifyError::DataLongerThanExpected {
                    frame_len: sample.len(),
                    remaining: unverified_media_data.len(),
                });
            }

            let expected_sample_data = &unverified_media_data[..sample.len()];
            if &sample[..] != expected_sample_data {
                let offset = unverified_media_data.len() as u64 - expected_media_data.unwrap_or_default().len() as u64;
                return Err(VerifyError::DataMismatch { offset, len: sample.len() });
            }
            *unverified_media_data = &unverified_media_data[expected_sample_data.len()..];
        }
    }
    if let Some(unverified_media_data) = unverified_media_data {
        if !unverified_media_data.is_empty() {
            return Err(VerifyError::DataShorterThanExpected { remaining: unverified_media_data.len() });
        }
    }
    Ok(())
}

impl From<bindings::Bool> for bool {
    fn from(from: bindings::Bool) -> Self {
        match from {
            bindings::Bool::GF_FALSE => false,
            bindings::Bool::GF_TRUE => true,
        }
    }
}
