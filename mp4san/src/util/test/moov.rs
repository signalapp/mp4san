use derive_builder::Builder;

use crate::parse::{Co64Box, FourCC, MdiaBox, MinfBox, MoovBox, Mp4Box, StblBox, StcoBox, TrakBox};

use super::{test_dinf, test_hdlr, test_mdhd, test_mvhd, test_stsc, test_stsd, test_stsz, test_stts, test_tkhd};

#[derive(Builder)]
#[builder(name = "TestMoovBuilder", build_fn(name = "build_spec"))]
pub struct TestMoovSpec {
    #[builder(default)]
    #[builder(setter(into, each(name = "add_co_entry")))]
    pub co_entries: Vec<u64>,

    #[builder(default = "true")]
    pub stco: bool,

    #[builder(default)]
    pub co64: bool,

    #[builder(default = "true")]
    pub stbl: bool,

    #[builder(default = "true")]
    pub minf: bool,

    #[builder(default = "true")]
    pub mdia: bool,

    #[builder(default = "true")]
    pub trak: bool,
}

impl TestMoovBuilder {
    pub fn build(&self) -> Mp4Box<MoovBox> {
        let spec = self.build_spec().unwrap();
        let chunk_count = spec.co_entries.len() as u32;

        let mut stbl = vec![test_stsd(), test_stts(chunk_count), test_stsc(), test_stsz(chunk_count)];
        if spec.co64 {
            let entries = spec.co_entries.iter().cloned();
            stbl.push(Mp4Box::with_data(Co64Box::with_entries(entries).into()).unwrap().into());
        }
        if spec.stco {
            let entries = spec.co_entries.into_iter().map(|entry| entry as u32);
            stbl.push(Mp4Box::with_data(StcoBox::with_entries(entries).into()).unwrap().into());
        }

        let mut minf = vec![test_dinf()];
        if spec.stbl {
            minf.push(Mp4Box::with_data(StblBox::with_children(stbl).into()).unwrap().into());
        }

        let mut mdia = vec![test_mdhd(), test_hdlr(FourCC::META)];
        if spec.minf {
            mdia.push(Mp4Box::with_data(MinfBox::with_children(minf).into()).unwrap().into());
        }

        let mut trak = vec![test_tkhd(1)];
        if spec.mdia {
            trak.push(Mp4Box::with_data(MdiaBox::with_children(mdia).into()).unwrap().into());
        }

        let mut moov = vec![test_mvhd()];
        if spec.trak {
            moov.push(Mp4Box::with_data(TrakBox::with_children(trak).into()).unwrap().into());
        }
        Mp4Box::with_data(MoovBox::with_children(moov).into()).unwrap()
    }
}
