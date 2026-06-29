#![allow(missing_docs)]

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use presslint_pdf::inspect_classic_xref_table;

fn synthetic_xref_table(entries: u32) -> Vec<u8> {
    let mut source = Vec::new();
    source.extend_from_slice(b"xref\n0 ");
    source.extend_from_slice(entries.to_string().as_bytes());
    source.extend_from_slice(b"\n");

    for object_number in 0..entries {
        if object_number == 0 {
            source.extend_from_slice(b"0000000000 65535 f \n");
        } else {
            let byte_offset = object_number.saturating_mul(37);
            let line = format!("{byte_offset:010} 00000 n \n");
            source.extend_from_slice(line.as_bytes());
        }
    }

    source.extend_from_slice(b"trailer\n<< /Size ");
    source.extend_from_slice(entries.to_string().as_bytes());
    source.extend_from_slice(b" >>\n");
    source
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

fn classic_xref_table_throughput(c: &mut Criterion) {
    let cases = [
        ("small_32_entries", synthetic_xref_table(32)),
        ("large_1024_entries", synthetic_xref_table(1024)),
    ];

    let mut group = c.benchmark_group("pdf_source/inspect_classic_xref_table");
    for (name, source) in cases {
        group.throughput(Throughput::Bytes(throughput_count(source.len())));
        group.bench_with_input(BenchmarkId::from_parameter(name), &source, |b, input| {
            b.iter(|| {
                require_ok(
                    inspect_classic_xref_table(black_box(input), 0),
                    "synthetic xref table inspects",
                )
            });
        });
    }
    group.finish();
}

criterion_group!(benches, classic_xref_table_throughput);
criterion_main!(benches);
