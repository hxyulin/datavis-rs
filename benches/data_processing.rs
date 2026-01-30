//! Benchmarks for data processing operations
//!
//! Run with: cargo bench

#![allow(dead_code, clippy::approx_constant)] // Benchmark code may have unused fields and test values

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::collections::VecDeque;
use std::time::Duration;

// Simulate the DataPoint structure
#[derive(Clone)]
struct DataPoint {
    timestamp: Duration,
    raw_value: f64,
    converted_value: f64,
}

impl DataPoint {
    fn new(timestamp: Duration, value: f64) -> Self {
        Self {
            timestamp,
            raw_value: value,
            converted_value: value,
        }
    }
}

// Simulate VariableData structure
struct VariableData {
    data_points: VecDeque<DataPoint>,
}

impl VariableData {
    fn new(capacity: usize) -> Self {
        Self {
            data_points: VecDeque::with_capacity(capacity),
        }
    }

    fn push(&mut self, point: DataPoint, max_points: usize) {
        if self.data_points.len() >= max_points {
            self.data_points.pop_front();
        }
        self.data_points.push_back(point);
    }

    fn as_plot_points(&self) -> Vec<[f64; 2]> {
        self.data_points
            .iter()
            .map(|dp| [dp.timestamp.as_secs_f64(), dp.converted_value])
            .collect()
    }
}

fn bench_data_point_insertion(c: &mut Criterion) {
    let mut group = c.benchmark_group("data_point_insertion");

    for size in [1000, 10_000, 100_000].iter() {
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(BenchmarkId::new("push", size), size, |b, &size| {
            let mut data = VariableData::new(size);
            let mut i = 0u64;
            b.iter(|| {
                let point = DataPoint::new(Duration::from_micros(i), i as f64);
                data.push(black_box(point), size);
                i = i.wrapping_add(1);
            });
        });
    }

    group.finish();
}

fn bench_plot_points_conversion(c: &mut Criterion) {
    let mut group = c.benchmark_group("plot_points_conversion");

    for size in [1000, 10_000, 50_000].iter() {
        // Pre-fill with data
        let mut data = VariableData::new(*size);
        for i in 0..*size as u64 {
            data.push(DataPoint::new(Duration::from_micros(i), i as f64), *size);
        }

        group.throughput(Throughput::Elements(*size as u64));
        group.bench_with_input(
            BenchmarkId::new("as_plot_points", size),
            &data,
            |b, data| {
                b.iter(|| black_box(data.as_plot_points()));
            },
        );
    }

    group.finish();
}

fn bench_value_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("value_parsing");

    // Benchmark parsing different types from bytes
    let bytes_u32: [u8; 4] = 1000u32.to_le_bytes();
    let bytes_f32: [u8; 4] = 3.14159f32.to_le_bytes();
    let bytes_u64: [u8; 8] = 1_000_000u64.to_le_bytes();
    let bytes_f64: [u8; 8] = 3.14159265358979f64.to_le_bytes();

    group.bench_function("parse_u32", |b| {
        b.iter(|| black_box(u32::from_le_bytes(bytes_u32) as f64));
    });

    group.bench_function("parse_f32", |b| {
        b.iter(|| black_box(f32::from_le_bytes(bytes_f32) as f64));
    });

    group.bench_function("parse_u64", |b| {
        b.iter(|| black_box(u64::from_le_bytes(bytes_u64) as f64));
    });

    group.bench_function("parse_f64", |b| {
        b.iter(|| black_box(f64::from_le_bytes(bytes_f64)));
    });

    group.finish();
}

fn bench_ring_buffer_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("ring_buffer");

    let max_size = 10_000;
    let mut data = VariableData::new(max_size);

    // Pre-fill to max capacity
    for i in 0..max_size as u64 {
        data.push(DataPoint::new(Duration::from_micros(i), i as f64), max_size);
    }

    group.bench_function("push_at_capacity", |b| {
        let mut i = max_size as u64;
        b.iter(|| {
            let point = DataPoint::new(Duration::from_micros(i), i as f64);
            data.push(black_box(point), max_size);
            i = i.wrapping_add(1);
        });
    });

    group.bench_function("iter_all", |b| {
        b.iter(|| {
            let sum: f64 = data.data_points.iter().map(|p| p.converted_value).sum();
            black_box(sum)
        });
    });

    group.bench_function("find_min_max", |b| {
        b.iter(|| {
            let mut min = f64::INFINITY;
            let mut max = f64::NEG_INFINITY;
            for p in &data.data_points {
                min = min.min(p.converted_value);
                max = max.max(p.converted_value);
            }
            black_box((min, max))
        });
    });

    group.finish();
}

fn bench_statistics_calculation(c: &mut Criterion) {
    let mut group = c.benchmark_group("statistics");

    for size in [1000, 10_000, 50_000].iter() {
        let mut data = VariableData::new(*size);
        for i in 0..*size as u64 {
            data.push(
                DataPoint::new(Duration::from_micros(i), (i as f64).sin()),
                *size,
            );
        }

        group.throughput(Throughput::Elements(*size as u64));

        group.bench_with_input(BenchmarkId::new("mean", size), &data, |b, data| {
            b.iter(|| {
                let sum: f64 = data.data_points.iter().map(|p| p.converted_value).sum();
                let mean = sum / data.data_points.len() as f64;
                black_box(mean)
            });
        });

        group.bench_with_input(BenchmarkId::new("std_dev", size), &data, |b, data| {
            b.iter(|| {
                let sum: f64 = data.data_points.iter().map(|p| p.converted_value).sum();
                let mean = sum / data.data_points.len() as f64;
                let variance: f64 = data
                    .data_points
                    .iter()
                    .map(|p| (p.converted_value - mean).powi(2))
                    .sum::<f64>()
                    / data.data_points.len() as f64;
                let std_dev = variance.sqrt();
                black_box(std_dev)
            });
        });
    }

    group.finish();
}

fn bench_downsample(c: &mut Criterion) {
    let mut group = c.benchmark_group("downsample");

    // Simulate downsampling for plot rendering
    let source_size = 100_000;
    let mut data = VariableData::new(source_size);
    for i in 0..source_size as u64 {
        data.push(
            DataPoint::new(Duration::from_micros(i), (i as f64).sin()),
            source_size,
        );
    }

    for target_size in [100, 500, 1000, 2000].iter() {
        group.bench_with_input(
            BenchmarkId::new("lttb_like", target_size),
            target_size,
            |b, &target_size| {
                b.iter(|| {
                    // Simple decimation (not true LTTB, but similar concept)
                    let step = data.data_points.len() / target_size;
                    let downsampled: Vec<[f64; 2]> = data
                        .data_points
                        .iter()
                        .step_by(step.max(1))
                        .take(target_size)
                        .map(|p| [p.timestamp.as_secs_f64(), p.converted_value])
                        .collect();
                    black_box(downsampled)
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_data_point_insertion,
    bench_plot_points_conversion,
    bench_value_parsing,
    bench_ring_buffer_operations,
    bench_statistics_calculation,
    bench_downsample,
);

criterion_main!(benches);
