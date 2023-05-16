macro_rules! bail_attach {
    ($err:expr $(, $($attachment:expr),+)? $(,)?) => {
        return Err(report_attach!($err $(, $($attachment),+)?).into())
    };
}

macro_rules! ensure_attach {
    ($cond:expr, $err:expr $(, $($attachment:expr),+)? $(,)?) => {{
        let cond: bool = $cond;
        if !cond {
            bail_attach!($err, concat!("condition failed: ", stringify!($cond)) $(, $($attachment),+)?);
        }
    }};
}

macro_rules! report_attach {
    ($err:expr $(, $($attachment:expr),+)? $(,)?) => {
        $crate::error::Report::from($err)
            $($(.attach_printable($attachment))+)?
    };
}
