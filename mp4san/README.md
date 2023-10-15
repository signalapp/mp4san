# mp4san &emsp; [![Fuzzing Status](https://oss-fuzz-build-logs.storage.googleapis.com/badges/mp4san.svg)](https://oss-fuzz.com/coverage-report/job/libfuzzer_asan_mp4san/latest)

A Rust MP4 format "sanitizer".

Currently the sanitizer is capable of:

- Returning all presentation metadata present in the input as a self-contained contiguous byte array.
- Finding and returning a pointer to the span in the input containing the (contiguous) media data.

"Presentation" metadata means any metadata which is required by an MP4 player to play the file. "Self-contained and
contiguous" means that the returned metadata can be concatenated with the media data to form a valid MP4 file.

## Unsupported MP4 features

The sanitizer does not currently support:

- "Fragmented" MP4 files, which are mostly used for adaptive-bitrate streaming.
- Discontiguous media data, i.e. media data (`mdat`) boxes interspersed with presentation metadata (`moov`).
- Media data references (`dref`) pointing to separate files.
- Any similar format, e.g. Quicktime File Format (`mov`) or the legacy MP4 version 1, which does not contain the `isom`
  compatible brand in its file type header (`ftyp`).

## Usage

The main entry points to the sanitizer are [`sanitize`]/[`sanitize_async`], which take a [`Read`] + [`Skip`] input. The
[`Skip`] trait represents a subset of the [`Seek`] trait; an input stream which can be skipped forward, but not
necessarily seeked to arbitrary positions.

```rust
use mp4san_test::{example_ftyp, example_mdat, example_moov};

let example_input = [example_ftyp(), example_mdat(), example_moov()].concat();

let sanitized = mp4san::sanitize(std::io::Cursor::new(example_input)).unwrap();

assert_eq!(sanitized.metadata, Some([example_ftyp(), example_moov()].concat()));
assert_eq!(sanitized.data.offset, example_ftyp().len() as u64);
assert_eq!(sanitized.data.len, example_mdat().len() as u64);
```

The [`parse`] module also contains a less stable and undocumented API which can be used to parse individual MP4 box
types.

[API Documentation](https://privacyresearchgroup.github.io/mp4san/public/mp4san/)  
[Private Documentation](https://privacyresearchgroup.github.io/mp4san/private/mp4san/)  

[`sanitize`]: https://privacyresearchgroup.github.io/mp4san/public/mp4san/fn.sanitize.html
[`sanitize_async`]: https://privacyresearchgroup.github.io/mp4san/public/mp4san/fn.sanitize_async.html
[`Read`]: https://doc.rust-lang.org/std/io/trait.Read.html
[`Skip`]: https://privacyresearchgroup.github.io/mp4san/public/mediasan_common/trait.Skip.html
[`Seek`]: https://doc.rust-lang.org/std/io/trait.Seek.html
[`parse`]: https://privacyresearchgroup.github.io/mp4san/public/mp4san/parse/index.html

## Contributing Bug Reports

GitHub is the project's bug tracker. Please [search](https://github.com/privacyresearchgroup/mp4san/issues) for similar
existing issues before [submitting a new one](https://github.com/privacyresearchgroup/mp4san/issues/new).

## Testing

FFMpeg and GPAC-based verification of mp4san output can be enabled using the features `mp4san-test/ffmpeg` and
`mp4san-test/gpac`.

The `mp4san-test/ffmpeg` feature requires the following FFMpeg libraries and their headers to be installed:

- `libavcodec`
- `libavformat`
- `libavutil`
- `libswresample`
- `libswscale`

The `mp4san-test/gpac` feature requires `libgpac >= 2.2` and its headers to be installed.

The [`test_data`](tests/test-data.rs) integration test runs on sample data files in the private
[`test-data`](../test-data) submodule. If you have access to this repo, you may check out the submodule manually:

```shell
$ git submodule update --init --checkout
```

Integration tests on sample data files can be processed through `mp4san-test-gen` before being added to the
`mp4san-test-data` repo. This removes any actual media data from the sample file, since it's not read by `mp4san`
anyway, leaving only metadata for testing purposes. This allows even very large media files to be gzipped to very small
sizes.

```shell
$ cargo run --bin mp4san-test-gen -- test-sample.mp4 test-data/test-sample.mp4.gz
```

### Fuzz Testing

Fuzz testing via both `cargo afl` and `cargo fuzz` is supported. See [the Rust Fuzz Book](https://rust-fuzz.github.io/book/) for more details. To run AFL-based fuzzing:

```shell
$ cargo install cargo-afl
$ cd fuzz-afl
$ ./fuzz $num_cpus
```

To run libFuzzer-based fuzzing:

```shell
$ cargo +nightly install cargo-fuzz
$ cargo +nightly fuzz run sanitize -- -dict=fuzz/mp4.dict -seed_inputs=fuzz/input/ffmpeg-black-1f.mp4,fuzz/input/ffmpeg-smptebars-30f.mp4
```

### OSS-Fuzz

Continuous fuzz testing is also provided by [OSS-Fuzz](https://google.github.io/oss-fuzz/).

[Build Status](https://oss-fuzz-build-logs.storage.googleapis.com/index.html#mp4san)  
[Code Coverage](https://oss-fuzz.com/coverage-report/job/libfuzzer_asan_mp4san/latest)  
[Bugs Found](https://bugs.chromium.org/p/oss-fuzz/issues/list?sort=-opened&can=1&q=proj:mp4san)  

## License

Licensed under [MIT](https://opensource.org/licenses/MIT).
