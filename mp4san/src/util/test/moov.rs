use derive_builder::Builder;

use crate::parse::{Co64Box, MdiaBox, MinfBox, MoovBox, Mp4Box, StblBox, StcoBox, TrakBox};

#[derive(Builder)]
#[builder(name = "TestMoovBuilder", build_fn(name = "build_spec"))]
pub struct TestMoovSpec {
    #[builder(default)]
    #[builder(setter(into))]
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

        let mut co = vec![];
        if spec.co64 {
            let entries = spec.co_entries.iter().cloned();
            co.push(Mp4Box::with_data(Co64Box::with_entries(entries).into()).unwrap().into());
        }
        if spec.stco {
            let entries = spec.co_entries.into_iter().map(|entry| entry as u32);
            co.push(Mp4Box::with_data(StcoBox::with_entries(entries).into()).unwrap().into());
        }
        let stbl = match spec.stbl {
            true => vec![Mp4Box::with_data(StblBox::with_children(co).into()).unwrap().into()],
            false => vec![],
        };
        let minf = match spec.minf {
            true => vec![Mp4Box::with_data(MinfBox::with_children(stbl).into()).unwrap().into()],
            false => vec![],
        };
        let mdia = match spec.mdia {
            true => vec![Mp4Box::with_data(MdiaBox::with_children(minf).into()).unwrap().into()],
            false => vec![],
        };
        let trak = match spec.trak {
            true => vec![Mp4Box::with_data(TrakBox::with_children(mdia).into()).unwrap().into()],
            false => vec![],
        };
        Mp4Box::with_data(MoovBox::with_children(trak).into()).unwrap()
    }
}
