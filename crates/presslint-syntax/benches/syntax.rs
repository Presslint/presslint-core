#![allow(missing_docs)]

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use presslint_syntax::{
    assemble_operators, serialize_tokens_unmodified, serialize_unmodified, tokenize,
};

const SMALL_STREAM: &[u8] = br"
q
0.1 0.2 0.3 rg
10 20 30 40 re f
BT /F1 12 Tf (Hello) Tj ET
/Im1 Do
/GS1 gs
Q
";

const REPEATED_STREAM_UNIT: &[u8] = br"
q
1 0 0 1 2 3 cm
0.1 0.2 0.3 rg
0.4 G
10 20 m 30 40 l S
5 5 15 15 re f
BT 0 Tr (Synthetic text) Tj [(A) 120 (B)] TJ ET
/Im1 Do
/Fm1 Do
/GS1 gs
n
Q
";

fn repeated_stream(repetitions: usize) -> Vec<u8> {
    let mut stream = Vec::with_capacity(REPEATED_STREAM_UNIT.len() * repetitions);
    for _ in 0..repetitions {
        stream.extend_from_slice(REPEATED_STREAM_UNIT);
    }
    stream
}

fn syntax_streams() -> [(&'static str, Vec<u8>); 2] {
    [
        ("small_mixed_bytes", SMALL_STREAM.to_vec()),
        ("large_repeated_bytes", repeated_stream(128)),
    ]
}

fn require_ok<T, E: core::fmt::Debug>(result: Result<T, E>, context: &str) -> T {
    match result {
        Ok(value) => value,
        Err(error) => {
            eprintln!("{context}: {error:?}");
            std::process::abort();
        }
    }
}

fn throughput_count(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn tokenize_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("syntax/tokenize");
    for (name, stream) in syntax_streams() {
        group.throughput(Throughput::Bytes(throughput_count(stream.len())));
        group.bench_with_input(BenchmarkId::from_parameter(name), &stream, |b, input| {
            b.iter(|| require_ok(tokenize(black_box(input)), "synthetic stream tokenizes"));
        });
    }
    group.finish();
}

fn assemble_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("syntax/assemble_operators");
    for (name, stream) in syntax_streams() {
        let tokens = require_ok(tokenize(&stream), "synthetic stream tokenizes");
        group.throughput(Throughput::Elements(throughput_count(tokens.len())));
        group.bench_with_input(BenchmarkId::from_parameter(name), &tokens, |b, input| {
            b.iter(|| {
                require_ok(
                    assemble_operators(black_box(input)),
                    "synthetic stream assembles",
                )
            });
        });
    }
    group.finish();
}

fn serialize_throughput(c: &mut Criterion) {
    {
        let mut group = c.benchmark_group("syntax/serialize_unmodified");
        for (name, stream) in syntax_streams() {
            let tokens = require_ok(tokenize(&stream), "synthetic stream tokenizes");
            group.throughput(Throughput::Bytes(throughput_count(stream.len())));
            group.bench_with_input(
                BenchmarkId::from_parameter(format!("{name}/from_tokens")),
                &(stream, tokens),
                |b, (source, tokens)| {
                    b.iter(|| {
                        require_ok(
                            serialize_tokens_unmodified(black_box(source), black_box(tokens)),
                            "synthetic stream serializes",
                        )
                    });
                },
            );
        }
        group.finish();
    }

    let mut group = c.benchmark_group("syntax/serialize_unmodified_parse");
    for (name, stream) in syntax_streams() {
        group.throughput(Throughput::Bytes(throughput_count(stream.len())));
        group.bench_with_input(BenchmarkId::from_parameter(name), &stream, |b, input| {
            b.iter(|| serialize_unmodified(black_box(input)));
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    tokenize_throughput,
    assemble_throughput,
    serialize_throughput
);
criterion_main!(benches);
