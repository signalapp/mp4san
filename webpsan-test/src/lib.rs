//! `webpsan` testing library.
//!
//! This crate is separate from `webpsan` to workaround cargo's inability to specify optional dev-dependencies (see
//! rust-lang/cargo#1596).

#[cfg(feature = "libwebp")]
pub mod libwebp;

//
// public functions
//

/// Read `data` using `libwebp`, verifying that it cannot be decoded.
#[cfg_attr(not(feature = "libwebp"), allow(unused_variables))]
pub fn libwebp_assert_invalid(data: &[u8]) {
    #[cfg(not(feature = "libwebp"))]
    log::info!("not verifying sanitizer output using libwebp; libwebp feature disabled");
    #[cfg(feature = "libwebp")]
    libwebp::verify(data)
        .err()
        .unwrap_or_else(|| panic!("libwebp didn't return an error"));
}

/// Read `data` using `libwebp`, verifying that it can be decoded.
#[cfg_attr(not(feature = "libwebp"), allow(unused_variables))]
pub fn libwebp_assert_valid(data: &[u8]) {
    #[cfg(not(feature = "libwebp"))]
    log::info!("not verifying sanitizer output using libwebp; libwebp feature disabled");
    #[cfg(feature = "libwebp")]
    libwebp::verify(data).unwrap_or_else(|error| panic!("libwebp returned an error: {error}\n{error:?}"));
}
