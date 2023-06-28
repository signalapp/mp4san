use std::fs;
use std::io;
use std::io::Cursor;
use std::io::Read;

use libflate::gzip;
use mp4san::sanitize;
use mp4san_test::{ffmpeg_assert_invalid, ffmpeg_assert_valid, gpac_assert_invalid, gpac_assert_valid};

struct TestDirSpec {
    path: &'static str,
    test_type: TestType,
}

enum TestType {
    Valid,
    InvalidPass,
    InvalidFail,
}

fn init_logger() {
    // Ignore errors initializing the logger if tests race to configure it
    let _ignore = env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .parse_default_env()
        .is_test(true)
        .try_init();
}

fn gunzip(input: &[u8]) -> Vec<u8> {
    let mut decoder = gzip::Decoder::new(input).unwrap();
    let mut data = Vec::new();
    decoder.read_to_end(&mut data).unwrap();
    data
}

macro_rules! test_dir {
    ($name:literal, $test_type:ident) => {
        TestDirSpec {
            path: concat!(env!("CARGO_MANIFEST_DIR"), "/tests/test-data/", $name),
            test_type: TestType::$test_type,
        }
    };
}

const TEST_DATA_DIRS: &[TestDirSpec] = &[
    test_dir!("valid", Valid),
    test_dir!("/tests/test-data/invalid-pass", InvalidPass),
    test_dir!("/tests/test-data/invalid-fail", InvalidFail),
];

#[test]
fn test_data() {
    init_logger();
    for dir_spec in TEST_DATA_DIRS {
        let dir_entries = match fs::read_dir(dir_spec.path) {
            Ok(dir_entries) => dir_entries,
            Err(err) => match err.kind() {
                io::ErrorKind::NotFound => return,
                _ => panic!("could not read test data directory: {err}"),
            },
        };

        for dir_entry in dir_entries.map(Result::unwrap) {
            let file_name = dir_entry.file_name();
            let data = match file_name.to_string_lossy() {
                file_name_str if file_name_str.ends_with(".mp4") => Some(fs::read(dir_entry.path()).unwrap()),
                file_name_str if file_name_str.ends_with(".mp4.gz") => {
                    Some(gunzip(&fs::read(dir_entry.path()).unwrap()))
                }
                _ => None,
            };
            if let Some(data) = data {
                match dir_spec.test_type {
                    TestType::Valid => {
                        log::info!("running test on valid input: {file_name:?}");
                        sanitize(Cursor::new(&data[..])).unwrap();
                        ffmpeg_assert_valid(&data);
                        gpac_assert_valid(&data);
                    }
                    TestType::InvalidPass => {
                        log::info!("running test on invalid file (expecting sanitizer pass): {file_name:?}");
                        sanitize(Cursor::new(&data[..])).unwrap();
                        ffmpeg_assert_invalid(&data);
                        gpac_assert_invalid(&data);
                    }
                    TestType::InvalidFail => {
                        log::info!("running test on invalid file: {file_name:?}");
                        sanitize(Cursor::new(&data[..])).unwrap_err();
                        ffmpeg_assert_invalid(&data);
                        gpac_assert_invalid(&data);
                    }
                }
            }
        }
    }
}
