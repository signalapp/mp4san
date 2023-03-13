/// `mp4san` testing library.
///
/// This crate is separate from mp4san to workaround cargo's inability to specify optional dev-dependencies (see
/// rust-lang/cargo#1596).

#[cfg(feature = "ffmpeg")]
mod ffmpeg;

#[cfg(feature = "gpac")]
mod gpac;

/// Read `data` using ffmpeg, verifying that the demuxed frames match the `expected_mdat_data`.
#[cfg_attr(not(feature = "ffmpeg"), allow(unused_variables))]
pub fn verify_ffmpeg(data: &[u8], expected_mdat_data: &[u8]) {
    #[cfg(not(feature = "ffmpeg"))]
    log::info!("not verifying sanitizer output using ffmpeg; ffmpeg-tests feature disabled");
    #[cfg(feature = "ffmpeg")]
    ffmpeg::verify_ffmpeg(data, expected_mdat_data);
}

/// Read `data` using GPAC.
#[cfg_attr(not(feature = "gpac"), allow(unused_variables))]
pub fn verify_gpac(data: &[u8], expected_mdat_data: &[u8]) {
    #[cfg(not(feature = "gpac"))]
    log::info!("not verifying sanitizer output using gpac; gpac-tests feature disabled");
    #[cfg(feature = "gpac")]
    gpac::verify_gpac(data, expected_mdat_data);
}
