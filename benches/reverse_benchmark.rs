use criterion::{black_box, criterion_group, criterion_main, Criterion};
use skywatcher_rs::str_24bits_to_u32;

fn reverse_24bits_str_benchmark(c: &mut Criterion) {
    let test_str = black_box(String::from("a29701"));
    c.bench_function("convert 24bits str representation to u32", |b| {
        b.iter(|| str_24bits_to_u32(test_str.clone()))
    });
}

criterion_group!(benches, reverse_24bits_str_benchmark);
criterion_main!(benches);
