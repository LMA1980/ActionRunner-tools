//! Internal argument parsing and metric parsing helpers for built-in tool adapters.

use std::collections::BTreeMap;

use crate::adapter::{ParsedBenchmark, ParsedTestCase, TestCaseStatus};
use crate::error::{MetricParseError, ToolValidationError};

/// Split an argument slice at the first `"--"` separator.
///
/// Returns `(before_dash_dash, Some(after_dash_dash))` if `"--"` is present,
/// or `(args, None)` if not.
#[must_use]
pub(crate) fn split_dash_dash(args: &[String]) -> (&[String], Option<&[String]>) {
    if let Some(pos) = args.iter().position(|a| a == "--") {
        (&args[..pos], Some(&args[pos + 1..]))
    } else {
        (args, None)
    }
}

/// Parse a value-taking flag in either `--flag value`, `-f value`, `--flag=value`, or `-f=value` form.
///
/// If matched, advances `*idx` when the value is in the next argument and returns `Ok(Some(value))`.
/// If the argument does not match `long` or `short`, returns `Ok(None)` without modifying `*idx`.
///
/// # Errors
///
/// Returns [`ToolValidationError::MissingRequiredArg`] if the flag is missing its value, or
/// [`ToolValidationError::InvalidArgValue`] if the value is empty or starts with `-`.
pub(crate) fn parse_flag_value<'a>(
    args: &'a [String],
    idx: &mut usize,
    short: Option<&str>,
    long: &str,
    tool: &str,
) -> Result<Option<&'a str>, ToolValidationError> {
    let arg = &args[*idx];
    if arg == long || short.is_some_and(|s| arg == s) {
        if *idx + 1 >= args.len() {
            return Err(ToolValidationError::MissingRequiredArg {
                tool: tool.to_string(),
                expected: long.to_string(),
            });
        }
        let val = &args[*idx + 1];
        if val.starts_with('-') {
            return Err(ToolValidationError::InvalidArgValue {
                tool: tool.to_string(),
                flag: long.to_string(),
                value: val.to_string(),
                reason: "flag value cannot start with '-'".to_string(),
            });
        }
        *idx += 1;
        return Ok(Some(val.as_str()));
    }

    let long_prefix = format!("{long}=");
    if arg.starts_with(&long_prefix) {
        let val = &arg[long_prefix.len()..];
        if val.is_empty() {
            return Err(ToolValidationError::InvalidArgValue {
                tool: tool.to_string(),
                flag: long.to_string(),
                value: String::new(),
                reason: "flag value cannot be empty".to_string(),
            });
        }
        return Ok(Some(val));
    }

    if let Some(s) = short {
        let short_prefix = format!("{s}=");
        if arg.starts_with(&short_prefix) {
            let val = &arg[short_prefix.len()..];
            if val.is_empty() {
                return Err(ToolValidationError::InvalidArgValue {
                    tool: tool.to_string(),
                    flag: s.to_string(),
                    value: String::new(),
                    reason: "flag value cannot be empty".to_string(),
                });
            }
            return Ok(Some(val));
        }
    }

    Ok(None)
}

/// Count occurrences of a string pattern across UTF-8 stdout and stderr streams.
///
/// # Errors
///
/// Returns [`MetricParseError::InvalidUtf8`] if either stream is not valid UTF-8.
pub(crate) fn count_occurrences(
    stdout: &[u8],
    stderr: &[u8],
    pattern: &str,
) -> Result<u32, MetricParseError> {
    let out = std::str::from_utf8(stdout).map_err(|_| MetricParseError::InvalidUtf8)?;
    let err = std::str::from_utf8(stderr).map_err(|_| MetricParseError::InvalidUtf8)?;
    let total = out.matches(pattern).count() + err.matches(pattern).count();
    Ok(total as u32)
}

/// Detailed test execution output including summary counts and granular per-test cases.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct ParsedTestSuiteOutput {
    pub passed: Option<u32>,
    pub failed: Option<u32>,
    pub ignored: Option<u32>,
    pub test_cases: Vec<ParsedTestCase>,
}

/// Parse an optional trailing duration token such as `(12ms)`, `(1.25s)`, `12ms`, or `1.25s`.
fn parse_duration_token_ms(token: &str) -> Option<u64> {
    let trimmed = token.trim().trim_start_matches('(').trim_end_matches(')');
    if let Some(ms_str) = trimmed.strip_suffix("ms") {
        ms_str.trim().parse::<f64>().ok().map(|v| v.round() as u64)
    } else if let Some(us_str) = trimmed
        .strip_suffix("us")
        .or_else(|| trimmed.strip_suffix("µs"))
    {
        us_str
            .trim()
            .parse::<f64>()
            .ok()
            .map(|v| (v / 1000.0).round() as u64)
    } else if let Some(s_str) = trimmed.strip_suffix('s') {
        s_str
            .trim()
            .parse::<f64>()
            .ok()
            .map(|v| (v * 1000.0).round() as u64)
    } else {
        None
    }
}

/// Extract failure stdout/stderr sections formatted as `---- <test_name> stdout ----` or `---- <test_name> stderr ----`.
fn extract_failure_snippets(text: &str) -> BTreeMap<String, String> {
    let mut snippets: BTreeMap<String, String> = BTreeMap::new();
    let mut current_test: Option<String> = None;
    let mut current_lines: Vec<&str> = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(inner) = trimmed
            .strip_prefix("---- ")
            .and_then(|s| s.strip_suffix(" ----"))
        {
            if let Some(prev_name) = current_test.take() {
                let joined = current_lines.join("\n").trim().to_string();
                if !joined.is_empty() {
                    snippets
                        .entry(prev_name)
                        .and_modify(|existing| {
                            existing.push('\n');
                            existing.push_str(&joined);
                        })
                        .or_insert(joined);
                }
                current_lines.clear();
            }
            let test_name = inner
                .strip_suffix(" stdout")
                .or_else(|| inner.strip_suffix(" stderr"))
                .unwrap_or(inner)
                .trim();
            if !test_name.is_empty() {
                current_test = Some(test_name.to_string());
            }
            continue;
        }

        if trimmed == "failures:" || trimmed.starts_with("test result:") {
            if let Some(prev_name) = current_test.take() {
                let joined = current_lines.join("\n").trim().to_string();
                if !joined.is_empty() {
                    snippets
                        .entry(prev_name)
                        .and_modify(|existing| {
                            existing.push('\n');
                            existing.push_str(&joined);
                        })
                        .or_insert(joined);
                }
                current_lines.clear();
            }
            continue;
        }

        if current_test.is_some() {
            current_lines.push(line);
        }
    }

    if let Some(prev_name) = current_test.take() {
        let joined = current_lines.join("\n").trim().to_string();
        if !joined.is_empty() {
            snippets
                .entry(prev_name)
                .and_modify(|existing| {
                    existing.push('\n');
                    existing.push_str(&joined);
                })
                .or_insert(joined);
        }
    }

    snippets
}

/// Parse individual `test <name> ... ok|FAILED|ignored` lines and `test result:` summary lines.
///
/// # Errors
///
/// Returns [`MetricParseError::InvalidUtf8`] if output is not valid UTF-8.
pub(crate) fn parse_test_results(
    stdout: &[u8],
    stderr: &[u8],
) -> Result<ParsedTestSuiteOutput, MetricParseError> {
    let out = std::str::from_utf8(stdout).map_err(|_| MetricParseError::InvalidUtf8)?;
    let err = std::str::from_utf8(stderr).map_err(|_| MetricParseError::InvalidUtf8)?;

    let mut snippets = extract_failure_snippets(out);
    for (k, v) in extract_failure_snippets(err) {
        snippets
            .entry(k)
            .and_modify(|existing| {
                existing.push('\n');
                existing.push_str(&v);
            })
            .or_insert(v);
    }

    let mut total_passed = 0u32;
    let mut total_failed = 0u32;
    let mut total_ignored = 0u32;
    let mut found_summary = false;
    let mut test_cases = Vec::new();

    for text in [out, err] {
        for line in text.lines() {
            let trimmed = line.trim();
            if let Some(rest_after_test) = trimmed.strip_prefix("test ") {
                if !rest_after_test.starts_with("result:") {
                    if let Some((name_part, outcome_part)) = rest_after_test.split_once(" ... ") {
                        let test_name = name_part.trim().to_string();
                        let outcome_trimmed = outcome_part.trim();
                        if !outcome_trimmed.starts_with("bench:") && !test_name.is_empty() {
                            let mut parts = outcome_trimmed.split_whitespace();
                            if let Some(status_tok) = parts.next() {
                                let status_opt = match status_tok {
                                    "ok" => Some(TestCaseStatus::Passed),
                                    "FAILED" => Some(TestCaseStatus::Failed),
                                    "ignored" => Some(TestCaseStatus::Ignored),
                                    _ => None,
                                };
                                if let Some(status) = status_opt {
                                    let remainder = outcome_trimmed[status_tok.len()..].trim();
                                    let duration_ms = if remainder.is_empty() {
                                        None
                                    } else {
                                        parse_duration_token_ms(remainder)
                                    };
                                    let failure_snippet = if status == TestCaseStatus::Failed {
                                        snippets.get(&test_name).cloned()
                                    } else {
                                        None
                                    };
                                    test_cases.push(ParsedTestCase {
                                        test_name,
                                        status,
                                        duration_ms,
                                        failure_snippet,
                                    });
                                }
                            }
                        }
                    }
                }
            }

            if let Some(pos) = line.find("test result:") {
                let rest = &line[pos + "test result:".len()..];
                let mut passed_val = None;
                let mut failed_val = None;
                let mut ignored_val = None;
                let tokens: Vec<&str> = rest.split_whitespace().collect();
                for (i, &token) in tokens.iter().enumerate() {
                    let cleaned = token.trim_end_matches([';', ',', '.']);
                    if cleaned == "passed" && i > 0 {
                        if let Ok(n) = tokens[i - 1].parse::<u32>() {
                            passed_val = Some(n);
                        }
                    } else if cleaned == "failed" && i > 0 {
                        if let Ok(n) = tokens[i - 1].parse::<u32>() {
                            failed_val = Some(n);
                        }
                    } else if cleaned == "ignored" && i > 0 {
                        if let Ok(n) = tokens[i - 1].parse::<u32>() {
                            ignored_val = Some(n);
                        }
                    }
                }
                if let (Some(p), Some(f)) = (passed_val, failed_val) {
                    total_passed += p;
                    total_failed += f;
                    if let Some(ig) = ignored_val {
                        total_ignored += ig;
                    }
                    found_summary = true;
                }
            }
        }
    }

    if found_summary {
        Ok(ParsedTestSuiteOutput {
            passed: Some(total_passed),
            failed: Some(total_failed),
            ignored: Some(total_ignored),
            test_cases,
        })
    } else if !test_cases.is_empty() {
        let passed = test_cases
            .iter()
            .filter(|c| c.status == TestCaseStatus::Passed)
            .count() as u32;
        let failed = test_cases
            .iter()
            .filter(|c| c.status == TestCaseStatus::Failed)
            .count() as u32;
        let ignored = test_cases
            .iter()
            .filter(|c| c.status == TestCaseStatus::Ignored)
            .count() as u32;
        Ok(ParsedTestSuiteOutput {
            passed: Some(passed),
            failed: Some(failed),
            ignored: Some(ignored),
            test_cases,
        })
    } else {
        Ok(ParsedTestSuiteOutput::default())
    }
}

/// Recursively search a `serde_json::Value` for a numeric `"percent"` field.
fn find_json_percent(val: &serde_json::Value) -> Option<f64> {
    match val {
        serde_json::Value::Object(map) => {
            if let Some(p) = map.get("percent") {
                if let Some(f) = p.as_f64() {
                    return Some(f);
                }
            }
            for v in map.values() {
                if let Some(f) = find_json_percent(v) {
                    return Some(f);
                }
            }
            None
        }
        serde_json::Value::Array(arr) => {
            for v in arr {
                if let Some(f) = find_json_percent(v) {
                    return Some(f);
                }
            }
            None
        }
        _ => None,
    }
}

/// Parse coverage percentage from JSON output (`"percent": <f64>`) or `TOTAL ... XX.XX%` lines.
///
/// # Errors
///
/// Returns [`MetricParseError::InvalidUtf8`] if output is not valid UTF-8.
pub(crate) fn parse_coverage_pct(
    stdout: &[u8],
    stderr: &[u8],
) -> Result<Option<f64>, MetricParseError> {
    let out = std::str::from_utf8(stdout).map_err(|_| MetricParseError::InvalidUtf8)?;
    let err = std::str::from_utf8(stderr).map_err(|_| MetricParseError::InvalidUtf8)?;

    // 1. Check if stdout is JSON
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(out) {
        if let Some(pct) = find_json_percent(&val) {
            return Ok(Some(pct));
        }
    }

    // 2. Scan text for TOTAL lines
    for text in [out, err] {
        for line in text.lines() {
            if line.contains("TOTAL") {
                for word in line.split_whitespace().rev() {
                    if let Some(pct_str) = word.strip_suffix('%') {
                        if let Ok(pct) = pct_str.parse::<f64>() {
                            return Ok(Some(pct));
                        }
                    }
                }
            }
        }
    }

    Ok(None)
}

/// Detailed benchmark execution output including aggregate metrics and individual benchmarks.
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct ParsedBenchSuiteOutput {
    pub bench_ns_per_iter: Option<f64>,
    pub bench_mb_per_sec: Option<f64>,
    pub benchmarks: Vec<ParsedBenchmark>,
}

fn parse_number_clean(s: &str) -> Option<f64> {
    let cleaned: String = s.chars().filter(|&c| c != ',' && c != '_').collect();
    cleaned.trim().parse::<f64>().ok()
}

fn convert_time_to_ns(val: f64, unit: &str) -> Option<f64> {
    match unit.trim() {
        "ps" => Some(val / 1_000.0),
        "ns" => Some(val),
        "us" | "µs" | "μs" => Some(val * 1_000.0),
        "ms" => Some(val * 1_000_000.0),
        "s" => Some(val * 1_000_000_000.0),
        _ => None,
    }
}

fn convert_throughput_to_mb_s(val: f64, unit: &str) -> Option<f64> {
    match unit.trim() {
        "B/s" => Some(val / 1_000_000.0),
        "KB/s" | "kB/s" => Some(val / 1_000.0),
        "KiB/s" => Some(val * 1024.0 / 1_000_000.0),
        "MB/s" => Some(val),
        "MiB/s" => Some(val * 1_048_576.0 / 1_000_000.0),
        "GB/s" => Some(val * 1_000.0),
        "GiB/s" => Some(val * 1_073_741_824.0 / 1_000_000.0),
        _ => None,
    }
}

#[derive(Debug, Default, Clone)]
struct HardwareTagContext {
    arch: Option<String>,
    cpu_cluster: Option<String>,
    vlen_bits: Option<u32>,
}

impl HardwareTagContext {
    fn update_from_str(&mut self, text: &str) {
        for raw_token in text
            .split(|c: char| c.is_whitespace() || matches!(c, '[' | ']' | '(' | ')' | ',' | ';'))
        {
            let token = raw_token.trim();
            if token.is_empty() {
                continue;
            }
            if let Some((k, v)) = token.split_once('=') {
                let key = k.trim();
                let val = v.trim();
                if val.is_empty() {
                    continue;
                }
                match key {
                    "arch" => self.arch = Some(val.to_string()),
                    "cluster" | "cpu_cluster" => self.cpu_cluster = Some(val.to_string()),
                    "vlen" | "vlen_bits" => {
                        if let Ok(bits) = val.parse::<u32>() {
                            self.vlen_bits = Some(bits);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

fn parse_libtest_bench_line(line: &str, hw_ctx: &HardwareTagContext) -> Option<ParsedBenchmark> {
    let rest = line.trim().strip_prefix("test ")?;
    let (name_part, outcome_part) = rest.split_once(" ... ")?;
    let bench_part = outcome_part.trim().strip_prefix("bench:")?.trim();

    let mut local_hw = hw_ctx.clone();
    local_hw.update_from_str(line);

    let (ns_part, after_ns) = bench_part.split_once("ns/iter")?;
    let ns_per_iter = parse_number_clean(ns_part)?;

    let mut variance_ns = 0.0;
    if let Some(plus_minus_idx) = after_ns.find("+/-") {
        let after_pm = &after_ns[plus_minus_idx + 3..];
        let var_end = after_pm.find(')').unwrap_or(after_pm.len());
        if let Some(v) = parse_number_clean(&after_pm[..var_end]) {
            variance_ns = v;
        }
    }

    let mut throughput_mb_s = None;
    if let Some(eq_idx) = after_ns.find('=') {
        let after_eq = after_ns[eq_idx + 1..].trim();
        let tokens: Vec<&str> = after_eq.split_whitespace().collect();
        if tokens.len() >= 2 {
            if let Some(val) = parse_number_clean(tokens[0]) {
                let unit = tokens[1].trim_end_matches(')');
                throughput_mb_s = convert_throughput_to_mb_s(val, unit);
            }
        }
    }

    let clean_name = name_part
        .split_whitespace()
        .next()
        .unwrap_or(name_part.trim())
        .to_string();

    Some(ParsedBenchmark {
        bench_name: clean_name,
        ns_per_iter,
        variance_ns,
        throughput_mb_s,
        arch: local_hw.arch,
        cpu_cluster: local_hw.cpu_cluster,
        vlen_bits: local_hw.vlen_bits,
    })
}

fn parse_criterion_time_bracket(bracket_inner: &str) -> Option<(f64, f64)> {
    let tokens: Vec<&str> = bracket_inner.split_whitespace().collect();
    if tokens.len() >= 6 {
        let low_val = parse_number_clean(tokens[0])?;
        let low_ns = convert_time_to_ns(low_val, tokens[1])?;
        let est_val = parse_number_clean(tokens[2])?;
        let est_ns = convert_time_to_ns(est_val, tokens[3])?;
        let high_val = parse_number_clean(tokens[4])?;
        let high_ns = convert_time_to_ns(high_val, tokens[5])?;
        let variance_ns = ((high_ns - low_ns) / 2.0).max(0.0);
        Some((est_ns, variance_ns))
    } else {
        None
    }
}

fn parse_criterion_thrpt_bracket(bracket_inner: &str) -> Option<f64> {
    let tokens: Vec<&str> = bracket_inner.split_whitespace().collect();
    if tokens.len() >= 6 {
        let est_val = parse_number_clean(tokens[2])?;
        convert_throughput_to_mb_s(est_val, tokens[3])
    } else if tokens.len() >= 2 {
        let est_val = parse_number_clean(tokens[0])?;
        convert_throughput_to_mb_s(est_val, tokens[1])
    } else {
        None
    }
}

/// Parse standard Rust `libtest` and Criterion benchmark output into granular and aggregate metrics.
///
/// # Errors
///
/// Returns [`MetricParseError::InvalidUtf8`] if output is not valid UTF-8.
pub(crate) fn parse_bench_results(
    stdout: &[u8],
    stderr: &[u8],
) -> Result<ParsedBenchSuiteOutput, MetricParseError> {
    let out = std::str::from_utf8(stdout).map_err(|_| MetricParseError::InvalidUtf8)?;
    let err = std::str::from_utf8(stderr).map_err(|_| MetricParseError::InvalidUtf8)?;

    let mut benchmarks: Vec<ParsedBenchmark> = Vec::new();
    let mut hw_ctx = HardwareTagContext::default();
    let mut pending_criterion_name: Option<String> = None;

    for text in [out, err] {
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            hw_ctx.update_from_str(trimmed);

            if let Some(bench) = parse_libtest_bench_line(trimmed, &hw_ctx) {
                benchmarks.push(bench);
                pending_criterion_name = None;
                continue;
            }

            if let Some(time_idx) = trimmed.find("time:") {
                let prefix = trimmed[..time_idx].trim();
                let after_time = trimmed[time_idx + "time:".len()..].trim();
                if let Some(open_b) = after_time.find('[') {
                    if let Some(close_b) = after_time[open_b + 1..].find(']') {
                        let bracket_content = &after_time[open_b + 1..open_b + 1 + close_b];
                        if let Some((ns_per_iter, variance_ns)) =
                            parse_criterion_time_bracket(bracket_content)
                        {
                            let raw_name = if !prefix.is_empty() {
                                Some(prefix.to_string())
                            } else {
                                pending_criterion_name.take()
                            };
                            if let Some(name) = raw_name {
                                let clean_name = name
                                    .split_whitespace()
                                    .next()
                                    .unwrap_or(name.trim())
                                    .to_string();
                                let mut local_hw = hw_ctx.clone();
                                local_hw.update_from_str(trimmed);
                                benchmarks.push(ParsedBenchmark {
                                    bench_name: clean_name,
                                    ns_per_iter,
                                    variance_ns,
                                    throughput_mb_s: None,
                                    arch: local_hw.arch,
                                    cpu_cluster: local_hw.cpu_cluster,
                                    vlen_bits: local_hw.vlen_bits,
                                });
                            }
                        }
                    }
                }
                continue;
            }

            if let Some(thrpt_idx) = trimmed.find("thrpt:") {
                let after_thrpt = trimmed[thrpt_idx + "thrpt:".len()..].trim();
                if let Some(open_b) = after_thrpt.find('[') {
                    if let Some(close_b) = after_thrpt[open_b + 1..].find(']') {
                        let bracket_content = &after_thrpt[open_b + 1..open_b + 1 + close_b];
                        if let Some(mb_s) = parse_criterion_thrpt_bracket(bracket_content) {
                            if let Some(last) = benchmarks.last_mut() {
                                if last.throughput_mb_s.is_none() {
                                    last.throughput_mb_s = Some(mb_s);
                                }
                            }
                        }
                    }
                }
                continue;
            }

            // Track candidate Criterion benchmark header lines (e.g., `Benchmarking foo` or standalone `<bench_name>`)
            if let Some(rest) = trimmed.strip_prefix("Benchmarking ") {
                if !rest.starts_with("Warming up")
                    && !rest.starts_with("Collecting")
                    && !rest.starts_with("Analyzing")
                {
                    let name = rest.split(':').next().unwrap_or(rest).trim();
                    if !name.is_empty() {
                        pending_criterion_name = Some(name.to_string());
                    }
                }
            } else if !trimmed.contains(':')
                && !trimmed.starts_with("running ")
                && !trimmed.starts_with("Gnuplot ")
                && !trimmed.starts_with("Found ")
                && !trimmed.starts_with("change")
            {
                let first_tok = trimmed.split_whitespace().next().unwrap_or("");
                if !first_tok.is_empty() && !first_tok.contains('=') {
                    pending_criterion_name = Some(first_tok.to_string());
                }
            }
        }
    }

    let bench_ns_per_iter = if benchmarks.is_empty() {
        None
    } else {
        let sum: f64 = benchmarks.iter().map(|b| b.ns_per_iter).sum();
        Some(sum / benchmarks.len() as f64)
    };

    let thrpts: Vec<f64> = benchmarks
        .iter()
        .filter_map(|b| b.throughput_mb_s)
        .collect();
    let bench_mb_per_sec = if thrpts.is_empty() {
        None
    } else {
        let sum: f64 = thrpts.iter().sum();
        Some(sum / thrpts.len() as f64)
    };

    Ok(ParsedBenchSuiteOutput {
        bench_ns_per_iter,
        bench_mb_per_sec,
        benchmarks,
    })
}
