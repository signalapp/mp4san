use std::fs;
use std::io;
use std::io::Cursor;
use std::io::Read;

use libflate::gzip;
use mp4san::sanitize;

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

const TEST_DATA_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/test-data/");

#[test]
fn test_data() {
    init_logger();
    let dir_entries = match fs::read_dir(TEST_DATA_DIR) {
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
            file_name_str if file_name_str.ends_with(".mp4.gz") => Some(gunzip(&fs::read(dir_entry.path()).unwrap())),
            _ => None,
        };
        if let Some(data) = data {
            log::info!("running test: {file_name:?}");
            sanitize(Cursor::new(&data[..])).unwrap();
        }
    }
}
