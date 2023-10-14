use std::io::Cursor;

use mediasan_common_test::{init_logger, TestType};
use webpsan::{sanitize_with_config, Config};

const CONFIG: Config = Config { allow_unknown_chunks: true };

#[test]
fn test_data() {
    init_logger();
    mediasan_common_test::test_data(".webp", |test_type, data| match test_type {
        TestType::Valid => {
            sanitize_with_config(Cursor::new(data), CONFIG).unwrap();
        }
        TestType::InvalidPass => {
            sanitize_with_config(Cursor::new(data), CONFIG).unwrap();
        }
        TestType::InvalidFail => {
            dbg!(sanitize_with_config(Cursor::new(data), CONFIG).unwrap_err());
        }
    });
}
