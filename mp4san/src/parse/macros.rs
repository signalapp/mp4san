macro_rules! define_fourcc_lower {
    ($($name:ident),+ $(,)?) => {
        paste::paste! {
            $(
                #[doc = concat!("The `", stringify!([<$name:lower>]), "` FourCC code.")]
                pub const $name: $crate::parse::FourCC = $crate::parse::FourCC::from_str(stringify!([<$name:lower>]));
            )+
        }
    };
}
