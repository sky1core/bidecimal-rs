use std::{
    collections::BTreeMap,
    env,
    error::Error,
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn raw_command(program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .env("LC_ALL", "C")
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "{program} {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn pinned_channel() -> Result<String> {
    let config = fs::read_to_string("../rust-toolchain.toml")?;
    let mut in_toolchain = false;
    let mut channel = None;
    for line in config.lines().map(str::trim) {
        if line.starts_with('[') {
            in_toolchain = line == "[toolchain]";
        } else if in_toolchain {
            if let Some((key, value)) = line.split_once('=') {
                if key.trim() == "channel" {
                    let value = value
                        .trim()
                        .strip_prefix('"')
                        .and_then(|v| v.strip_suffix('"'))
                        .ok_or("channel must be a quoted pinned release")?;
                    if channel.replace(value.to_owned()).is_some() {
                        return Err("duplicate channel".into());
                    }
                }
            }
        }
    }
    let channel = channel.ok_or("rust-toolchain.toml: missing channel")?;
    if channel.split('.').count() != 3
        || !channel
            .split('.')
            .all(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err("toolchain channel must pin a numeric major.minor.patch release".into());
    }
    Ok(channel)
}

fn command(program: &str, args: &[&str]) -> Result<String> {
    if matches!(program, "cargo" | "rustc") {
        let pinned = pinned_channel()?;
        let mut pinned_args = vec!["run", &pinned, program];
        pinned_args.extend_from_slice(args);
        raw_command("rustup", &pinned_args)
    } else {
        raw_command(program, args)
    }
}

fn files(root: &Path, name: &str, found: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_dir() {
            files(&path, name, found)?;
        } else if path.file_name().is_some_and(|x| x == name) {
            found.push(path);
        }
    }
    Ok(())
}

fn source_id() -> Result<String> {
    let mut paths = vec![
        PathBuf::from("Cargo.toml"),
        PathBuf::from("Cargo.lock"),
        PathBuf::from("perf.rs"),
        PathBuf::from("../Cargo.toml"),
        PathBuf::from("../rust-toolchain.toml"),
    ];
    for directory in ["../src", "benches", "tests"] {
        let mut pending = vec![PathBuf::from(directory)];
        while let Some(dir) = pending.pop() {
            for entry in fs::read_dir(dir)? {
                let path = entry?.path();
                if path.is_dir() {
                    pending.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    paths.push(path);
                }
            }
        }
    }
    paths.sort();
    paths.dedup();
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for path in paths {
        for byte in path
            .to_str()
            .ok_or("non-UTF8 source path")?
            .bytes()
            .chain([0])
            .chain(fs::read(&path)?)
        {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3);
        }
    }
    Ok(format!("fnv1a64:{hash:016x}"))
}

fn array(json: &str, key: &str) -> Result<Vec<f64>> {
    let rest = json
        .split_once(&format!("\"{key}\":"))
        .ok_or("missing sample field")?
        .1
        .trim_start();
    let body = rest
        .strip_prefix('[')
        .ok_or("sample field is not array")?
        .split_once(']')
        .ok_or("unclosed array")?
        .0;
    Ok(body
        .split(',')
        .map(|s| s.trim().parse())
        .collect::<std::result::Result<_, _>>()?)
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Stats {
    median: f64,
    min: f64,
    p95: f64,
}

fn summarize(values: &[f64]) -> Result<Stats> {
    if values.is_empty() || values.iter().any(|v| !v.is_finite() || *v <= 0.0) {
        return Err("samples must be finite and positive".into());
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let n = sorted.len();
    let median = if n.is_multiple_of(2) {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    } else {
        sorted[n / 2]
    };
    Ok(Stats {
        median,
        min: sorted[0],
        p95: sorted[(95 * n).div_ceil(100) - 1],
    })
}

fn close(a: f64, b: f64) -> bool {
    a.is_finite() && b.is_finite() && (a - b).abs() <= 1e-10 * a.abs().max(b.abs()).max(1.0)
}

fn cross_check(values: &[f64], reported: Stats) -> Result<()> {
    let mut raw = values.to_vec();
    let n = raw.len();
    let min = raw
        .iter()
        .copied()
        .reduce(f64::min)
        .ok_or("empty samples")?;
    let upper = *raw.select_nth_unstable_by(n / 2, f64::total_cmp).1;
    let median = if n.is_multiple_of(2) {
        let lower = raw[..n / 2]
            .iter()
            .copied()
            .reduce(f64::max)
            .ok_or("empty lower half")?;
        (lower + upper) / 2.0
    } else {
        upper
    };
    let p95 = *raw
        .select_nth_unstable_by((95 * n).div_ceil(100) - 1, f64::total_cmp)
        .1;
    if !close(reported.median, median) || !close(reported.min, min) || !close(reported.p95, p95) {
        return Err("raw-sample statistics cross-check failed".into());
    }
    Ok(())
}

const SLOW_SAMPLE_MULTIPLIER: f64 = 1.5;
const RATIO_HALF_MEDIAN_CHANGE_LIMIT: f64 = 0.10;

#[derive(Debug, PartialEq)]
struct Diagnostics {
    ratio_median: f64,
    ratio_half_change: f64,
    ratio_iqr_over_median: f64,
    win_fraction: f64,
    half_changes: [f64; 2],
    slow_fractions: [f64; 2],
    slow_difference: f64,
    unstable: bool,
}

fn diagnostics(a: &[f64], b: &[f64]) -> Result<Diagnostics> {
    if a.len() != b.len() || a.len() < 2 {
        return Err("diagnostics require equal series of at least two samples".into());
    }
    let summaries = [summarize(a)?, summarize(b)?];
    let half = a.len() / 2;
    let mut half_changes = [0.0; 2];
    let mut slow_counts = [0_usize; 2];
    for (i, values) in [a, b].into_iter().enumerate() {
        half_changes[i] =
            summarize(&values[half..])?.median / summarize(&values[..half])?.median - 1.0;
        slow_counts[i] = values
            .iter()
            .filter(|v| **v >= SLOW_SAMPLE_MULTIPLIER * summaries[i].min)
            .count();
    }
    let n = a.len() as f64;
    let slow_fractions = slow_counts.map(|count| count as f64 / n);
    let slow_difference = slow_counts[0].abs_diff(slow_counts[1]) as f64 / n;
    let mut ratios: Vec<_> = a.iter().zip(b).map(|(a, b)| a / b).collect();
    let ratio_median = summarize(&ratios)?.median;
    let first_half = summarize(&ratios[..half])?.median;
    let second_half = summarize(&ratios[half..])?.median;
    let ratio_half_change = (second_half - first_half) / first_half;
    ratios.sort_by(f64::total_cmp);
    let q1 = ratios[(25 * ratios.len()).div_ceil(100) - 1];
    let q3 = ratios[(75 * ratios.len()).div_ceil(100) - 1];
    Ok(Diagnostics {
        ratio_median,
        ratio_half_change,
        ratio_iqr_over_median: (q3 - q1) / ratio_median,
        win_fraction: a.iter().zip(b).filter(|(a, b)| a <= b).count() as f64 / n,
        half_changes,
        slow_fractions,
        slow_difference,
        unstable: ratio_half_change.abs() >= RATIO_HALF_MEDIAN_CHANGE_LIMIT,
    })
}

struct PairedStats {
    ours: Stats,
    baseline: Stats,
    diagnostics: Diagnostics,
}

fn paired_stats(raw: &str, criterion: &str, count: f64) -> Result<PairedStats> {
    let iterations = array(raw, "iterations")?;
    let a = array(raw, "bidecimal_ns")?;
    let b = array(raw, "rust_decimal_ns")?;
    let expected_iters = array(criterion, "iters")?;
    let expected_times = array(criterion, "times")?;
    if [
        iterations.len(),
        a.len(),
        b.len(),
        expected_iters.len(),
        expected_times.len(),
    ]
    .iter()
    .any(|n| *n != 100)
        || !count.is_finite()
        || count <= 0.0
    {
        return Err("paired measurement requires 100 samples and a positive set size".into());
    }
    let mut ours = Vec::with_capacity(100);
    let mut baseline = Vec::with_capacity(100);
    for i in 0..100 {
        let iters = iterations[i];
        let total = a[i] + b[i];
        let time = expected_times[i];
        if !iters.is_finite()
            || iters <= 0.0
            || iters.fract() != 0.0
            || iters != expected_iters[i]
            || !a[i].is_finite()
            || a[i] <= 0.0
            || !b[i].is_finite()
            || b[i] <= 0.0
            || !total.is_finite()
            || !time.is_finite()
            || time <= 0.0
            || total != time
        {
            return Err(format!("paired identity cross-check failed at sample {i}").into());
        }
        ours.push(a[i] / iters / count);
        baseline.push(b[i] / iters / count);
    }
    let ours_summary = summarize(&ours)?;
    let baseline_summary = summarize(&baseline)?;
    cross_check(&ours, ours_summary)?;
    cross_check(&baseline, baseline_summary)?;
    Ok(PairedStats {
        ours: ours_summary,
        baseline: baseline_summary,
        diagnostics: diagnostics(&ours, &baseline)?,
    })
}

fn ratio(a: Stats, b: Stats) -> Result<f64> {
    let value = a.median / b.median;
    if !a.median.is_finite()
        || a.median <= 0.0
        || !b.median.is_finite()
        || b.median <= 0.0
        || !value.is_finite()
    {
        return Err("invalid median ratio".into());
    }
    Ok(value)
}

fn preliminary(loads: &[f64]) -> Result<bool> {
    if loads.iter().any(|v| !v.is_finite() || *v < 0.0) {
        return Err("invalid load average".into());
    }
    Ok(loads.iter().any(|v| *v >= 1.0))
}

fn workload_lines(log: &str) -> Vec<&str> {
    log.lines()
        .filter(|s| s.starts_with("CHECK ") || s.starts_with("WORKLOAD "))
        .collect()
}

fn expected_functions(log: &str) -> Result<BTreeMap<(String, String), usize>> {
    let mut functions = BTreeMap::new();
    for line in log.lines().filter(|s| s.starts_with("CHECK ")) {
        let mut parts = line.split_whitespace();
        parts.next();
        let name = parts
            .next()
            .ok_or("missing workload name")?
            .trim_end_matches(':');
        let counts: BTreeMap<_, _> = parts.filter_map(|v| v.split_once('=')).collect();
        let count = |key: &str| -> Result<usize> {
            Ok(counts.get(key).ok_or("missing workload count")?.parse()?)
        };
        let inside = count("comparison")?;
        let outside = count("standalone")?;
        if inside == 0
            || inside + outside + count("differing")? != count("generated")?
            || count("inside")? != inside + count("differing")?
        {
            return Err("invalid workload partition".into());
        }
        for implementation in ["bidecimal", "rust_decimal"] {
            if functions
                .insert((name.to_owned(), implementation.to_owned()), inside)
                .is_some()
            {
                return Err("duplicate workload".into());
            }
        }
        if outside > 0 {
            functions.insert((format!("wide_{name}"), "bidecimal".to_owned()), outside);
        }
    }
    if functions.is_empty() {
        return Err("no workload checks".into());
    }
    Ok(functions)
}

fn stats(
    sample: &Path,
    expected: &BTreeMap<(String, String), usize>,
    raw_directory: &Path,
) -> Result<(String, Vec<(String, Stats)>, Option<Diagnostics>)> {
    let dir = sample.parent().ok_or("no sample parent")?;
    let json = fs::read_to_string(sample)?;
    let iters = array(&json, "iters")?;
    let times = array(&json, "times")?;
    if iters.len() != 100 || times.len() != 100 {
        return Err("expected 100 samples".into());
    }
    let benchmark = fs::read_to_string(dir.join("benchmark.json"))?;
    let rest = benchmark
        .split_once("\"Elements\":")
        .ok_or("missing element throughput")?
        .1;
    let count: f64 = rest
        .split(|c: char| !c.is_ascii_digit())
        .find(|s| !s.is_empty())
        .ok_or("missing count")?
        .parse()?;
    let mut values = Vec::new();
    for (time, iterations) in times.iter().zip(&iters) {
        if !time.is_finite()
            || *time <= 0.0
            || !iterations.is_finite()
            || *iterations <= 0.0
            || !count.is_finite()
            || count <= 0.0
        {
            return Err("invalid sample".into());
        }
        values.push(time / iterations / count);
    }

    let implementation = dir
        .parent()
        .and_then(Path::file_name)
        .ok_or("no implementation")?
        .to_str()
        .ok_or("bad name")?
        .to_owned();
    let path = dir
        .parent()
        .and_then(Path::parent)
        .and_then(Path::file_name)
        .ok_or("no path")?
        .to_str()
        .ok_or("bad name")?
        .to_owned();
    let paired = implementation == "paired" && !path.starts_with("wide_");
    let expected_implementation = if paired { "bidecimal" } else { &implementation };
    if expected
        .get(&(path.clone(), expected_implementation.to_owned()))
        .copied()
        != Some(count as usize)
        || count.fract() != 0.0
        || (!paired && (!path.starts_with("wide_") || implementation != "bidecimal"))
        || (paired
            && expected
                .get(&(path.clone(), "rust_decimal".to_owned()))
                .copied()
                != Some(count as usize))
    {
        return Err(format!("throughput does not match workload: {path}/{implementation}").into());
    }
    if paired {
        let raw_path = raw_directory.join(format!("{path}.json"));
        let raw =
            fs::read_to_string(&raw_path).map_err(|e| format!("{}: {e}", raw_path.display()))?;
        let paired = paired_stats(&raw, &json, count)?;
        println!("CHECK_STATS {path}/paired: samples=100 elements={count} paired_identity PASS");
        return Ok((
            path,
            vec![
                ("bidecimal".to_owned(), paired.ours),
                ("rust_decimal".to_owned(), paired.baseline),
            ],
            Some(paired.diagnostics),
        ));
    }
    let summary = summarize(&values)?;
    cross_check(&values, summary)?;
    let estimates = fs::read_to_string(dir.join("estimates.json"))?;
    let estimate: f64 = estimates
        .split_once("\"median\":")
        .ok_or("missing criterion median")?
        .1
        .split_once("\"point_estimate\":")
        .ok_or("missing criterion point estimate")?
        .1
        .split([',', '}'])
        .next()
        .ok_or("missing point estimate value")?
        .trim()
        .parse()?;
    if !close(summary.median, estimate / count) {
        return Err(format!("criterion median cross-check failed: {path}/{implementation}").into());
    }
    println!(
        "CHECK_STATS {path}/{implementation}: samples={} elements={count} median/min/p95={:.9}/{:.9}/{:.9} criterion_median={:.9} PASS",
        values.len(),
        summary.median,
        summary.min,
        summary.p95,
        estimate / count
    );
    Ok((path, vec![(implementation, summary)], None))
}

fn benchmark(root: &Path, profile: &str, args: &[&str], baseline_first: bool) -> Result<()> {
    let start = Instant::now();
    let pinned = pinned_channel()?;
    let compiler = raw_command("rustup", &["which", "--toolchain", &pinned, "rustc"])?;
    let raw = root.join("raw");
    fs::create_dir(&raw)?;
    let removals = root.join("removed-inputs");
    fs::create_dir(&removals)?;
    let mut child = Command::new("rustup")
        .args([
            "run",
            &pinned,
            "cargo",
            "bench",
            "--offline",
            "--locked",
            "--profile",
            profile,
            "--bench",
            "paths",
            "--",
        ])
        .args(args)
        .env("CRITERION_HOME", root.join("criterion"))
        .env("PERF_REMOVALS_DIR", removals)
        .env("PERF_RAW_DIR", raw)
        .env("RUSTC", compiler)
        .env(
            "PERF_BASELINE_FIRST",
            if baseline_first { "1" } else { "0" },
        )
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = child.stdout.take().ok_or("stdout pipe missing")?;
    let stderr = child.stderr.take().ok_or("stderr pipe missing")?;
    let mut log = fs::File::create(root.join("full-output.txt"))?;
    std::thread::scope(|scope| -> Result<()> {
        let (tx, rx) = std::sync::mpsc::channel();
        let out = tx.clone();
        scope.spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if out.send(line).is_err() {
                    break;
                }
            }
        });
        scope.spawn(move || {
            for line in BufReader::new(stderr).lines() {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        for line in rx {
            let line = line?;
            println!("{line}");
            writeln!(log, "{line}")?;
        }
        Ok(())
    })?;
    let status = child.wait()?;
    let elapsed = start.elapsed().as_secs_f64();
    println!("wall_seconds={elapsed:.3}");
    writeln!(log, "wall_seconds={elapsed:.3}")?;
    if !status.success() {
        return Err("benchmark failed; see full-output.txt".into());
    }
    Ok(())
}

fn verify_version(program: &str, version: &str, pinned: &str) -> Result<()> {
    if version.split_whitespace().nth(1) != Some(pinned) {
        return Err(format!("toolchain mismatch: {program} reports {version}; rust-toolchain.toml requires {pinned}; use rustup proxies on PATH").into());
    }
    Ok(())
}

fn verify_toolchain() -> Result<()> {
    let pinned = pinned_channel()?;
    for program in ["cargo", "rustc"] {
        verify_version(program, &raw_command(program, &["-V"])?, &pinned)?;
        verify_version(program, &command(program, &["-V"])?, &pinned)?;
    }
    Ok(())
}

const DEFAULT_MAX_WAIT_SECONDS: u64 = 120;
const LOAD_POLL_SECONDS: u64 = 5;

fn options(args: impl IntoIterator<Item = String>) -> Result<(PathBuf, Duration, &'static str)> {
    let mut args = args.into_iter();
    let root = PathBuf::from(args.next().ok_or(
        "usage: perf-runner NEW_OUTPUT_DIRECTORY [--max-wait-seconds SECONDS] [--profile bench|bench-fat-lto] (defaults: 120 seconds, bench)",
    )?);
    let mut seconds = DEFAULT_MAX_WAIT_SECONDS;
    let mut profile = "bench";
    let mut seen_wait = false;
    let mut seen_profile = false;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--max-wait-seconds" if !seen_wait => {
                seconds = args.next().ok_or("missing maximum wait")?.parse()?;
                seen_wait = true;
            }
            "--profile" if !seen_profile => {
                profile = match args.next().as_deref() {
                    Some("bench") => "bench",
                    Some("bench-fat-lto") => "bench-fat-lto",
                    Some(_) => return Err("profile must be bench or bench-fat-lto".into()),
                    None => return Err("missing profile".into()),
                };
                seen_profile = true;
            }
            _ => return Err(format!("unexpected or repeated option: {flag}").into()),
        }
    }
    Ok((root, Duration::from_secs(seconds), profile))
}

const REJECTED_VARIABLES: [&str; 8] = [
    "CARGO",
    "RUSTC",
    "RUSTC_BOOTSTRAP",
    "RUSTC_WORKSPACE_WRAPPER",
    "RUSTC_WRAPPER",
    "RUSTDOC",
    "RUSTDOCFLAGS",
    "RUSTFLAGS",
];
const ALLOWED_CARGO_VARIABLES: [&str; 8] = [
    "CARGO_HOME",
    "CARGO_TARGET_DIR",
    "CARGO_BUILD_TARGET_DIR",
    "CARGO_BUILD_BUILD_DIR",
    "CARGO_LOG",
    "CARGO_CACHE_RUSTC_INFO",
    "CARGO_CACHE_AUTO_CLEAN_FREQUENCY",
    "CARGO_FUTURE_INCOMPAT_REPORT_FREQUENCY",
];
const ALLOWED_CARGO_PREFIXES: [&str; 3] = ["CARGO_TERM_", "CARGO_HTTP_", "CARGO_NET_"];
const DIRECTORY_CONFIGS: [&str; 2] = [".cargo/config.toml", ".cargo/config"];
const HOME_CONFIGS: [&str; 2] = ["config.toml", "config"];

fn rejected_variable(key: &str) -> bool {
    REJECTED_VARIABLES.contains(&key)
        || (key.starts_with("CARGO_")
            && !ALLOWED_CARGO_VARIABLES.contains(&key)
            && !ALLOWED_CARGO_PREFIXES
                .iter()
                .any(|prefix| key.starts_with(prefix)))
}

fn cargo_home(
    cwd: &Path,
    cargo_home: Option<std::ffi::OsString>,
    home_dir: Option<PathBuf>,
) -> Result<PathBuf> {
    match cargo_home.filter(|value| !value.is_empty()) {
        Some(value) => Ok(cwd.join(value)),
        None => Ok(home_dir
            .ok_or("cannot locate the home directory")?
            .join(".cargo")),
    }
}

fn cargo_config_candidates(cwd: &Path, cargo_home: &Path) -> Result<Vec<PathBuf>> {
    if !cwd.is_absolute() {
        return Err(format!("working directory must be absolute: {}", cwd.display()).into());
    }
    let mut candidates = Vec::new();
    for dir in cwd.ancestors() {
        candidates.extend(DIRECTORY_CONFIGS.iter().map(|config| dir.join(config)));
    }
    candidates.extend(HOME_CONFIGS.iter().map(|config| cargo_home.join(config)));
    Ok(candidates)
}

fn verify_build_variables(variables: impl IntoIterator<Item = std::ffi::OsString>) -> Result<()> {
    for key in variables {
        let key = key.to_string_lossy();
        if rejected_variable(&key) {
            return Err(format!("build environment must be absent: {key}").into());
        }
    }
    Ok(())
}

fn verify_configs_absent(candidates: &[PathBuf]) -> Result<()> {
    for config in candidates {
        if config.try_exists()? {
            return Err(format!("Cargo configuration must be absent: {}", config.display()).into());
        }
    }
    Ok(())
}

fn check_build_environment() -> Result<()> {
    let cwd = env::current_dir()?;
    let home = cargo_home(&cwd, env::var_os("CARGO_HOME"), env::home_dir())?;
    verify_build_variables(env::vars_os().map(|(key, _)| key))?;
    verify_configs_absent(&cargo_config_candidates(&cwd, &home)?)
}

fn build_check_rows(host: &str) -> String {
    let prefixes: Vec<String> = ALLOWED_CARGO_PREFIXES
        .iter()
        .map(|prefix| format!("{prefix}*"))
        .collect();
    format!(
        "| Target | {host} (host of the pinned rustc: the benchmark build passes no --target, CARGO_BUILD_TARGET is absent and no Cargo configuration file exists) |\n| Build environment variables | absent: {}, and every CARGO_* variable except {}, {} |\n| Cargo configuration files | absent: {} in the working directory and every ancestor directory; {} in the Cargo home |\n",
        REJECTED_VARIABLES.join(", "),
        ALLOWED_CARGO_VARIABLES.join(", "),
        prefixes.join(", "),
        DIRECTORY_CONFIGS.join(", "),
        HOME_CONFIGS.join(", "),
    )
}

fn load_average(os: &str) -> Result<f64> {
    let text = match os {
        "Darwin" => command("uptime", &[])?
            .split_once("load averages:")
            .ok_or("load averages missing")?
            .1
            .to_owned(),
        "Linux" => fs::read_to_string("/proc/loadavg")?,
        _ => return Err("unsupported load average host".into()),
    };
    let load = text
        .split_whitespace()
        .next()
        .ok_or("load missing")?
        .parse()?;
    preliminary(&[load])?;
    Ok(load)
}

fn wait_for_load(
    maximum: Duration,
    mut read_load: impl FnMut() -> Result<f64>,
    mut elapsed: impl FnMut() -> Duration,
    mut pause: impl FnMut(Duration),
) -> Result<(f64, Duration)> {
    loop {
        let load = read_load()?;
        preliminary(&[load])?;
        let waited = elapsed();
        if load < 1.0 || waited >= maximum {
            return Ok((load, waited));
        }
        pause(Duration::from_secs(LOAD_POLL_SECONDS).min(maximum - waited));
    }
}

fn main() -> Result<()> {
    let (root, maximum_wait, profile) = options(env::args().skip(1))?;
    check_build_environment()?;
    verify_toolchain()?;
    if root.exists() {
        return Err("output directory already exists; use a new directory".into());
    }
    fs::create_dir_all(&root)?;
    let start = Instant::now();
    let os = command("uname", &["-s"])?;
    let cpu = match os.as_str() {
        "Darwin" => command("system_profiler", &["SPHardwareDataType"])?
            .lines()
            .find_map(|s| s.trim().strip_prefix("Chip: ").map(str::to_owned))
            .ok_or("CPU chip missing")?,
        "Linux" => fs::read_to_string("/proc/cpuinfo")?
            .lines()
            .find_map(|line| {
                line.strip_prefix("model name")
                    .and_then(|s| s.split_once(':'))
                    .map(|(_, s)| s.trim().to_owned())
            })
            .ok_or("CPU model missing")?,
        _ => return Err("supported hosts: Darwin, Linux".into()),
    };
    let source = source_id()?;
    let metadata = format!(
        "| Field | Value |\n|---|---|\n| rustc | {} |\n| cargo | {} |\n| Commit | {} |\n| Working tree | {} |\n| Source fingerprint | {source} |\n| Profile | {profile} |\n| CPU | {cpu} |\n| OS | {} |\n{}",
        command("rustc", &["-V"])?,
        command("cargo", &["-V"])?,
        command("git", &["rev-parse", "HEAD"])?,
        if command("git", &["status", "--porcelain"])?.is_empty() {
            "clean"
        } else {
            "modified"
        },
        command("uname", &["-srm"])?,
        build_check_rows(
            command("rustc", &["-vV"])?
                .lines()
                .find_map(|s| s.strip_prefix("host: "))
                .ok_or("host missing")?
        )
    );
    let metadata = format!(
        "{metadata}| Maximum load wait per session (seconds) | {} |\n| Load poll interval (seconds) | {LOAD_POLL_SECONDS} |\n",
        maximum_wait.as_secs()
    );
    fs::write(root.join("metadata.md"), &metadata)?;
    println!("{metadata}");
    let check = root.join("preflight");
    fs::create_dir(&check)?;
    benchmark(&check, profile, &["--test"], false)?;
    let mut report = metadata;
    let preflight = fs::read_to_string(check.join("full-output.txt"))?;
    report.push_str("\n```text\n");
    let expected = expected_functions(&preflight)?;
    let checks = workload_lines(&preflight);
    for line in &checks {
        report.push_str(line);
        report.push('\n');
    }
    report.push_str("```\n");
    report.push_str("\n| Session | Date (UTC) | Load start | Load end | Wait seconds | Status | Unstable | Rerun cause |\n|---|---|---:|---:|---:|---|---|---|\n");
    let mut rows = String::from(
        "\n| Session | Path | bidecimal median/min/p95 (ns/element) | rust_decimal median/min/p95 (ns/element) | Ratio |\n|---:|---|---:|---:|---:|\n",
    );
    let mut diagnostic_rows = String::from(
        "\n| Session | Path | Sample ratio median (diagnostic) | bidecimal <= baseline fraction (diagnostic) | Half median change bidecimal / baseline (diagnostic) | Slow fraction bidecimal / baseline (diagnostic) | Slow fraction difference (diagnostic) | Sample ratio half change (diagnostic) | Sample ratio IQR/median (diagnostic) | Unstable |\n|---:|---|---:|---:|---:|---:|---:|---:|---:|---|\n",
    );
    for session in 1..=3 {
        if source_id()? != source {
            return Err("sources changed during run".into());
        }
        let dir = root.join(format!("session-{session}"));
        fs::create_dir(&dir)?;
        check_build_environment()?;
        println!(
            "WAIT session={session} maximum_seconds={}",
            maximum_wait.as_secs()
        );
        let wait_start = Instant::now();
        let (load, waited) = wait_for_load(
            maximum_wait,
            || load_average(&os),
            || wait_start.elapsed(),
            std::thread::sleep,
        )?;
        let date = command("date", &["-u", "+%Y-%m-%dT%H:%M:%SZ"])?;
        benchmark(&dir, profile, &["--noplot"], session == 2)?;
        let end_load = load_average(&os)?;
        let log = fs::read_to_string(dir.join("full-output.txt"))?;
        if workload_lines(&log) != checks {
            return Err("workload changed between sessions".into());
        }
        let mut samples = Vec::new();
        files(&dir.join("criterion"), "sample.json", &mut samples)?;
        samples.sort();
        let mut grouped: BTreeMap<String, BTreeMap<String, Stats>> = BTreeMap::new();
        let mut unstable_paths = Vec::new();
        for sample in samples {
            if sample
                .parent()
                .and_then(Path::file_name)
                .is_some_and(|s| s == "new")
            {
                let (path, measurements, diagnostics) =
                    stats(&sample, &expected, &dir.join("raw"))?;
                if let Some(d) = diagnostics {
                    if d.unstable {
                        unstable_paths.push(path.clone());
                    }
                    diagnostic_rows.push_str(&format!(
                        "| {session} | {path} | {:.6} | {:.2} | {:+.6}/{:+.6} | {:.2}/{:.2} | {:.2} | {:+.6} | {:.6} | {} |\n",
                        d.ratio_median, d.win_fraction, d.half_changes[0], d.half_changes[1],
                        d.slow_fractions[0], d.slow_fractions[1], d.slow_difference,
                        d.ratio_half_change, d.ratio_iqr_over_median, d.unstable
                    ));
                }
                let results = grouped.entry(path).or_default();
                for (implementation, summary) in measurements {
                    if results.insert(implementation, summary).is_some() {
                        return Err("duplicate measured function".into());
                    }
                }
            }
        }
        let measured: std::collections::BTreeSet<_> = grouped
            .iter()
            .flat_map(|(path, implementations)| {
                implementations.keys().map(|i| (path.clone(), i.clone()))
            })
            .collect();
        let required: std::collections::BTreeSet<_> = expected.keys().cloned().collect();
        if measured.is_empty() || measured != required {
            return Err(format!(
                "incomplete run: measured {} functions, expected {}",
                measured.len(),
                required.len()
            )
            .into());
        }
        unstable_paths.sort();
        let unstable = !unstable_paths.is_empty();
        let state = if load >= 1.0 {
            "preliminary"
        } else {
            "load < 1.0"
        };
        let mut causes = Vec::new();
        if load >= 1.0 {
            causes.push("load >= 1.0 at session start".to_owned());
        }
        if unstable {
            causes.push(format!("unstable paths: {}", unstable_paths.join(", ")));
        }
        let cause = if causes.is_empty() {
            "none".to_owned()
        } else {
            causes.join("; ")
        };
        let line = format!(
            "| {session} | {date} | {load:.2} | {end_load:.2} | {:.3} | {state} | {unstable} | {cause} |\n",
            waited.as_secs_f64()
        );
        print!("{line}");
        fs::write(dir.join("metadata.md"), &line)?;
        report.push_str(&line);
        for (path, results) in grouped {
            let a = results.get("bidecimal").ok_or("missing bidecimal")?;
            let row = if path.starts_with("wide_") {
                if results.len() != 1 {
                    return Err("unexpected standalone baseline".into());
                }
                format!(
                    "| {session} | {path} | {:.4}/{:.4}/{:.4} | — | — |\n",
                    a.median, a.min, a.p95
                )
            } else {
                let b = results.get("rust_decimal").ok_or("missing baseline")?;
                let ratio = ratio(*a, *b)?;
                format!(
                    "| {session} | {path} | {:.4}/{:.4}/{:.4} | {:.4}/{:.4}/{:.4} | {ratio:.6} |\n",
                    a.median, a.min, a.p95, b.median, b.min, b.p95
                )
            };
            print!("{row}");
            rows.push_str(&row);
        }
    }
    if source_id()? != source {
        return Err("sources changed during run".into());
    }
    report.push_str(&rows);
    report.push_str(&diagnostic_rows);
    report.push_str(&format!(
        "\nTotal wall seconds: {:.3}\n",
        start.elapsed().as_secs_f64()
    ));
    fs::write(root.join("report.md"), &report)?;
    print!("{report}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture(PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn median_min_and_nearest_rank_p95() {
        for (values, expected) in [
            (
                vec![3.0],
                Stats {
                    median: 3.0,
                    min: 3.0,
                    p95: 3.0,
                },
            ),
            (
                vec![9.0, 1.0, 5.0],
                Stats {
                    median: 5.0,
                    min: 1.0,
                    p95: 9.0,
                },
            ),
            (
                vec![9.0, 1.0, 5.0, 3.0],
                Stats {
                    median: 4.0,
                    min: 1.0,
                    p95: 9.0,
                },
            ),
            (
                (1..=100).rev().map(f64::from).collect(),
                Stats {
                    median: 50.5,
                    min: 1.0,
                    p95: 95.0,
                },
            ),
            (
                (1..=20).rev().map(f64::from).collect(),
                Stats {
                    median: 10.5,
                    min: 1.0,
                    p95: 19.0,
                },
            ),
            (
                (1..=21).rev().map(f64::from).collect(),
                Stats {
                    median: 11.0,
                    min: 1.0,
                    p95: 20.0,
                },
            ),
        ] {
            assert_eq!(summarize(&values).unwrap(), expected);
            cross_check(&values, expected).unwrap();
            let wrong = Stats {
                p95: expected.p95 + 1.0,
                ..expected
            };
            assert!(cross_check(&values, wrong).is_err());
        }
        for values in [
            vec![],
            vec![0.0],
            vec![-1.0],
            vec![f64::NAN],
            vec![f64::INFINITY],
        ] {
            assert!(summarize(&values).is_err());
        }
    }

    #[test]
    fn ratios_and_preliminary_labels() {
        let a = summarize(&[2.0, 4.0, 6.0]).unwrap();
        let b = summarize(&[1.0, 2.0, 3.0]).unwrap();
        assert_eq!(ratio(a, b).unwrap(), 2.0);
        assert_eq!(ratio(b, a).unwrap(), 0.5);
        assert_eq!(ratio(a, a).unwrap(), 1.0);
        let quiet = [0.0, 0.99, 0.5];
        assert!(!preliminary(&quiet).unwrap());
        assert!(preliminary(&[0.9, 1.0, 0.5]).unwrap());
        assert!(preliminary(&[1.0]).unwrap());
        assert!(preliminary(&[f64::NAN]).is_err());
        assert!(preliminary(&[-0.1]).is_err());
        assert!(ratio(Stats { median: 0.0, ..a }, b).is_err());
    }

    #[test]
    fn raw_sample_scaling_and_manifest() {
        let iters = array(r#"{"iters":[1,2,4],"times":[20,80,240]}"#, "iters").unwrap();
        let times = array(r#"{"iters":[1,2,4],"times":[20,80,240]}"#, "times").unwrap();
        let values: Vec<_> = times.iter().zip(iters).map(|(t, i)| t / i / 10.0).collect();
        assert_eq!(
            summarize(&values).unwrap(),
            Stats {
                median: 4.0,
                min: 2.0,
                p95: 6.0
            }
        );
        let functions = expected_functions(
            "CHECK parse: generated=10 inside=8 comparison=7 standalone=2 differing=1 examples=[]",
        )
        .unwrap();
        assert_eq!(functions.len(), 3);
        assert_eq!(functions[&("parse".to_owned(), "bidecimal".to_owned())], 7);
        assert_eq!(
            functions[&("wide_parse".to_owned(), "bidecimal".to_owned())],
            2
        );
        assert!(
            expected_functions("CHECK parse: generated=10 comparison=7 standalone=1 differing=1")
                .is_err()
        );
    }

    #[test]
    fn criterion_files_cross_check_every_reported_statistic() {
        let root = PathBuf::from(format!("target/perf-stats-fixture-{}", std::process::id()));
        assert!(!root.exists());
        let _fixture = Fixture(root.clone());
        let dir = root.join("wide_parse/bidecimal/new");
        fs::create_dir_all(&dir).unwrap();
        let values: Vec<_> = (1..=100).map(f64::from).collect();
        let times: Vec<_> = values.iter().map(|v| v * v * 10.0).collect();
        fs::write(
            dir.join("sample.json"),
            format!("{{\"iters\":{values:?},\"times\":{times:?}}}"),
        )
        .unwrap();
        fs::write(
            dir.join("benchmark.json"),
            r#"{"throughput":{"Elements":10}}"#,
        )
        .unwrap();
        fs::write(
            dir.join("estimates.json"),
            r#"{"median":{"point_estimate":505.0}}"#,
        )
        .unwrap();
        let expected = BTreeMap::from([(("wide_parse".to_owned(), "bidecimal".to_owned()), 10)]);
        let (_, results, diagnostic) = stats(&dir.join("sample.json"), &expected, &root).unwrap();
        assert!(diagnostic.is_none());
        let result = results[0].1;
        assert_eq!(
            result,
            Stats {
                median: 50.5,
                min: 1.0,
                p95: 95.0
            }
        );
        fs::write(
            dir.join("estimates.json"),
            r#"{"median":{"point_estimate":506.0}}"#,
        )
        .unwrap();
        assert!(stats(&dir.join("sample.json"), &expected, &root).is_err());
        fs::write(dir.join("sample.json"), r#"{"iters":[1],"times":[1]}"#).unwrap();
        assert!(stats(&dir.join("sample.json"), &expected, &root).is_err());
        drop(_fixture);
        assert!(!root.exists());
    }

    fn paired_json(a: &[f64], b: &[f64]) -> (String, String) {
        let iterations: Vec<_> = (1..=100).map(f64::from).collect();
        let a: Vec<_> = a
            .iter()
            .zip(&iterations)
            .map(|(v, i)| v * i * 10.0)
            .collect();
        let b: Vec<_> = b
            .iter()
            .zip(&iterations)
            .map(|(v, i)| v * i * 10.0)
            .collect();
        let times: Vec<_> = a.iter().zip(&b).map(|(a, b)| a + b).collect();
        (
            format!(
                "{{\"iterations\":{iterations:?},\"bidecimal_ns\":{a:?},\"rust_decimal_ns\":{b:?}}}"
            ),
            format!("{{\"iters\":{iterations:?},\"times\":{times:?}}}"),
        )
    }

    #[test]
    fn paired_raw_statistics_and_diagnostics() {
        let a: Vec<_> = (1..=100).map(f64::from).collect();
        let (raw, criterion) = paired_json(&a, &[100.0; 100]);
        let result = paired_stats(&raw, &criterion, 10.0).unwrap();
        assert_eq!(
            result.ours,
            Stats {
                median: 50.5,
                min: 1.0,
                p95: 95.0
            }
        );
        assert_eq!(
            result.baseline,
            Stats {
                median: 100.0,
                min: 100.0,
                p95: 100.0
            }
        );
        assert_eq!(ratio(result.ours, result.baseline).unwrap(), 0.505);
        assert_eq!(result.diagnostics.ratio_median, 0.505);
        assert!(close(
            result.diagnostics.ratio_half_change,
            (0.755 - 0.255) / 0.255
        ));
        assert!(close(
            result.diagnostics.ratio_iqr_over_median,
            (0.75 - 0.25) / 0.505
        ));
        assert_eq!(result.diagnostics.win_fraction, 1.0);
        assert!(close(result.diagnostics.half_changes[0], 75.5 / 25.5 - 1.0));
        assert_eq!(result.diagnostics.half_changes[1], 0.0);
        assert_eq!(result.diagnostics.slow_fractions, [0.99, 0.0]);
        assert_eq!(result.diagnostics.slow_difference, 0.99);
        assert!(result.diagnostics.unstable);
        let a: Vec<_> = [1.0, 8.0].repeat(50);
        let b: Vec<_> = [2.0, 4.0].repeat(50);
        let (raw, criterion) = paired_json(&a, &b);
        let result = paired_stats(&raw, &criterion, 10.0).unwrap();
        assert_eq!(result.diagnostics.ratio_median, 1.25);
        assert_eq!(result.diagnostics.win_fraction, 0.5);
        assert_eq!(result.diagnostics.ratio_half_change, 0.0);
        assert_eq!(result.diagnostics.ratio_iqr_over_median, 1.2);
        assert!(!result.diagnostics.unstable);
        assert_eq!(ratio(result.ours, result.baseline).unwrap(), 1.5);
    }

    #[test]
    fn paired_identity_rejects_iteration_and_timing_violations() {
        let (raw, criterion) = paired_json(&[1.0; 100], &[2.0; 100]);
        paired_stats(&raw, &criterion, 10.0).unwrap();
        for changed in [
            raw.replacen("[1.0,", "[2.0,", 1),
            raw.replacen("[1.0,", "[0.0,", 1),
            raw.replacen("[1.0,", "[1.5,", 1),
            raw.replacen("[10.0,", "[11.0,", 1),
            raw.replacen("[10.0,", "[9.0,", 1),
            raw.replacen("[10.0,", "[0.0,", 1),
            raw.replacen("[10.0,", "[NaN,", 1),
            raw.replacen("[10.0,", "[inf,", 1),
            raw.replacen("[10.0,", "[", 1),
        ] {
            assert!(
                paired_stats(&changed, &criterion, 10.0).is_err(),
                "{changed}"
            );
        }
        for changed in [
            criterion.replacen("[30.0,", "[30.2,", 1),
            criterion.replacen("[30.0,", "[29.8,", 1),
            criterion.replacen("[30.0,", "[100.0,", 1),
        ] {
            assert!(paired_stats(&raw, &changed, 10.0).is_err(), "{changed}");
        }
        assert!(paired_stats(&raw, &criterion, 0.0).is_err());
        assert!(paired_stats(&raw, &criterion, f64::NAN).is_err());
        assert!(paired_stats(&raw, &criterion.replace("[30.0,", "[inf,"), 10.0).is_err());
    }

    #[test]
    fn lockstep_drift_is_diagnostic_only() {
        assert!(!diagnostics(&[1.0; 100], &[2.0; 100]).unwrap().unstable);
        let step = [vec![1.0; 50], vec![1.7; 50]].concat();
        let d = diagnostics(&step, &step).unwrap();
        assert!(!d.unstable);
        assert!(d.half_changes.iter().all(|v| close(*v, 0.7)));
        assert_eq!(d.slow_difference, 0.0);
        assert_eq!(d.ratio_half_change, 0.0);
        assert_eq!(d.ratio_iqr_over_median, 0.0);
    }

    #[test]
    fn ratio_half_change_has_inclusive_ten_percent_boundary() {
        for (second, unstable) in [
            (11.0_f64.next_down(), false),
            (11.0, true),
            (11.0_f64.next_up(), true),
            (9.0_f64.next_up(), false),
            (9.0, true),
            (9.0_f64.next_down(), true),
        ] {
            let step = [vec![10.0; 50], vec![second; 50]].concat();
            assert_eq!(
                diagnostics(&step, &[1.0; 100]).unwrap().unstable,
                unstable,
                "second-half ratio {second:?}"
            );
        }
    }

    #[test]
    fn unequal_slow_fractions_with_stable_ratio_are_diagnostic_only() {
        for slow_count_per_half in [5, 6, 20] {
            let half: Vec<_> = (0..50)
                .map(|i| if i < slow_count_per_half { 1.5 } else { 1.0 })
                .collect();
            let series = half.repeat(2);
            let d = diagnostics(&series, &[1.0; 100]).unwrap();
            assert_eq!(d.half_changes, [0.0; 2]);
            assert!(close(
                d.slow_difference,
                f64::from(slow_count_per_half) / 50.0
            ));
            assert!(!d.unstable);
            assert!(!diagnostics(&[1.0; 100], &series).unwrap().unstable);
        }
        let a = [1.0, 1.51].repeat(50);
        let b = [1.0, 1.49].repeat(50);
        let d = diagnostics(&a, &b).unwrap();
        assert_eq!(d.slow_difference, 0.5);
        assert!(!d.unstable);
        assert!(diagnostics(&[], &[]).is_err());
        assert!(diagnostics(&[1.0, 2.0], &[1.0]).is_err());
    }

    #[test]
    fn paired_files_use_raw_sides_instead_of_criterion_combined_estimate() {
        let root = PathBuf::from(format!("target/perf-paired-fixture-{}", std::process::id()));
        assert!(!root.exists());
        let _fixture = Fixture(root.clone());
        let dir = root.join("parse/paired/new");
        let raw_dir = root.join("raw");
        fs::create_dir_all(&dir).unwrap();
        fs::create_dir(&raw_dir).unwrap();
        let (raw, criterion) = paired_json(&[1.0; 100], &[2.0; 100]);
        fs::write(dir.join("sample.json"), &criterion).unwrap();
        fs::write(
            dir.join("benchmark.json"),
            r#"{"throughput":{"Elements":10}}"#,
        )
        .unwrap();
        fs::write(raw_dir.join("parse.json"), &raw).unwrap();
        let expected = BTreeMap::from([
            (("parse".to_owned(), "bidecimal".to_owned()), 10),
            (("parse".to_owned(), "rust_decimal".to_owned()), 10),
        ]);
        let (path, results, diagnostic) =
            stats(&dir.join("sample.json"), &expected, &raw_dir).unwrap();
        assert_eq!(path, "parse");
        assert_eq!(results[0].1.median, 1.0);
        assert_eq!(results[1].1.median, 2.0);
        assert!(!diagnostic.unwrap().unstable);
        fs::write(
            raw_dir.join("parse.json"),
            raw.replacen("[1.0,", "[2.0,", 1),
        )
        .unwrap();
        assert!(stats(&dir.join("sample.json"), &expected, &raw_dir).is_err());
    }

    #[test]
    fn load_wait_stops_when_quiet_or_at_explicit_limit() {
        use std::cell::Cell;
        for (loads, maximum, expected, waited) in [
            (vec![0.99], 120, 0.99, 0),
            (vec![1.0, 1.2, 0.8], 120, 0.8, 10),
            (vec![3.0, 2.0, 1.0], 7, 1.0, 7),
            (vec![2.0], 0, 2.0, 0),
        ] {
            let clock = Cell::new(Duration::ZERO);
            let mut loads = loads.into_iter();
            let (load, duration) = wait_for_load(
                Duration::from_secs(maximum),
                || Ok(loads.next().unwrap()),
                || clock.get(),
                |d| clock.set(clock.get() + d),
            )
            .unwrap();
            assert_eq!(load, expected);
            assert_eq!(duration, Duration::from_secs(waited));
        }
        assert!(wait_for_load(Duration::ZERO, || Ok(f64::NAN), || Duration::ZERO, |_| {}).is_err());
    }

    #[test]
    fn command_line_options_are_explicit_or_documented_defaults() {
        let parse = |args: &[&str]| options(args.iter().map(|s| (*s).to_owned()));
        assert_eq!(parse(&["output"]).unwrap().1, Duration::from_secs(120));
        assert_eq!(parse(&["output"]).unwrap().2, "bench");
        for profile in ["bench", "bench-fat-lto"] {
            assert_eq!(
                parse(&["output", "--profile", profile]).unwrap(),
                (PathBuf::from("output"), Duration::from_secs(120), profile)
            );
            for args in [
                vec!["output", "--profile", profile, "--max-wait-seconds", "0"],
                vec!["output", "--max-wait-seconds", "0", "--profile", profile],
            ] {
                assert_eq!(
                    parse(&args).unwrap(),
                    (PathBuf::from("output"), Duration::ZERO, profile)
                );
            }
        }
        assert_eq!(
            parse(&["output", "--max-wait-seconds", "0"]).unwrap().1,
            Duration::ZERO
        );
        for args in [
            vec![],
            vec!["output", "--max-wait-seconds"],
            vec!["output", "--max-wait-seconds", "-1"],
            vec!["output", "--other", "120"],
            vec!["output", "--max-wait-seconds", "1", "extra"],
            vec![
                "output",
                "--max-wait-seconds",
                "1",
                "--max-wait-seconds",
                "2",
            ],
            vec!["output", "--profile"],
            vec!["output", "--profile", "release"],
            vec!["output", "--profile", ""],
            vec!["output", "--profile", "bench", "--profile", "bench-fat-lto"],
        ] {
            assert!(parse(&args).is_err());
        }
    }

    #[test]
    fn refuses_build_override_variables() {
        verify_build_variables(Vec::new()).unwrap();
        for key in REJECTED_VARIABLES.into_iter().chain([
            "CARGO_ENCODED_RUSTFLAGS",
            "CARGO_ENCODED_RUSTDOCFLAGS",
            "CARGO_BUILD_RUSTFLAGS",
            "CARGO_BUILD_RUSTDOCFLAGS",
            "CARGO_BUILD_TARGET",
            "CARGO_BUILD_RUSTC",
            "CARGO_BUILD_RUSTC_WRAPPER",
            "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
            "CARGO_BUILD_RUSTDOC",
            "CARGO_BUILD_INCREMENTAL",
            "CARGO_BUILD_JOBS",
            "CARGO_INCREMENTAL",
            "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS",
            "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER",
            "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER",
            "CARGO_TARGET_AARCH64_APPLE_DARWIN_RUSTFLAGS",
            "CARGO_PROFILE_RELEASE_LTO",
            "CARGO_PROFILE_BENCH_OPT_LEVEL",
            "CARGO_PROFILE_BENCH_LTO",
            "CARGO_PROFILE_BENCH_FAT_LTO_CODEGEN_UNITS",
            "CARGO_PROFILE_BENCH_BUILD_OVERRIDE_OPT_LEVEL",
            "CARGO_ALIAS_BENCH",
            "CARGO_REGISTRIES_CRATES_IO_PROTOCOL",
            "CARGO_RESOLVER_LOCKFILE_PATH",
            "CARGO_UNSTABLE_BUILD_STD",
        ]) {
            assert_eq!(
                verify_build_variables(vec![key.into()])
                    .unwrap_err()
                    .to_string(),
                format!("build environment must be absent: {key}")
            );
        }
        for key in ALLOWED_CARGO_VARIABLES.into_iter().chain([
            "OTHER",
            "PATH",
            "HOME",
            "RUSTUP_HOME",
            "RUSTUP_TOOLCHAIN",
            "CARGO_TERM_COLOR",
            "CARGO_HTTP_TIMEOUT",
            "CARGO_NET_OFFLINE",
        ]) {
            verify_build_variables(vec![key.into()]).unwrap();
        }
    }

    #[test]
    fn refuses_cargo_configuration_files() {
        let root = env::current_dir()
            .unwrap()
            .join(format!("target/perf-env-fixture-{}", std::process::id()));
        assert!(!root.exists());
        let _fixture = Fixture(root.clone());
        let candidates_inside = |base: &Path| -> Vec<PathBuf> {
            fs::create_dir_all(base.join("work/a/b")).unwrap();
            fs::create_dir_all(base.join("home")).unwrap();
            let candidates: Vec<PathBuf> =
                cargo_config_candidates(&base.join("work/a/b"), &base.join("home"))
                    .unwrap()
                    .into_iter()
                    .filter(|candidate| candidate.starts_with(base))
                    .collect();
            assert_eq!(candidates.len(), 10);
            candidates
        };
        verify_configs_absent(&candidates_inside(&root.join("clean"))).unwrap();
        let cases = [
            ("work/a/b/.cargo/config.toml", "working directory"),
            ("work/a/b/.cargo/config", "working directory"),
            ("work/.cargo/config.toml", "ancestor"),
            ("work/a/.cargo/config", "ancestor"),
            (".cargo/config.toml", "ancestor"),
            ("home/config.toml", "Cargo home"),
            ("home/config", "Cargo home"),
        ];
        for (i, (config, place)) in cases.into_iter().enumerate() {
            let case = root.join(i.to_string());
            let candidates = candidates_inside(&case);
            let file = case.join(config);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(&file, "").unwrap();
            assert_eq!(
                verify_configs_absent(&candidates).unwrap_err().to_string(),
                format!("Cargo configuration must be absent: {}", file.display()),
                "{place}: {config}"
            );
        }
        drop(_fixture);
        assert!(!root.exists());
    }

    #[test]
    fn cargo_configuration_paths_follow_cargo_probing_order() {
        assert_eq!(
            cargo_config_candidates(Path::new("/p/w"), Path::new("/h/.cargo")).unwrap(),
            [
                "/p/w/.cargo/config.toml",
                "/p/w/.cargo/config",
                "/p/.cargo/config.toml",
                "/p/.cargo/config",
                "/.cargo/config.toml",
                "/.cargo/config",
                "/h/.cargo/config.toml",
                "/h/.cargo/config",
            ]
            .map(PathBuf::from)
        );
        assert_eq!(
            cargo_config_candidates(Path::new("work"), Path::new("/h/.cargo"))
                .unwrap_err()
                .to_string(),
            "working directory must be absolute: work"
        );
        let cwd = Path::new("/p/w");
        assert_eq!(
            cargo_home(cwd, Some("/c".into()), Some("/h".into())).unwrap(),
            PathBuf::from("/c")
        );
        assert_eq!(
            cargo_home(cwd, Some("c".into()), None).unwrap(),
            PathBuf::from("/p/w/c")
        );
        assert_eq!(
            cargo_home(cwd, None, Some("/h".into())).unwrap(),
            PathBuf::from("/h/.cargo")
        );
        assert_eq!(
            cargo_home(cwd, Some("".into()), Some("/h".into())).unwrap(),
            PathBuf::from("/h/.cargo")
        );
        for cargo_home_value in [None, Some("".into())] {
            assert_eq!(
                cargo_home(cwd, cargo_home_value, None)
                    .unwrap_err()
                    .to_string(),
                "cannot locate the home directory"
            );
        }
    }

    #[test]
    fn build_check_rows_name_every_checked_variable_and_file() {
        let rows = build_check_rows("x86_64-unknown-linux-gnu");
        assert!(rows.starts_with("| Target | x86_64-unknown-linux-gnu (host of the pinned rustc"));
        assert_eq!(rows.lines().count(), 3);
        for name in REJECTED_VARIABLES
            .into_iter()
            .chain(ALLOWED_CARGO_VARIABLES)
            .chain(DIRECTORY_CONFIGS)
            .chain(HOME_CONFIGS)
        {
            assert!(rows.contains(name), "{name}");
        }
        for prefix in ALLOWED_CARGO_PREFIXES {
            assert!(rows.contains(&format!("{prefix}*")), "{prefix}");
        }
    }

    #[test]
    fn rejects_wrong_toolchain_versions() {
        verify_version("rustc", "rustc 1.99.1 (hash date)", "1.99.1").unwrap();
        verify_version("cargo", "cargo 1.99.1 (hash date)", "1.99.1").unwrap();
        assert!(verify_version("cargo", "cargo 1.94.1 (hash date)", "1.99.1").is_err());
        assert!(verify_version("rustc", "rustc 1.98.0 (hash date)", "1.99.1").is_err());
    }
}
