use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use jqr::{parse, interpret, Jv};
use jqr::jv::parse_json;

fn parse_and_run(filter: &str, input: &str) -> Vec<Result<Jv, String>> {
    let ast = parse(filter).unwrap();
    let input_jv = parse_json(input).unwrap();
    interpret(&ast, input_jv).collect()
}

fn benchmark_identity(c: &mut Criterion) {
    let input = r#"{"a": 1, "b": 2}"#;
    c.bench_function("identity", |b| {
        b.iter(|| parse_and_run(black_box("."), black_box(input)))
    });
}

fn benchmark_field_access(c: &mut Criterion) {
    let input = r#"{"a": {"b": {"c": 1}}}"#;
    c.bench_function("field_access", |b| {
        b.iter(|| parse_and_run(black_box(".a.b.c"), black_box(input)))
    });
}

fn benchmark_array_iteration(c: &mut Criterion) {
    let input = "[1,2,3,4,5,6,7,8,9,10]";
    c.bench_function("array_iteration", |b| {
        b.iter(|| parse_and_run(black_box(".[]"), black_box(input)))
    });
}

fn benchmark_map(c: &mut Criterion) {
    let input = "[1,2,3,4,5,6,7,8,9,10]";
    c.bench_function("map", |b| {
        b.iter(|| parse_and_run(black_box("map(. * 2)"), black_box(input)))
    });
}

fn benchmark_select(c: &mut Criterion) {
    let input = "[1,2,3,4,5,6,7,8,9,10]";
    c.bench_function("select", |b| {
        b.iter(|| parse_and_run(black_box("[.[] | select(. > 5)]"), black_box(input)))
    });
}

fn benchmark_object_construction(c: &mut Criterion) {
    let input = r#"{"name": "test", "value": 42}"#;
    c.bench_function("object_construction", |b| {
        b.iter(|| parse_and_run(black_box("{n: .name, v: .value}"), black_box(input)))
    });
}

fn benchmark_large_array(c: &mut Criterion) {
    let input: String = format!("[{}]", (0..1000).map(|i| i.to_string()).collect::<Vec<_>>().join(","));

    let mut group = c.benchmark_group("large_array");

    group.bench_with_input(BenchmarkId::new("map", 1000), &input, |b, input| {
        b.iter(|| parse_and_run(black_box("map(. * 2)"), black_box(input)))
    });

    group.bench_with_input(BenchmarkId::new("filter", 1000), &input, |b, input| {
        b.iter(|| parse_and_run(black_box("[.[] | select(. % 2 == 0)]"), black_box(input)))
    });

    group.bench_with_input(BenchmarkId::new("sort", 1000), &input, |b, input| {
        b.iter(|| parse_and_run(black_box("sort_by(. * -1)"), black_box(input)))
    });

    group.bench_with_input(BenchmarkId::new("reduce", 1000), &input, |b, input| {
        b.iter(|| parse_and_run(black_box("reduce .[] as $x (0; . + $x)"), black_box(input)))
    });

    group.finish();
}

fn benchmark_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("parsing");

    group.bench_function("parse_simple", |b| {
        b.iter(|| parse(black_box(".")))
    });

    group.bench_function("parse_complex", |b| {
        b.iter(|| parse(black_box("[.[] | select(.a > 5) | {x: .a, y: .b}]")))
    });

    group.bench_function("json_parse_small", |b| {
        b.iter(|| parse_json(black_box(r#"{"a": 1}"#)))
    });

    let large_json = format!(
        "[{}]",
        (0..100)
            .map(|i| format!(r#"{{"id": {}, "name": "item{}"}}"#, i, i))
            .collect::<Vec<_>>()
            .join(",")
    );

    group.bench_with_input(BenchmarkId::new("json_parse_large", 100), &large_json, |b, json| {
        b.iter(|| parse_json(black_box(json)))
    });

    group.finish();
}

fn benchmark_string_ops(c: &mut Criterion) {
    let input = r#""hello world foo bar baz""#;

    let mut group = c.benchmark_group("string_ops");

    group.bench_function("split", |b| {
        b.iter(|| parse_and_run(black_box(r#"split(" ")"#), black_box(input)))
    });

    group.bench_function("join", |b| {
        b.iter(|| parse_and_run(black_box(r#"split(" ") | join("-")"#), black_box(input)))
    });

    group.bench_function("gsub", |b| {
        b.iter(|| parse_and_run(black_box(r#"gsub("o"; "0")"#), black_box(input)))
    });

    group.finish();
}

fn benchmark_recursive(c: &mut Criterion) {
    let deep_nested = r#"{"a":{"b":{"c":{"d":{"e":{"f":1}}}}}}"#;

    c.bench_function("recursive_descent", |b| {
        b.iter(|| parse_and_run(black_box(".. | numbers"), black_box(deep_nested)))
    });
}

criterion_group!(
    benches,
    benchmark_identity,
    benchmark_field_access,
    benchmark_array_iteration,
    benchmark_map,
    benchmark_select,
    benchmark_object_construction,
    benchmark_large_array,
    benchmark_parsing,
    benchmark_string_ops,
    benchmark_recursive,
);

criterion_main!(benches);
