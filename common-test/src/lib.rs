use std::fs;
use std::io;
use std::io::Read;

use libflate::gzip;

//
// public types
//

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TestType {
    Valid,
    InvalidPass,
    InvalidFail,
}

//
// private types
//

struct TestDirSpec {
    path: &'static str,
    test_type: TestType,
}

fn gunzip(input: &[u8]) -> Vec<u8> {
    let mut decoder = gzip::Decoder::new(input).unwrap();
    let mut data = Vec::new();
    decoder.read_to_end(&mut data).unwrap();
    data
}

macro_rules! test_dir {
    ($name:literal, $test_type:ident) => {
        $crate::TestDirSpec {
            path: concat!(env!("CARGO_MANIFEST_DIR"), "/../test-data/", $name),
            test_type: TestType::$test_type,
        }
    };
}

const TEST_DATA_DIRS: &[TestDirSpec] = &[
    test_dir!("valid", Valid),
    test_dir!("invalid-pass", InvalidPass),
    test_dir!("invalid-fail", InvalidFail),
];

//
// public functions
//

pub fn init_logger() {
    // Ignore errors initializing the logger if tests race to configure it
    let _ignore = env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .parse_default_env()
        .is_test(true)
        .try_init();
}

pub fn test_data<F: FnMut(TestType, &[u8])>(ext: &str, mut sanitize: F) {
    init_logger();
    let ext_gz = ext.to_string() + ".gz";
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
                file_name_str if file_name_str.ends_with(ext) => Some(fs::read(dir_entry.path()).unwrap()),
                file_name_str if file_name_str.ends_with(&ext_gz) => Some(gunzip(&fs::read(dir_entry.path()).unwrap())),
                _ => None,
            };
            if let Some(data) = data {
                match dir_spec.test_type {
                    TestType::Valid => log::info!("running test on valid input: {file_name:?}"),
                    TestType::InvalidPass => {
                        log::info!("running test on invalid file (expecting sanitizer pass): {file_name:?}");
                    }
                    TestType::InvalidFail => log::info!("running test on invalid file: {file_name:?}"),
                }
                sanitize(dir_spec.test_type, &data[..]);
            }
        }
    }
}
