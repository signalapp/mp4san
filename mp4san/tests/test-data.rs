use std::io::Cursor;

use mediasan_common_test::{init_logger, TestType};
use mp4san::sanitize;
use mp4san_test::{ffmpeg_assert_invalid, ffmpeg_assert_valid, gpac_assert_invalid, gpac_assert_valid};

#[test]
fn test_data() {
    init_logger();
    mediasan_common_test::test_data(".mp4", |test_type, data| match test_type {
        TestType::Valid => {
            sanitize(Cursor::new(data)).unwrap();
            ffmpeg_assert_valid(data);
            gpac_assert_valid(data);
        }
        TestType::InvalidPass => {
            sanitize(Cursor::new(data)).unwrap();
            ffmpeg_assert_invalid(data);
            gpac_assert_invalid(data);
        }
        TestType::InvalidFail => {
            sanitize(Cursor::new(data)).unwrap_err();
            ffmpeg_assert_invalid(data);
            gpac_assert_invalid(data);
        }
    });
}
