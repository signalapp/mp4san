# mediasan &emsp; [![Fuzzing Status](https://oss-fuzz-build-logs.storage.googleapis.com/badges/mp4san.svg)](https://oss-fuzz.com/coverage-report/job/libfuzzer_asan_mp4san/latest)

A collection of Rust media file format "sanitizers".

The sanitizers can be used to verify the validity of media files before presenting them, so that passing malformed files
to an unsafe parser can be avoided.

## Supported Formats

| Format | Crate   |         |
|--------|---------|:-------:|
| [MP4]  | [`mp4san`]  | [![crates.io](https://img.shields.io/crates/v/mp4san.svg)](https://crates.io/crates/mp4san) [![Documentation](https://docs.rs/mp4san/badge.svg)](https://docs.rs/mp4san) 
| [WebP] | [`webpsan`] | [![crates.io](https://img.shields.io/crates/v/webpsan.svg)](https://crates.io/crates/webpsan) [![Documentation](https://docs.rs/webpsan/badge.svg)](https://docs.rs/webpsan) 

[MP4]: https://en.wikipedia.org/wiki/MP4_file_format
[`mp4san`]: ./mp4san
[WebP]: https://developers.google.com/speed/webp
[`webpsan`]: ./webpsan

## Contributing Bug Reports

GitHub is the project's bug tracker. Please [search](https://github.com/privacyresearchgroup/mp4san/issues) for similar
existing issues before [submitting a new one](https://github.com/privacyresearchgroup/mp4san/issues/new).

### OSS-Fuzz

Continuous fuzz testing is also provided by [OSS-Fuzz](https://google.github.io/oss-fuzz/).

[Build Status](https://oss-fuzz-build-logs.storage.googleapis.com/index.html#mp4san)  
[Code Coverage](https://oss-fuzz.com/coverage-report/job/libfuzzer_asan_mp4san/latest)  
[Bugs Found](https://bugs.chromium.org/p/oss-fuzz/issues/list?sort=-opened&can=1&q=proj:mp4san)  

## License

Licensed under [MIT](https://opensource.org/licenses/MIT).
