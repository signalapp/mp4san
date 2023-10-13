use std::task::{Context, Poll};
use std::{io, pin::Pin};

use bitstream_io::{HuffmanRead, LE};
use criterion::async_executor::FuturesExecutor;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use futures_util::AsyncRead;
use webpsan::parse::{BitBufReader, CanonicalHuffmanTree};
use webpsan::Error;

criterion_group!(benches, bitbufreader);
criterion_main!(benches);

struct BlackBoxEmptyInput;

impl AsyncRead for BlackBoxEmptyInput {
    fn poll_read(self: Pin<&mut Self>, _cx: &mut Context<'_>, _buf: &mut [u8]) -> Poll<io::Result<usize>> {
        black_box(Poll::Ready(Ok(0)))
    }
}

pub fn bitbufreader(c: &mut Criterion) {
    let mut read_huffman = c.benchmark_group("read_huffman");
    read_huffman.bench_function("sync", |b| {
        let code = CanonicalHuffmanTree::default();
        b.to_async(FuturesExecutor).iter(|| async {
            let mut reader = BitBufReader::<_, LE>::with_capacity(BlackBoxEmptyInput, 0);
            for _ in 0..1000 {
                black_box(reader.reader().read_huffman(code.read_tree()))?;
            }
            Ok::<_, Error>(())
        })
    });
    read_huffman.bench_function("async", |b| {
        let code = CanonicalHuffmanTree::default();
        b.to_async(FuturesExecutor).iter(|| async {
            let mut reader = BitBufReader::<_, LE>::with_capacity(BlackBoxEmptyInput, 0);
            for _ in 0..1000 {
                black_box(reader.read_huffman(&code).await)?;
            }
            Ok::<_, Error>(())
        })
    });
}
