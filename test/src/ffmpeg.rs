mod bindings;

use std::ffi::{c_char, c_int, c_void, CStr};
use std::iter;
use std::{io, mem};

use ac_ffmpeg::format::demuxer::Demuxer as FFMpegDemuxer;
use ac_ffmpeg::format::io as ffmpeg_io;

pub fn verify_ffmpeg(data: &[u8], mut expected_mdat_data: &[u8]) {
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
    let demuxer = FFMpegDemuxer::builder().set_option("strict", "strict");
    let demuxer = demuxer.build(io).unwrap_or_else(|_| panic!());
    let mut demuxer = demuxer.find_stream_info(None).unwrap_or_else(|_| panic!());
    let frames = iter::from_fn(|| demuxer.take().unwrap());

    for frame in frames {
        let (expected_frame_data, rest_expected_mdat_data) = expected_mdat_data.split_at(frame.data().len());
        assert_eq!(frame.data(), expected_frame_data);
        expected_mdat_data = rest_expected_mdat_data;
    }
    assert_eq!(expected_mdat_data, b"");
}
