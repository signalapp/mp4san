/// Checked addition with a signed integer. Computes `lhs + rhs`, returning `None` if overflow occurred.
///
/// This is the unstable `<int>::checked_add_signed`.
pub fn checked_add_signed<Lhs: CheckedAddSigned>(lhs: Lhs, rhs: Lhs::Rhs) -> Option<Lhs> {
    lhs.checked_add_signed(rhs)
}

pub trait CheckedAddSigned: Sized {
    type Rhs;
    fn checked_add_signed(self, rhs: Self::Rhs) -> Option<Self>;
}

macro_rules! impl_checked_add_signed {
    ($lhs:ty, $rhs:ty) => {
        impl CheckedAddSigned for $lhs {
            type Rhs = $rhs;

            fn checked_add_signed(self, rhs: Self::Rhs) -> Option<Self> {
                let (result, overflowed) = self.overflowing_add(rhs as Self);
                if overflowed ^ (rhs < 0) {
                    None
                } else {
                    Some(result)
                }
            }
        }
    };
}

impl_checked_add_signed!(u8, i8);
impl_checked_add_signed!(u16, i16);
impl_checked_add_signed!(u32, i32);
impl_checked_add_signed!(u64, i64);
impl_checked_add_signed!(u128, i128);
impl_checked_add_signed!(usize, isize);

#[cfg(test)]
pub mod test {
    pub fn init_logger() {
        // Ignore errors initializing the logger if tests race to configure it
        let _ignore = env_logger::builder()
            .filter_level(log::LevelFilter::Info)
            .parse_default_env()
            .is_test(true)
            .try_init();
    }
}
