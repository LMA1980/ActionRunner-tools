//! Internal argument parsing and metric parsing helpers for built-in tool adapters.

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

/// Parse `test result: ... X passed; Y failed` summary counts across stdout and stderr.
///
/// # Errors
///
/// Returns [`MetricParseError::InvalidUtf8`] if output is not valid UTF-8.
pub(crate) fn parse_test_results(
    stdout: &[u8],
    stderr: &[u8],
) -> Result<(Option<u32>, Option<u32>), MetricParseError> {
    let out = std::str::from_utf8(stdout).map_err(|_| MetricParseError::InvalidUtf8)?;
    let err = std::str::from_utf8(stderr).map_err(|_| MetricParseError::InvalidUtf8)?;

    let mut total_passed = 0u32;
    let mut total_failed = 0u32;
    let mut found = false;

    for text in [out, err] {
        for line in text.lines() {
            if let Some(pos) = line.find("test result:") {
                let rest = &line[pos + "test result:".len()..];
                let mut passed_val = None;
                let mut failed_val = None;
                let tokens: Vec<&str> = rest.split_whitespace().collect();
                for (i, &token) in tokens.iter().enumerate() {
                    let cleaned = token.trim_end_matches([';', ',']);
                    if cleaned == "passed" && i > 0 {
                        if let Ok(n) = tokens[i - 1].parse::<u32>() {
                            passed_val = Some(n);
                        }
                    } else if cleaned == "failed" && i > 0 {
                        if let Ok(n) = tokens[i - 1].parse::<u32>() {
                            failed_val = Some(n);
                        }
                    }
                }
                if let (Some(p), Some(f)) = (passed_val, failed_val) {
                    total_passed += p;
                    total_failed += f;
                    found = true;
                }
            }
        }
    }

    if found {
        Ok((Some(total_passed), Some(total_failed)))
    } else {
        Ok((None, None))
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
