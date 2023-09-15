//! Common macros used by the `mediasan` crates.

#[macro_export]
/// Return an [`Err`] containing `$err` as a [`Report`](crate::error::Report) with optional `$attachment`s.
macro_rules! bail_attach {
    ($err:expr $(, $($attachment:expr),+)? $(,)?) => {
        return Err($crate::report_attach!($err $(, $($attachment),+)?))?
    };
}

#[macro_export]
/// Ensure `$cond` is `true`, or return an [`Err`] containing `$err` as a [`Report`](crate::error::Report) with optional
/// `$attachment`s.
macro_rules! ensure_attach {
    ($cond:expr, $err:expr $(, $($attachment:expr),+)? $(,)?) => {{
        let cond: bool = $cond;
        if !cond {
            $crate::bail_attach!($err, concat!("condition failed: ", stringify!($cond)) $(, $($attachment),+)?);
        }
    }};
}

#[macro_export]
/// Ensure `$expr` matches `$pat`, or return an [`Err`] containing `$err` as a [`Report`](crate::error::Report) with
/// optional `$attachment`s.
macro_rules! ensure_matches_attach {
    ($expr:expr, $pat:pat, $err:expr $(, $($attachment:expr),+)? $(,)?) => {
        let $pat = $expr else {
            $crate::bail_attach!($err, concat!("condition failed: let ", stringify!($pat), " = ", stringify!($expr))
                                 $(, $($attachment),+)?);
        };
    };
}

#[macro_export]
/// Create a [`Report`](crate::error::Report) contaning `$err` with optional `$attachment`s.
macro_rules! report_attach {
    ($err:expr $(, $($attachment:expr),+)? $(,)?) => {
        $crate::error::Report::from($err)
            $($(.attach_printable($attachment))+)?
    };
}
