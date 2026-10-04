//! The lexer benchmark harness.
//!
//! Usage:
//!
//! ```text
//! fernq-bench --tier dev|validation --corpus <dir> --out <dir>
//! fernq-bench --instructions <implementation> <workload> <count> --corpus <dir>
//! ```
//!
//! The harness reads the corpus `MANIFEST`. Before any timing, the
//! equivalence gate lexes every corpus file with Fernq and with the adapter
//! and requires the same token classes and spans; any difference ends the run
//! with status 1. It then times three implementations on each workload:
//! `fernq` (`fernq::bench::lex`), `adapter` (the equivalence adapter over
//! `rustc_lexer`), and `rustc_lexer-tokenize` (`rustc_lexer` alone, telemetry
//! that does less work than the other two). A workload is one synthetic file,
//! `real/<source>`, the files of one source of the real corpus, or `real`,
//! every file of the real corpus.
//!
//! A sample is one timed pass over the workload, repeated until it covers at
//! least [`SAMPLE_BYTES`]; ns per pass divides by the repetitions. Each
//! implementation gets one warm-up sample and [`SAMPLES`] samples per
//! workload. The dev tier is one run; the validation tier is two runs, the
//! second in reversed implementation order. A counting allocator records the
//! allocations of each sample.
//!
//! Writes `samples.tsv`, every raw sample, `summary.tsv`, the median,
//! minimum, and maximum of each implementation and workload, and `sizes.tsv`,
//! the sizes of the Fernq lexer representation, to the output directory. `--instructions` lexes one workload `count` times with one
//! implementation and prints the folded result, for an external counter such
//! as `time -l`.

mod adapter;

use std::alloc::{GlobalAlloc, Layout, System};
use std::fmt::Write as _;
use std::hint::black_box;
use std::path::Path;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;
use std::{env, fs};

use fernq::bench::TokenClass;
use ra_ap_rustc_lexer::FrontmatterAllowed;

use crate::adapter::{Edition, HashSink, Sink};

/// Timed samples per implementation and workload, after one warm-up sample.
const SAMPLES: usize = 15;

/// The fewest bytes one sample lexes: a short workload repeats its pass.
const SAMPLE_BYTES: usize = 1 << 20;

const IMPLEMENTATIONS: [&str; 3] = ["fernq", "adapter", "rustc_lexer-tokenize"];

/// Counts allocations while [`COUNTING`] is set.
struct CountingAllocator;

static COUNTING: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);

// SAFETY: every method forwards its arguments unchanged to `System`, which
// meets the `GlobalAlloc` contract; counting touches only atomics, which need
// atomicity, not ordering, and allocates nothing.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        // SAFETY: the caller upholds the `GlobalAlloc::alloc` contract for
        // `layout`, which `System` requires unchanged.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        // SAFETY: as for `alloc`; `System` gets the caller's `layout`.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` was allocated by this allocator, which is `System`,
        // with `layout`, as the caller guarantees.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count(new_size);
        // SAFETY: `ptr` was allocated by `System` with `layout`, and the
        // caller upholds the `realloc` contract for `new_size`.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn count(bytes: usize) {
    if COUNTING.load(Ordering::Relaxed) {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        ALLOCATED_BYTES.fetch_add(bytes as u64, Ordering::Relaxed);
    }
}

/// One corpus file: its manifest path, text, and edition.
struct File {
    path: String,
    text: String,
    edition: u16,
}

/// The files that one sample lexes once each.
struct Workload {
    name: String,
    files: Vec<usize>,
    bytes: usize,
    code_points: usize,
}

/// The first difference between Fernq and the adapter on one file.
#[derive(Debug, PartialEq, Eq)]
struct Mismatch {
    /// The index of the first token that differs.
    index: usize,
    fernq: Option<(TokenClass, u32, u32)>,
    adapter: Option<(TokenClass, u32, u32)>,
    /// Which side ended lexing with an error, if any.
    error: &'static str,
}

/// Compares the adapter's tokens with `expected`, Fernq's, as they arrive.
struct CompareSink<'a> {
    expected: &'a [(TokenClass, u32, u32)],
    index: usize,
    mismatch: Option<Mismatch>,
}

impl Sink for CompareSink<'_> {
    fn token(&mut self, class: TokenClass, lo: u32, hi: u32) {
        let got = (class, lo, hi);
        if self.mismatch.is_none() && self.expected.get(self.index) != Some(&got) {
            self.mismatch = Some(Mismatch {
                index: self.index,
                fernq: self.expected.get(self.index).copied(),
                adapter: Some(got),
                error: "",
            });
        }
        self.index += 1;
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let result = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [
            "--tier",
            tier @ ("dev" | "validation"),
            "--corpus",
            corpus,
            "--out",
            out,
        ] => run(tier, Path::new(corpus), Path::new(out)),
        [
            "--instructions",
            implementation,
            workload,
            count,
            "--corpus",
            corpus,
        ] => instructions(implementation, workload, count, Path::new(corpus)),
        _ => {
            eprintln!(
                "usage: fernq-bench --tier dev|validation --corpus <dir> --out <dir>\n       \
                 fernq-bench --instructions <implementation> <workload> <count> --corpus <dir>"
            );
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("fernq-bench: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(tier: &str, corpus: &Path, out: &Path) -> Result<(), String> {
    let files = load(corpus)?;
    let tokens = gate(&files)?;
    let workloads = workloads(&files);
    fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let runs: &[bool] = if tier == "validation" {
        &[false, true]
    } else {
        &[false]
    };
    let mut samples = String::from(
        "run\timplementation\tworkload\tsample\titerations\tns\tns_per_pass\tallocations\tbytes_allocated\n",
    );
    let mut summary = String::from(
        "run\timplementation\tworkload\tbytes\ttokens\titerations\tsamples\tmedian_ns\tmin_ns\tmax_ns\t\
         ns_per_byte\tmb_per_s\ttokens_per_s\tcode_points_per_s\tallocations\tbytes_allocated\n",
    );
    for (run_index, &reversed) in runs.iter().enumerate() {
        let run = run_index + 1;
        let mut order = IMPLEMENTATIONS;
        if reversed {
            order.reverse();
        }
        println!("run {run}: {}", order.join(", "));
        for workload in &workloads {
            for implementation in order {
                let measured = measure(implementation, workload, &files);
                let iterations = SAMPLE_BYTES.div_ceil(workload.bytes.max(1));
                for (i, m) in measured.iter().enumerate() {
                    let _ = writeln!(
                        samples,
                        "{run}\t{implementation}\t{}\t{}\t{iterations}\t{}\t{:.1}\t{}\t{}",
                        workload.name,
                        i + 1,
                        m.ns,
                        m.ns as f64 / iterations as f64,
                        m.allocations,
                        m.bytes_allocated
                    );
                }
                let token_count = match implementation {
                    "rustc_lexer-tokenize" => raw_tokens(workload, &files),
                    _ => workload.files.iter().map(|&f| tokens[f]).sum(),
                };
                let line = summarize(
                    run,
                    implementation,
                    workload,
                    token_count,
                    iterations,
                    &measured,
                );
                print!("{line}");
                summary.push_str(&line);
            }
        }
    }
    let write = |name: &str, text: &str| {
        let path = out.join(name);
        fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
    };
    let mut sizes = String::from("type\tbytes\n");
    for (name, size) in fernq::bench::sizes() {
        let _ = writeln!(sizes, "{name}\t{size}");
    }
    write("sizes.tsv", &sizes)?;
    write("samples.tsv", &samples)?;
    write("summary.tsv", &summary)
}

/// Reads the manifest and every file it lists.
fn load(corpus: &Path) -> Result<Vec<File>, String> {
    let manifest_path = corpus.join("MANIFEST");
    let manifest = fs::read_to_string(&manifest_path)
        .map_err(|e| format!("{}: {e}; run make bench-corpus", manifest_path.display()))?;
    let mut files = Vec::new();
    for line in manifest.lines().filter(|line| !line.starts_with('#')) {
        let fields: Vec<&str> = line.split('\t').collect();
        let [path, _bytes, _sha256, edition, ..] = fields.as_slice() else {
            return Err(format!("malformed manifest line: {line:?}"));
        };
        let edition = edition
            .parse()
            .map_err(|_| format!("malformed manifest line: {line:?}"))?;
        let text = fs::read_to_string(corpus.join(path)).map_err(|e| format!("{path}: {e}"))?;
        files.push(File {
            path: (*path).to_owned(),
            text,
            edition,
        });
    }
    Ok(files)
}

/// The equivalence gate. Returns the number of Fernq tokens of each file.
fn gate(files: &[File]) -> Result<Vec<usize>, String> {
    let mut counts = Vec::with_capacity(files.len());
    for file in files {
        let fernq = fernq::bench::tokens(&file.text, file.edition);
        let edition = Edition::from_year(file.edition)
            .ok_or_else(|| format!("{}: unknown edition {}", file.path, file.edition))?;
        if let Err(mismatch) = compare(fernq.as_deref(), &file.text, edition) {
            return Err(format!(
                "equivalence gate failed on {}: {mismatch:?}; nothing timed",
                file.path
            ));
        }
        counts.push(fernq.map_or(0, |tokens| tokens.len()));
    }
    let tokens: usize = counts.iter().sum();
    println!(
        "equivalence gate: {} files, {tokens} tokens, 0 mismatches",
        files.len()
    );
    Ok(counts)
}

/// Compares Fernq's result on `text`, `fernq`, with the adapter's. Where
/// Fernq ends with an error, which it gives without the tokens before it, the
/// adapter must end with an error too.
fn compare(
    fernq: Option<&[(TokenClass, u32, u32)]>,
    text: &str,
    edition: Edition,
) -> Result<(), Mismatch> {
    let Some(expected) = fernq else {
        if adapter::lex(text, edition, &mut HashSink::default()).is_none() {
            return Ok(());
        }
        return Err(Mismatch {
            index: 0,
            fernq: None,
            adapter: None,
            error: "only fernq ends with an error",
        });
    };
    let mut sink = CompareSink {
        expected,
        index: 0,
        mismatch: None,
    };
    let adapter = adapter::lex(text, edition, &mut sink);
    if let Some(mismatch) = sink.mismatch {
        return Err(mismatch);
    }
    let error = match adapter {
        Some(()) if sink.index == expected.len() => return Ok(()),
        Some(()) => "",
        None => "only the adapter ends with an error",
    };
    Err(Mismatch {
        index: sink.index,
        fernq: expected.get(sink.index).copied(),
        adapter: None,
        error,
    })
}

fn workloads(files: &[File]) -> Vec<Workload> {
    let workload = |name: String, files_in: Vec<usize>| Workload {
        bytes: files_in.iter().map(|&f| files[f].text.len()).sum(),
        code_points: files_in
            .iter()
            .map(|&f| files[f].text.chars().count())
            .sum(),
        name,
        files: files_in,
    };
    let mut workloads: Vec<Workload> = files
        .iter()
        .enumerate()
        .filter_map(|(index, file)| {
            let name = file.path.strip_prefix("synthetic/")?.strip_suffix(".rs")?;
            Some(workload(name.to_owned(), vec![index]))
        })
        .collect();
    let mut sources: Vec<&str> = Vec::new();
    for file in files {
        if let Some(source) = real_source(&file.path)
            && !sources.contains(&source)
        {
            sources.push(source);
        }
    }
    for source in sources {
        let members = (0..files.len())
            .filter(|&f| real_source(&files[f].path) == Some(source))
            .collect();
        workloads.push(workload(format!("real/{source}"), members));
    }
    let real = (0..files.len())
        .filter(|&f| real_source(&files[f].path).is_some())
        .collect();
    workloads.push(workload("real".to_owned(), real));
    workloads
}

/// The source of a real corpus path: `<source>` in `real/<source>/...`.
fn real_source(path: &str) -> Option<&str> {
    path.strip_prefix("real/")?.split('/').next()
}

/// The folded result of one pass of `implementation` over `files`.
fn pass(implementation: &str, workload: &Workload, files: &[File]) -> u64 {
    let mut folded = 0u64;
    for &f in &workload.files {
        let file = &files[f];
        let text = black_box(file.text.as_str());
        folded ^= match implementation {
            "fernq" => fernq::bench::lex(text, file.edition).map_or(0, |(n, h)| n as u64 ^ h),
            "adapter" => {
                let edition =
                    Edition::from_year(file.edition).expect("the gate checked the edition");
                let mut sink = HashSink::default();
                adapter::lex(text, edition, &mut sink).map_or(0, |()| sink.count as u64 ^ sink.hash)
            }
            _ => ra_ap_rustc_lexer::tokenize(text, FrontmatterAllowed::No)
                .fold(0u64, |acc, token| acc.wrapping_add(u64::from(token.len))),
        };
    }
    folded
}

/// The number of `rustc_lexer` tokens of the workload, trivia included.
fn raw_tokens(workload: &Workload, files: &[File]) -> usize {
    workload
        .files
        .iter()
        .map(|&f| ra_ap_rustc_lexer::tokenize(&files[f].text, FrontmatterAllowed::No).count())
        .sum()
}

struct Measurement {
    ns: u128,
    allocations: u64,
    bytes_allocated: u64,
}

/// One warm-up sample, then [`SAMPLES`] timed samples.
fn measure(implementation: &str, workload: &Workload, files: &[File]) -> Vec<Measurement> {
    let iterations = SAMPLE_BYTES.div_ceil(workload.bytes.max(1));
    let sample = || {
        ALLOCATIONS.store(0, Ordering::Relaxed);
        ALLOCATED_BYTES.store(0, Ordering::Relaxed);
        COUNTING.store(true, Ordering::Relaxed);
        let start = Instant::now();
        for _ in 0..iterations {
            black_box(pass(implementation, workload, files));
        }
        let ns = start.elapsed().as_nanos();
        COUNTING.store(false, Ordering::Relaxed);
        Measurement {
            ns,
            allocations: ALLOCATIONS.load(Ordering::Relaxed),
            bytes_allocated: ALLOCATED_BYTES.load(Ordering::Relaxed),
        }
    };
    sample();
    (0..SAMPLES).map(|_| sample()).collect()
}

/// One `summary.tsv` line. Times are per pass; allocations are per pass.
fn summarize(
    run: usize,
    implementation: &str,
    workload: &Workload,
    tokens: usize,
    iterations: usize,
    measured: &[Measurement],
) -> String {
    let per_pass = |m: &Measurement| m.ns as f64 / iterations as f64;
    let mut times: Vec<f64> = measured.iter().map(per_pass).collect();
    times.sort_by(f64::total_cmp);
    let median = times[times.len() / 2];
    let (min, max) = (times[0], times[times.len() - 1]);
    let seconds = median / 1e9;
    let code_points = if workload.name.starts_with("non-ascii") {
        format!("{:.0}", workload.code_points as f64 / seconds)
    } else {
        "-".to_owned()
    };
    let allocations = measured[0].allocations as f64 / iterations as f64;
    let bytes_allocated = measured[0].bytes_allocated as f64 / iterations as f64;
    format!(
        "{run}\t{implementation}\t{}\t{}\t{tokens}\t{iterations}\t{}\t{median:.0}\t{min:.0}\t{max:.0}\t\
         {:.3}\t{:.1}\t{:.0}\t{code_points}\t{allocations:.1}\t{bytes_allocated:.0}\n",
        workload.name,
        workload.bytes,
        measured.len(),
        median / workload.bytes as f64,
        workload.bytes as f64 / 1e6 / seconds,
        tokens as f64 / seconds,
    )
}

fn instructions(
    implementation: &str,
    workload: &str,
    count: &str,
    corpus: &Path,
) -> Result<(), String> {
    if !IMPLEMENTATIONS.contains(&implementation) {
        return Err(format!("unknown implementation {implementation:?}"));
    }
    let count: usize = count
        .parse()
        .map_err(|_| format!("count {count:?} is not a number"))?;
    let files = load(corpus)?;
    let workloads = workloads(&files);
    let workload = workloads
        .iter()
        .find(|w| w.name == workload)
        .ok_or_else(|| format!("unknown workload {workload:?}"))?;
    let mut folded = 0u64;
    for _ in 0..count {
        folded ^= black_box(pass(implementation, workload, &files));
    }
    println!("{implementation} {} x{count}: {folded:#x}", workload.name);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gate_accepts_equal_results() {
        let text = "fn main() { let x = 1 + 2; }";
        let fernq = fernq::bench::tokens(text, 2024);
        assert_eq!(compare(fernq.as_deref(), text, Edition::E2024), Ok(()));
        let invalid = "fn main() { ` }";
        assert_eq!(compare(None, invalid, Edition::E2024), Ok(()));
    }

    #[test]
    fn the_gate_reports_a_planted_difference() {
        let text = "fn main() { let x = 1 + 2; }";
        let mut planted = fernq::bench::tokens(text, 2024).unwrap();
        planted[5].0 = TokenClass::Identifier;
        let mismatch = compare(Some(&planted), text, Edition::E2024).unwrap_err();
        println!("planted class difference: {mismatch:?}");
        assert_eq!(
            mismatch,
            Mismatch {
                index: 5,
                fernq: Some((TokenClass::Identifier, 12, 15)),
                adapter: Some((TokenClass::Keyword, 12, 15)),
                error: "",
            }
        );

        planted.truncate(3);
        let mismatch = compare(Some(&planted), text, Edition::E2024).unwrap_err();
        println!("planted missing tokens: {mismatch:?}");
        assert_eq!(mismatch.index, 3);
        assert_eq!(mismatch.fernq, None);

        let mismatch = compare(None, text, Edition::E2024).unwrap_err();
        println!("planted error: {mismatch:?}");
        assert_eq!(mismatch.index, 0);
    }
}
