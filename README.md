# mp4san

A Rust MP4 file sanitizer.

[API Documentation](https://privacyresearchgroup.github.io/mp4san/public/mp4san/)  
[Private Documentation](https://privacyresearchgroup.github.io/mp4san/private/mp4san/)  

## Contributing Bug Reports

GitHub is the project's bug tracker. Please [search](https://github.com/privacyresearchgroup/mp4san/issues) for similar
existing issues before [submitting a new one](https://github.com/privacyresearchgroup/mp4san/issues/new).

## Testing

Integration tests on sample data files can be processed through `mp4san-test-gen` before being added to the
`mp4san-test-data` repo. This removes any actual media data from the sample file, since it's not read by `mp4san`
anyway, leaving only metadata for testing purposes. This allows even very large media files to be gzipped to very small
sizes.

```
$ cargo run --bin mp4san-test-gen -- test-sample.mp4 mp4san/tests/test-data/test-sample.mp4.gz
```

## License

Licensed under [MIT](https://opensource.org/licenses/MIT).
