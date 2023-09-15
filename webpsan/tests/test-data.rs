use std::io::Cursor;

use mediasan_common_test::{init_logger, TestType};
use webpsan::sanitize;

#[test]
fn test_data() {
    init_logger();
    mediasan_common_test::test_data(".webp", |test_type, data| match test_type {
        TestType::Valid => {
            sanitize(Cursor::new(data)).unwrap();
        }
        TestType::InvalidPass => {
            sanitize(Cursor::new(data)).unwrap();
        }
        TestType::InvalidFail => {
            dbg!(sanitize(Cursor::new(data)).unwrap_err());
        }
    });
}
