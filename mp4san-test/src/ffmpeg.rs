mod bindings;

use std::ffi::{c_char, c_int, c_void, CStr};
use std::iter;
use std::{io, mem};

use ac_ffmpeg::format::demuxer::Demuxer as FFMpegDemuxer;
use ac_ffmpeg::format::io as ffmpeg_io;
use ac_ffmpeg::Error as FFMpegError;

use crate::VerifyError;

pub fn verify_ffmpeg(data: &[u8], expected_media_data: Option<&[u8]>) -> Result<(), VerifyError<FFMpegError>> {
    #[no_mangle]
    unsafe extern "C" fn mp4san_test_ffmpeg_log(level: c_int, message: *const c_char) {
        let message = CStr::from_ptr(message).to_string_lossy();
        let message = message.trim();

        let level = match level {
            0..=23 => log::Level::Error,
            24..=31 => log::Level::Warn,
            32..=47 => log::Level::Info,
            48..=55 => log::Level::Debug,
            _ => log::Level::Trace,
        };
        log::log!(target: "ffmpeg", level, "{message}");
    }

    #[allow(clippy::useless_transmute)] // false positive
    unsafe {
        // va_list is a macro to a struct on some targets, which causes type checking to fail.
        let log_callback: unsafe extern "C" fn(ptr: *mut c_void, level: c_int, format: *const c_char, va_list: _) =
            bindings::mp4san_test_ffmpeg_log_callback;
        let log_callback: unsafe extern "C" fn(ptr: *mut c_void, level: c_int, format: *const c_char, va_list: _) =
            mem::transmute(log_callback);
        ffmpeg_sys_next::av_log_set_callback(Some(log_callback));
    }

    let io = ffmpeg_io::IO::from_seekable_read_stream(io::Cursor::new(data));
    let demuxer = FFMpegDemuxer::builder().set_option("strict", "strict").build(io)?;
    let mut demuxer = demuxer.find_stream_info(None).map_err(|(_demuxer, error)| error)?;
    let frames = iter::from_fn(|| demuxer.take().transpose());
    let mut unverified_media_data = expected_media_data;
    for frame in frames {
        let frame = frame?;

        if let Some(unverified_media_data) = &mut unverified_media_data {
            let expected_frame_data =
                unverified_media_data
                    .get(..frame.data().len())
                    .ok_or_else(|| VerifyError::DataLongerThanExpected {
                        frame_len: frame.data().len(),
                        remaining: unverified_media_data.len(),
                    })?;
            if frame.data() != expected_frame_data {
                let offset = unverified_media_data.len() as u64 - expected_media_data.unwrap_or_default().len() as u64;
                return Err(VerifyError::DataMismatch { offset, len: frame.data().len() });
            }
            *unverified_media_data = &unverified_media_data[expected_frame_data.len()..];
        }
    }
    if let Some(unverified_media_data) = &unverified_media_data {
        if !unverified_media_data.is_empty() {
            return Err(VerifyError::DataShorterThanExpected { remaining: unverified_media_data.len() });
        }
    }
    Ok(())
}
