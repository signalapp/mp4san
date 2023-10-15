# webpsan &emsp; [![Fuzzing Status](https://oss-fuzz-build-logs.storage.googleapis.com/badges/mp4san.svg)](https://oss-fuzz.com/coverage-report/job/libfuzzer_asan_mp4san/latest)

A Rust WebP format "sanitizer".

The sanitizer currently simply checks the validity of a WebP file input, so that passing malformed files to an unsafe
parser can be avoided.

## Usage

The main entry points to the sanitizer are [`sanitize`]/[`sanitize_async`], which take a [`Read`] + [`Skip`] input. The
[`Skip`] trait represents a subset of the [`Seek`] trait; an input stream which can be skipped forward, but not
necessarily seeked to arbitrary positions.

```rust
let example_input = b"RIFF\x14\0\0\0WEBPVP8L\x08\0\0\0\x2f\0\0\0\0\x88\x88\x08";
webpsan::sanitize(std::io::Cursor::new(example_input)).unwrap();
```

The [`parse`] module also contains a less stable and undocumented API which can be used to parse individual WebP chunk
types.

[API Documentation](https://privacyresearchgroup.github.io/mp4san/public/webpsan/)  
[Private Documentation](https://privacyresearchgroup.github.io/mp4san/private/webpsan/)  

[`sanitize`]: https://privacyresearchgroup.github.io/mp4san/public/webpsan/fn.sanitize.html
[`sanitize_async`]: https://privacyresearchgroup.github.io/mp4san/public/webpsan/fn.sanitize_async.html
[`Read`]: https://doc.rust-lang.org/std/io/trait.Read.html
[`Skip`]: https://privacyresearchgroup.github.io/mp4san/public/mediasan_common/trait.Skip.html
[`Seek`]: https://doc.rust-lang.org/std/io/trait.Seek.html
[`parse`]: https://privacyresearchgroup.github.io/mp4san/public/webpsan/parse/index.html

## Contributing Bug Reports

GitHub is the project's bug tracker. Please [search](https://github.com/privacyresearchgroup/mp4san/issues) for similar
existing issues before [submitting a new one](https://github.com/privacyresearchgroup/mp4san/issues/new).

## Testing

`libwebp`-based verification of webpsan tests can be enabled using the `webpsan-test/libwebp` feature. `libwebp` is
linked statically, so does not need to be installed for the tests.

The [`test_data`](tests/test-data.rs) integration test runs on sample data files in the private
[`test-data`](../test-data) submodule. If you have access to this repo, you may check out the submodule manually:

```shell
$ git submodule update --init --checkout
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
$ cargo +nightly fuzz run sanitize -- -dict=fuzz/webp.dict -seed_inputs=fuzz/input/smallest-possible.webp
```

### OSS-Fuzz

Continuous fuzz testing is also provided by [OSS-Fuzz](https://google.github.io/oss-fuzz/).

[Build Status](https://oss-fuzz-build-logs.storage.googleapis.com/index.html#mp4san)  
[Code Coverage](https://oss-fuzz.com/coverage-report/job/libfuzzer_asan_mp4san/latest)  
[Bugs Found](https://bugs.chromium.org/p/oss-fuzz/issues/list?sort=-opened&can=1&q=proj:mp4san)  

## License

Licensed under [MIT](https://opensource.org/licenses/MIT).
