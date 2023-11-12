use std::{fs, time::Duration};

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use tempfile::tempdir;

#[cfg(feature = "backend-threadpool")]
const BACKEND: &str = "threadpool";
#[cfg(feature = "backend-blocking")]
const BACKEND: &str = "blocking";
#[cfg(feature = "backend-rayon")]
const BACKEND: &str = "rayon";
#[cfg(feature = "backend-async-std")]
const BACKEND: &str = "async-std";
#[cfg(feature = "backend-smol")]
const BACKEND: &str = "smol";
#[cfg(feature = "backend-tokio")]
const BACKEND: &str = "tokio";

fn reading_ufos(c: &mut Criterion) {
    let files = fs::read_dir("benches/data/Roboto-Regular.ufo/glyphs")
        .unwrap()
        .filter_map(|de| {
            let de = de.unwrap();
            let file_name = de.file_name();
            let file_name = file_name.to_str().unwrap();
            file_name.ends_with(".glif").then(|| de.path())
        })
        .collect::<Vec<_>>();

    let mut group = c.benchmark_group("Reading");
    group
        .sample_size(100)
        .measurement_time(Duration::from_secs(10))
        .bench_with_input(
            BenchmarkId::new(
                format!("close_already {BACKEND}"),
                "Roboto-Regular.ufo",
            ),
            &files,
            |b, files| {
                b.iter(|| {
                    for file in files {
                        close_already::fs::read(file).unwrap();
                    }
                });
            },
        );
    group.bench_with_input(
        BenchmarkId::new("std::fs", "Roboto-Regular.ufo"),
        &files,
        |b, files| {
            b.iter(|| {
                for file in files {
                    fs::read(file).unwrap();
                }
            });
        },
    );
}

fn writing_ufos(c: &mut Criterion) {
    let temp_dir = tempdir().unwrap();
    let files = fs::read_dir("benches/data/Roboto-Regular.ufo/glyphs")
        .unwrap()
        .filter_map(|de| {
            let de = de.unwrap();
            let file_name = de.file_name();
            let file_name = file_name.to_str().unwrap();
            file_name.ends_with(".glif").then(|| {
                let bytes = fs::read(de.path()).unwrap();
                (temp_dir.path().join(file_name), bytes)
            })
        })
        .collect::<Vec<_>>();

    let mut group = c.benchmark_group("Writing");
    group
        .sample_size(20)
        .measurement_time(Duration::from_secs(50))
        .bench_with_input(
            BenchmarkId::new(
                format!("close_already {BACKEND}"),
                "Roboto-Regular.ufo",
            ),
            &files,
            |b, files| {
                b.iter(|| {
                    for (path, bytes) in files {
                        close_already::fs::write(path, bytes).unwrap();
                    }
                });
            },
        );
    group.bench_with_input(
        BenchmarkId::new("std::fs", "Roboto-Regular.ufo"),
        &files,
        |b, files| {
            b.iter(|| {
                for (path, bytes) in files {
                    fs::write(path, bytes).unwrap();
                }
            });
        },
    );
}

criterion_group!(criterion, reading_ufos, writing_ufos);
criterion_main!(criterion);
