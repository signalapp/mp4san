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

#[test]
fn test_cve_2019_11931_poc() {
    init_logger();
    let data = gunzip(include_bytes!("test-data/cve_2019_11931_poc.mp4.gz"));
    sanitize(Cursor::new(&data[..])).unwrap();
}

#[test]
fn test_iphone_12_mini() {
    init_logger();
    let data = gunzip(include_bytes!("test-data/iphone_12_mini.mp4.gz"));
    sanitize(Cursor::new(&data[..])).unwrap();
}
