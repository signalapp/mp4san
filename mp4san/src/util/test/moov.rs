use bytes::BytesMut;
use derive_builder::Builder;

use crate::parse::{
    fourcc, AnyMp4Box, BoxData, Boxes, Co64Box, MdiaBox, MinfBox, MoovBox, Mp4Box, Mp4Value, ParseBox, ParsedBox,
    StblBox, StcoBox, TrakBox,
};

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
            stbl.push(Mp4Box::with_data(Co64Box::from_iter(entries).into()).unwrap().into());
        }
        if spec.stco {
            let entries = spec.co_entries.into_iter().map(|entry| entry as u32);
            stbl.push(Mp4Box::with_data(StcoBox::from_iter(entries).into()).unwrap().into());
        }

        let mut minf = vec![test_dinf()];
        if spec.stbl {
            let stbl: Mp4Box<StblBox> = container(stbl);
            minf.push(stbl.into());
        }

        let mut mdia = vec![test_mdhd(), test_hdlr(fourcc::META)];
        if spec.minf {
            let minf: Mp4Box<MinfBox> = container(minf);
            mdia.push(minf.into());
        }

        let mut trak = vec![test_tkhd(1)];
        if spec.mdia {
            let mdia: Mp4Box<MdiaBox> = container(mdia);
            trak.push(mdia.into());
        }

        let mut moov = vec![test_mvhd()];
        if spec.trak {
            let trak: Mp4Box<TrakBox> = container(trak);
            moov.push(trak.into());
        }
        container(moov)
    }
}

fn container<T: ParseBox + ParsedBox, I: IntoIterator>(boxes: I) -> Mp4Box<T>
where
    AnyMp4Box: From<I::Item>,
{
    let mut data = BytesMut::new();
    let boxes = boxes.into_iter().map(AnyMp4Box::from).collect::<Vec<_>>();
    Boxes::<()>::try_from(boxes).unwrap().put_buf(&mut data);
    Mp4Box::with_data(BoxData::Bytes(data.into())).unwrap()
}
