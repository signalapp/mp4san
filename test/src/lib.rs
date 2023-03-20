//! `mp4san` testing library.
//!
//! This crate is separate from mp4san to workaround cargo's inability to specify optional dev-dependencies (see
//! rust-lang/cargo#1596).

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

pub fn example_ftyp() -> Vec<u8> {
    const EXAMPLE_FTYP: &[&[u8]] = &[
        &[0, 0, 0, 20], // box size
        b"ftyp",        // box type
        b"isom",        // major_brand
        &[0, 0, 0, 0],  // minor_version
        b"isom",        // compatible_brands
    ];
    EXAMPLE_FTYP.concat()
}

pub fn example_mdat() -> Vec<u8> {
    const EXAMPLE_MDAT: &[&[u8]] = &[
        &[0, 0, 0, 8], // box size
        b"mdat",       // box type
    ];
    EXAMPLE_MDAT.concat()
}

pub fn example_moov() -> Vec<u8> {
    const EXAMPLE_MOOV: &[&[u8]] = &[
        &[0, 0, 0, 56], // box size
        b"moov",        // box type
        //
        // trak box (inside moov box)
        //
        &[0, 0, 0, 48], // box size
        b"trak",        // box type
        //
        // mdia box (inside trak box)
        //
        &[0, 0, 0, 40], // box size
        b"mdia",        // box type
        //
        // minf box (inside mdia box)
        //
        &[0, 0, 0, 32], // box size
        b"minf",        // box type
        //
        // stbl box (inside minf box)
        //
        &[0, 0, 0, 24], // box size
        b"stbl",        // box type
        //
        // stco box (inside stbl box)
        //
        &[0, 0, 0, 16], // box size
        b"stco",        // box type
        &[0, 0, 0, 0],  // box version & flags
        &[0, 0, 0, 0],  // entry count
    ];
    EXAMPLE_MOOV.concat()
}
