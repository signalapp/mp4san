use derive_builder::Builder;

use crate::parse::{FourCC, FtypBox, Mp4Box};

use super::ISOM;

#[derive(Builder)]
#[builder(name = "TestFtypBuilder", build_fn(name = "build_spec"))]
pub struct TestFtypSpec {
    #[builder(default = "ISOM")]
    major_brand: FourCC,

    #[builder(default)]
    minor_version: u32,

    #[builder(default = "vec![ISOM]")]
    #[builder(setter(each(name = "add_compatible_brand")))]
    compatible_brands: Vec<FourCC>,
}

impl TestFtypBuilder {
    pub fn build(&self) -> Mp4Box<FtypBox> {
        let spec = self.build_spec().unwrap();

        Mp4Box::with_data(FtypBox::new(spec.major_brand, spec.minor_version, spec.compatible_brands).into()).unwrap()
    }
}
