macro_rules! bail_attach {
    ($err:expr, $($attachment:expr),+ $(,)?) => {
        return Err(report_attach!($err, $($attachment),+)).map_err(Into::into)
    };
}

macro_rules! ensure_attach {
    ($cond:expr, $err:expr, $($attachment:expr),+ $(,)?) => {
        if !bool::from($cond) {
            bail_attach!($err, concat!("condition failed: ", stringify!($cond)), $($attachment),+);
        }
    };
}

macro_rules! report_attach {
    ($err:expr, $($attachment:expr),+ $(,)?) => {
        report!($err)
            $(.attach_printable($attachment))+
    };
}
