use std::task::{Context, Poll};
use std::{io, pin::Pin};

use bitstream_io::LE;
use criterion::async_executor::FuturesExecutor;
use criterion::measurement::Measurement;
use criterion::{black_box, criterion_group, criterion_main, BatchSize, BenchmarkGroup, Criterion};
use futures_util::AsyncRead;
use webpsan::parse::{BitBufReader, CanonicalHuffmanTree};
use webpsan::Error;

criterion_group!(
    benches,
    read_huffman_one_symbol,
    read_huffman_two_symbols,
    read_huffman_256_symbols
);
criterion_main!(benches);

struct BlackBoxZeroesInput;

impl AsyncRead for BlackBoxZeroesInput {
    fn poll_read(self: Pin<&mut Self>, _cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        black_box(Poll::Ready(Ok(buf.len())))
    }
}

pub fn read_huffman_one_symbol(c: &mut Criterion) {
    let group = c.benchmark_group("one symbol");
    let code = CanonicalHuffmanTree::<LE, ()>::default();
    read_huffman(group, &code);
}

pub fn read_huffman_two_symbols(c: &mut Criterion) {
    let group = c.benchmark_group("two symbols");
    let code = CanonicalHuffmanTree::new(&mut [((), 1); 2]).unwrap();
    read_huffman(group, &code);
}

pub fn read_huffman_256_symbols(c: &mut Criterion) {
    let group = c.benchmark_group("256 symbols");
    let code = CanonicalHuffmanTree::new(&mut [((), 8); 256]).unwrap();
    read_huffman(group, &code);
}

fn read_huffman<M: Measurement, S: Clone>(mut group: BenchmarkGroup<'_, M>, code: &CanonicalHuffmanTree<LE, S>) {
    let buf_len = 4096;
    let setup = || BitBufReader::<_, LE>::with_capacity(BlackBoxZeroesInput, buf_len);
    group.throughput(criterion::Throughput::Bytes(buf_len as u64));
    group.bench_function("buf_read_huffman", |bencher| {
        bencher.to_async(FuturesExecutor).iter_batched(
            setup,
            |mut reader| async move {
                if code.longest_code_len() != 0 {
                    reader.fill_buf().await?;
                }
                for _ in 0..buf_len * 8 {
                    black_box(reader.buf_read_huffman(code))?;
                }
                Ok::<_, Error>(())
            },
            BatchSize::SmallInput,
        )
    });
    group.bench_function("read_huffman", |bencher| {
        bencher.to_async(FuturesExecutor).iter_batched(
            setup,
            |mut reader| async move {
                for _ in 0..buf_len * 8 {
                    black_box(reader.read_huffman(code).await)?;
                }
                Ok::<_, Error>(())
            },
            BatchSize::SmallInput,
        )
    });
    group.finish();
}
