#![cfg(not(doctest))]

mod bindings;
mod blob;
mod iso_file;

use std::ffi::{c_char, CStr};
use std::ptr::null_mut;

use self::bindings::{
    gf_log_set_callback, gf_log_set_tool_level, mp4san_test_gpac_log_callback, GF_LOG_Level, GF_LOG_Tool,
};
use self::blob::Blob;
use self::iso_file::IsoFile;

pub fn verify_gpac(data: &[u8], mut expected_mdat_data: &[u8]) {
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
    let mut file = IsoFile::new(blob.url()).unwrap();
    assert!(file.has_movie());
    assert!(file.moov_first());
    for track_number in 1..=file.track_count() {
        for sample in file.samples(track_number) {
            let sample = sample.unwrap();
            let (expected_sample_data, rest_expected_mdat_data) = expected_mdat_data.split_at(sample.len());
            assert_eq!(&*sample, expected_sample_data);
            expected_mdat_data = rest_expected_mdat_data;
        }
    }
    assert_eq!(expected_mdat_data, b"");
}

impl From<bindings::Bool> for bool {
    fn from(from: bindings::Bool) -> Self {
        match from {
            bindings::Bool::GF_FALSE => false,
            bindings::Bool::GF_TRUE => true,
        }
    }
}
