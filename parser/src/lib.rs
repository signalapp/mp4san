use crate as mp4san_isomparse;

pub use mp4san_isomparse_macros::Mp4Box;

pub trait Mp4Box {
    fn size(&self) -> u64;
}

#[derive(Mp4Box)]
pub struct NotARealBox {
    pub bar_ax: u64,
    pub foo_by: u32,
}

#[derive(Mp4Box)]
pub enum FakeEnumBox {
    A { foo: u32 },
    B(u64),
    C,
}

//impl Mp4Box for NotARealBox {
//    fn size(&self) -> u64 {
//        0 + size_of::<u64>() + size_of::<u32>()
//    }
//    //XXX: fn type_(&self) -> u32 | u128 { .. }
//    //XXX: and then the size() computation calls type_() to see whether it's 4 bytes or 20(?) bytes
//}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_1() {
        let not_a_real = NotARealBox {
            bar_ax: u64::MAX,
            foo_by: u32::MAX,
        };
        assert_eq!(not_a_real.size(), 4 + 8 + 4);
    }

    #[test]
    fn test_2() {
        let fake_enum = FakeEnumBox::A { foo: u32::MAX };
        assert_eq!(fake_enum.size(), 4 + 4);
    }

    #[test]
    fn test_3() {
        let fake_enum = FakeEnumBox::B(u64::MAX);
        assert_eq!(fake_enum.size(), 4 + 8);
    }

    #[test]
    fn test_4() {
        let fake_enum = FakeEnumBox::C;
        assert_eq!(fake_enum.size(), 4);
    }
}
