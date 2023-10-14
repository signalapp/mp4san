use std::mem::MaybeUninit;
use std::ptr::{null_mut, NonNull};

use libwebp_sys::{
    VP8StatusCode, WebPData, WebPDecode, WebPDecoderConfig, WebPDecoderOptions, WebPDemuxDelete, WebPDemuxGetFrame,
    WebPDemuxInternal, WebPDemuxNextFrame, WebPDemuxReleaseIterator, WebPDemuxer, WebPGetDemuxABIVersion,
    WebPGetFeatures, WebPIterator, WEBP_CSP_MODE,
};

#[derive(Clone, Debug, thiserror::Error)]
pub enum Error {
    #[error("VP8 error: {_0:?}")]
    VP8(VP8StatusCode),

    #[error("error demuxing")]
    Demux,

    #[error("no frames present")]
    NoFrames,
}

trait VP8StatusCodeExt {
    fn ok(self) -> Result<(), Error>;
}

struct Decoder<'a> {
    demuxer: NonNull<WebPDemuxer>,
    frame_iter: WebPIterator,
    _data: &'a [u8],
}

pub fn verify(data: &[u8]) -> Result<(), Error> {
    let mut config = WebPDecoderConfig::new().unwrap();
    config.options = WebPDecoderOptions {
        bypass_filtering: 1,
        no_fancy_upsampling: 1,
        use_cropping: 1,
        crop_left: 0,
        crop_top: 0,
        crop_width: 1,
        crop_height: 1,
        use_scaling: 0,
        scaled_width: 0,
        scaled_height: 0,
        use_threads: 1,
        dithering_strength: 0,
        flip: 0,
        alpha_dithering_strength: 0,
        pad: [0; 5],
    };
    let mut out_buf = [0; 4];
    config.output.colorspace = WEBP_CSP_MODE::MODE_ARGB;
    config.output.width = 1;
    config.output.height = 1;
    config.output.u.RGBA.rgba = out_buf.as_mut_ptr();
    config.output.u.RGBA.stride = 4;
    config.output.u.RGBA.size = 4;
    config.output.is_external_memory = 1;

    unsafe { WebPGetFeatures(data.as_ptr(), data.len(), &mut config.input).ok()? };

    if config.input.has_animation == 0 {
        unsafe { WebPDecode(data.as_ptr(), data.len(), &mut config).ok()? };
    }

    for frame in Decoder::new(data)? {
        let frame = frame?;
        unsafe { WebPDecode(frame.fragment.bytes, frame.fragment.size, &mut config).ok()? };
    }
    Ok(())
}

impl VP8StatusCodeExt for VP8StatusCode {
    fn ok(self) -> Result<(), Error> {
        match self {
            Self::VP8_STATUS_OK => Ok(()),
            _ => Err(Error::VP8(self)),
        }
    }
}

impl<'a> Decoder<'a> {
    pub fn new(data: &'a [u8]) -> Result<Self, Error> {
        unsafe {
            let webp_data = WebPData { bytes: data.as_ptr(), size: data.len() };

            let Some(demuxer) = NonNull::new(WebPDemuxInternal(&webp_data, 0, null_mut(), WebPGetDemuxABIVersion()))
            else {
                return Err(Error::Demux);
            };

            let mut frame_iter = MaybeUninit::uninit();
            if WebPDemuxGetFrame(demuxer.as_ptr(), 1, frame_iter.as_mut_ptr()) == 0 {
                return Err(Error::Demux);
            }

            Ok(Self { demuxer, frame_iter: frame_iter.assume_init(), _data: data })
        }
    }
}

impl<'a> Iterator for Decoder<'a> {
    type Item = Result<WebPIterator, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            if self.frame_iter.frame_num >= self.frame_iter.num_frames {
                return None;
            }
            if WebPDemuxNextFrame(&mut self.frame_iter) == 0 {
                return Some(Err(Error::Demux));
            }

            Some(Ok(self.frame_iter))
        }
    }
}

impl Drop for Decoder<'_> {
    fn drop(&mut self) {
        unsafe {
            WebPDemuxReleaseIterator(&mut self.frame_iter);
            WebPDemuxDelete(self.demuxer.as_ptr());
        }
    }
}
