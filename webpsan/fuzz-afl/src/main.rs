use std::io;

fn main() {
    afl::fuzz!(|data: &[u8]| {
        std::panic::set_hook(Box::new(|panic| {
            eprintln!("{panic}");
            std::process::abort();
        }));
        match webpsan::sanitize(io::Cursor::new(data)) {
            Ok(sanitized) => {
                eprintln!("webpsan returned ok");
            }
            Err(error) => match error {
                webpsan::Error::Io(error) => match error.kind() {
                    io::ErrorKind::InvalidData => {
                        eprintln!("webpsan returned an io error: {error}\n{error:?}");
                    }
                    _ => panic!(),
                },
                webpsan::Error::Parse(error) => {
                    eprintln!("webpsan returned a parse error: {error}\n{error:?}");
                }
            },
        }
    });
}
