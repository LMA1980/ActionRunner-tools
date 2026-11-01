//! Compile-time [`ToolAdapter`] implementations for Cargo commands.

use std::ffi::OsString;

use crate::adapter::{
    SandboxProfileRequirement, ToolAdapter, ToolExecutionMetrics, ToolInvocation,
};
use crate::adapters::common::{
    count_occurrences, parse_bench_results, parse_coverage_pct, parse_flag_value,
    parse_test_results, split_dash_dash,
};
use crate::error::{MetricParseError, ToolBuildError, ToolValidationError};
use crate::sanitize::{
    validate_invocation_base, validate_path_within_working_dir, validate_safe_relative_path,
};

/// Adapter for `cargo fmt` checking and formatting.
#[derive(Debug, Default, Clone, Copy)]
pub struct CargoFmtAdapter;

impl ToolAdapter for CargoFmtAdapter {
    fn name(&self) -> &'static str {
        "cargo-fmt"
    }

    fn validate(&self, inv: &ToolInvocation) -> Result<(), ToolValidationError> {
        validate_invocation_base(inv)?;
        let (cargo_args, dash_dash) = split_dash_dash(&inv.args);

        let mut i = 0;
        while i < cargo_args.len() {
            if parse_flag_value(cargo_args, &mut i, Some("-p"), "--package", self.name())?.is_some()
            {
                i += 1;
                continue;
            }
            let arg = &cargo_args[i];
            match arg.as_str() {
                "--all" | "--check" => {}
                _ => {
                    return Err(ToolValidationError::UnsupportedFlag {
                        tool: self.name().to_string(),
                        flag: arg.clone(),
                    });
                }
            }
            i += 1;
        }

        if let Some(dd) = dash_dash {
            for arg in dd {
                if arg != "--check" {
                    return Err(ToolValidationError::UnsupportedFlag {
                        tool: self.name().to_string(),
                        flag: arg.clone(),
                    });
                }
            }
        }

        Ok(())
    }

    fn build_argv(&self, inv: &ToolInvocation) -> Result<Vec<OsString>, ToolBuildError> {
        self.validate(inv)?;
        let mut argv = vec![OsString::from("cargo"), OsString::from("fmt")];
        for arg in &inv.args {
            argv.push(OsString::from(arg));
        }
        Ok(argv)
    }

    fn sandbox_profile(&self, inv: &ToolInvocation) -> SandboxProfileRequirement {
        let is_check = inv.args.iter().any(|a| a == "--check");
        SandboxProfileRequirement {
            allow_network: false,
            allowed_egress_hosts: Vec::new(),
            writable_dirs: if is_check {
                Vec::new()
            } else {
                vec![".".to_string()]
            },
            readable_dirs: vec![".".to_string()],
            exec_binaries: vec!["cargo".to_string(), "rustfmt".to_string()],
            required_memlock_kb: 65_536,
        }
    }

    fn parse_metrics(
        &self,
        _stdout: &[u8],
        _stderr: &[u8],
        exit_code: i32,
    ) -> Result<ToolExecutionMetrics, MetricParseError> {
        Ok(ToolExecutionMetrics {
            exit_code,
            ..ToolExecutionMetrics::default()
        })
    }
}

/// Adapter for `cargo clippy` code linting.
#[derive(Debug, Default, Clone, Copy)]
pub struct CargoClippyAdapter;

impl ToolAdapter for CargoClippyAdapter {
    fn name(&self) -> &'static str {
        "cargo-clippy"
    }

    fn validate(&self, inv: &ToolInvocation) -> Result<(), ToolValidationError> {
        validate_invocation_base(inv)?;
        let (cargo_args, dash_dash) = split_dash_dash(&inv.args);

        let mut i = 0;
        while i < cargo_args.len() {
            if parse_flag_value(cargo_args, &mut i, Some("-p"), "--package", self.name())?.is_some()
                || parse_flag_value(cargo_args, &mut i, None, "--features", self.name())?.is_some()
                || parse_flag_value(cargo_args, &mut i, None, "--target", self.name())?.is_some()
            {
                i += 1;
                continue;
            }
            let arg = &cargo_args[i];
            match arg.as_str() {
                "--workspace"
                | "--all-targets"
                | "--all-features"
                | "--no-default-features"
                | "--bins"
                | "--lib"
                | "--tests" => {}
                _ => {
                    return Err(ToolValidationError::UnsupportedFlag {
                        tool: self.name().to_string(),
                        flag: arg.clone(),
                    });
                }
            }
            i += 1;
        }

        if let Some(dd) = dash_dash {
            let mut j = 0;
            while j < dd.len() {
                let arg = &dd[j];
                if matches!(
                    arg.as_str(),
                    "-D" | "-W" | "-A" | "-F" | "--deny" | "--warn" | "--allow" | "--forbid"
                ) {
                    if j + 1 >= dd.len() {
                        return Err(ToolValidationError::MissingRequiredArg {
                            tool: self.name().to_string(),
                            expected: arg.clone(),
                        });
                    }
                    let lint = &dd[j + 1];
                    if lint.starts_with('-') {
                        return Err(ToolValidationError::InvalidArgValue {
                            tool: self.name().to_string(),
                            flag: arg.clone(),
                            value: lint.clone(),
                            reason: "lint name cannot start with '-'".to_string(),
                        });
                    }
                    j += 2;
                    continue;
                } else if arg.starts_with("-D")
                    || arg.starts_with("-W")
                    || arg.starts_with("-A")
                    || arg.starts_with("-F")
                    || arg == "warnings"
                {
                    j += 1;
                    continue;
                }
                return Err(ToolValidationError::UnsupportedFlag {
                    tool: self.name().to_string(),
                    flag: arg.clone(),
                });
            }
        }

        Ok(())
    }

    fn build_argv(&self, inv: &ToolInvocation) -> Result<Vec<OsString>, ToolBuildError> {
        self.validate(inv)?;
        let mut argv = vec![OsString::from("cargo"), OsString::from("clippy")];
        for arg in &inv.args {
            argv.push(OsString::from(arg));
        }
        Ok(argv)
    }

    fn sandbox_profile(&self, _inv: &ToolInvocation) -> SandboxProfileRequirement {
        SandboxProfileRequirement {
            allow_network: false,
            allowed_egress_hosts: Vec::new(),
            writable_dirs: vec!["target".to_string()],
            readable_dirs: vec![".".to_string()],
            exec_binaries: vec![
                "cargo".to_string(),
                "rustc".to_string(),
                "clippy-driver".to_string(),
            ],
            required_memlock_kb: 65_536,
        }
    }

    fn parse_metrics(
        &self,
        stdout: &[u8],
        stderr: &[u8],
        exit_code: i32,
    ) -> Result<ToolExecutionMetrics, MetricParseError> {
        let warnings = count_occurrences(stdout, stderr, "warning:")?;
        Ok(ToolExecutionMetrics {
            exit_code,
            warnings_count: Some(warnings),
            ..ToolExecutionMetrics::default()
        })
    }
}

/// Adapter for `cargo test` execution.
#[derive(Debug, Default, Clone, Copy)]
pub struct CargoTestAdapter;

impl ToolAdapter for CargoTestAdapter {
    fn name(&self) -> &'static str {
        "cargo-test"
    }

    fn validate(&self, inv: &ToolInvocation) -> Result<(), ToolValidationError> {
        validate_invocation_base(inv)?;
        let (cargo_args, dash_dash) = split_dash_dash(&inv.args);

        let mut i = 0;
        while i < cargo_args.len() {
            if parse_flag_value(cargo_args, &mut i, Some("-p"), "--package", self.name())?.is_some()
                || parse_flag_value(cargo_args, &mut i, None, "--bin", self.name())?.is_some()
                || parse_flag_value(cargo_args, &mut i, None, "--test", self.name())?.is_some()
                || parse_flag_value(cargo_args, &mut i, None, "--features", self.name())?.is_some()
                || parse_flag_value(cargo_args, &mut i, None, "--target", self.name())?.is_some()
            {
                i += 1;
                continue;
            }
            let arg = &cargo_args[i];
            match arg.as_str() {
                "--workspace"
                | "--lib"
                | "--bins"
                | "--tests"
                | "--doc"
                | "--all-targets"
                | "--all-features"
                | "--no-default-features"
                | "--no-run"
                | "--locked"
                | "--offline" => {}
                _ => {
                    return Err(ToolValidationError::UnsupportedFlag {
                        tool: self.name().to_string(),
                        flag: arg.clone(),
                    });
                }
            }
            i += 1;
        }

        if let Some(dd) = dash_dash {
            let mut j = 0;
            while j < dd.len() {
                if parse_flag_value(dd, &mut j, None, "--test-threads", self.name())?.is_some() {
                    j += 1;
                    continue;
                }
                let arg = &dd[j];
                if matches!(
                    arg.as_str(),
                    "--nocapture" | "--include-ignored" | "--ignored"
                ) || !arg.starts_with('-')
                {
                    j += 1;
                    continue;
                }
                return Err(ToolValidationError::UnsupportedFlag {
                    tool: self.name().to_string(),
                    flag: arg.clone(),
                });
            }
        }

        Ok(())
    }

    fn build_argv(&self, inv: &ToolInvocation) -> Result<Vec<OsString>, ToolBuildError> {
        self.validate(inv)?;
        let mut argv = vec![OsString::from("cargo"), OsString::from("test")];
        for arg in &inv.args {
            argv.push(OsString::from(arg));
        }
        Ok(argv)
    }

    fn sandbox_profile(&self, _inv: &ToolInvocation) -> SandboxProfileRequirement {
        SandboxProfileRequirement {
            allow_network: false,
            allowed_egress_hosts: Vec::new(),
            writable_dirs: vec!["target".to_string(), "/tmp".to_string()],
            readable_dirs: vec![".".to_string()],
            exec_binaries: vec!["cargo".to_string(), "rustc".to_string()],
            required_memlock_kb: 65_536,
        }
    }

    fn parse_metrics(
        &self,
        stdout: &[u8],
        stderr: &[u8],
        exit_code: i32,
    ) -> Result<ToolExecutionMetrics, MetricParseError> {
        let parsed = parse_test_results(stdout, stderr)?;
        Ok(ToolExecutionMetrics {
            exit_code,
            tests_passed: parsed.passed,
            tests_failed: parsed.failed,
            tests_ignored: parsed.ignored,
            test_cases: parsed.test_cases,
            ..ToolExecutionMetrics::default()
        })
    }
}

/// Adapter for `cargo bench` benchmarking.
#[derive(Debug, Default, Clone, Copy)]
pub struct CargoBenchAdapter;

impl ToolAdapter for CargoBenchAdapter {
    fn name(&self) -> &'static str {
        "cargo-bench"
    }

    fn validate(&self, inv: &ToolInvocation) -> Result<(), ToolValidationError> {
        validate_invocation_base(inv)?;
        let (cargo_args, dash_dash) = split_dash_dash(&inv.args);

        let mut i = 0;
        while i < cargo_args.len() {
            if parse_flag_value(cargo_args, &mut i, Some("-p"), "--package", self.name())?.is_some()
                || parse_flag_value(cargo_args, &mut i, None, "--bench", self.name())?.is_some()
                || parse_flag_value(cargo_args, &mut i, None, "--features", self.name())?.is_some()
                || parse_flag_value(cargo_args, &mut i, None, "--target", self.name())?.is_some()
            {
                i += 1;
                continue;
            }
            let arg = &cargo_args[i];
            match arg.as_str() {
                "--workspace" | "--benches" | "--no-run" | "--all-features" => {}
                _ => {
                    return Err(ToolValidationError::UnsupportedFlag {
                        tool: self.name().to_string(),
                        flag: arg.clone(),
                    });
                }
            }
            i += 1;
        }

        if let Some(dd) = dash_dash {
            for arg in dd {
                if arg != "--nocapture" && arg.starts_with('-') {
                    return Err(ToolValidationError::UnsupportedFlag {
                        tool: self.name().to_string(),
                        flag: arg.clone(),
                    });
                }
            }
        }

        Ok(())
    }

    fn build_argv(&self, inv: &ToolInvocation) -> Result<Vec<OsString>, ToolBuildError> {
        self.validate(inv)?;
        let mut argv = vec![OsString::from("cargo"), OsString::from("bench")];
        for arg in &inv.args {
            argv.push(OsString::from(arg));
        }
        Ok(argv)
    }

    fn sandbox_profile(&self, _inv: &ToolInvocation) -> SandboxProfileRequirement {
        SandboxProfileRequirement {
            allow_network: false,
            allowed_egress_hosts: Vec::new(),
            writable_dirs: vec!["target".to_string()],
            readable_dirs: vec![".".to_string()],
            exec_binaries: vec!["cargo".to_string(), "rustc".to_string()],
            required_memlock_kb: 65_536,
        }
    }

    fn parse_metrics(
        &self,
        stdout: &[u8],
        stderr: &[u8],
        exit_code: i32,
    ) -> Result<ToolExecutionMetrics, MetricParseError> {
        let parsed = parse_bench_results(stdout, stderr)?;
        Ok(ToolExecutionMetrics {
            exit_code,
            bench_ns_per_iter: parsed.bench_ns_per_iter,
            bench_mb_per_sec: parsed.bench_mb_per_sec,
            benchmarks: parsed.benchmarks,
            ..ToolExecutionMetrics::default()
        })
    }
}

/// Adapter for `cargo audit` security vulnerability checks.
#[derive(Debug, Default, Clone, Copy)]
pub struct CargoAuditAdapter;

impl ToolAdapter for CargoAuditAdapter {
    fn name(&self) -> &'static str {
        "cargo-audit"
    }

    fn validate(&self, inv: &ToolInvocation) -> Result<(), ToolValidationError> {
        validate_invocation_base(inv)?;

        let mut i = 0;
        while i < inv.args.len() {
            if parse_flag_value(&inv.args, &mut i, Some("-D"), "--deny", self.name())?.is_some()
                || parse_flag_value(&inv.args, &mut i, None, "--ignore", self.name())?.is_some()
                || parse_flag_value(&inv.args, &mut i, None, "--db", self.name())?.is_some()
            {
                i += 1;
                continue;
            }

            if let Some(file_path) =
                parse_flag_value(&inv.args, &mut i, None, "--file", self.name())?
            {
                validate_safe_relative_path(file_path)?;
                if let Some(ref wd) = inv.working_dir {
                    validate_path_within_working_dir(wd, file_path)?;
                }
                i += 1;
                continue;
            }

            let arg = &inv.args[i];
            match arg.as_str() {
                "--json" | "--no-fetch" => {}
                _ => {
                    return Err(ToolValidationError::UnsupportedFlag {
                        tool: self.name().to_string(),
                        flag: arg.clone(),
                    });
                }
            }
            i += 1;
        }

        Ok(())
    }

    fn build_argv(&self, inv: &ToolInvocation) -> Result<Vec<OsString>, ToolBuildError> {
        self.validate(inv)?;
        let has_db = inv
            .args
            .iter()
            .any(|a| a == "--db" || a.starts_with("--db="));
        let has_no_fetch = inv.args.iter().any(|a| a == "--no-fetch");

        let mut argv = vec![OsString::from("cargo"), OsString::from("audit")];
        if !has_db {
            argv.push(OsString::from("--db"));
            argv.push(OsString::from("/var/lib/action-runner/.cargo/advisory-db"));
        }
        if !has_no_fetch {
            argv.push(OsString::from("--no-fetch"));
        }
        for arg in &inv.args {
            argv.push(OsString::from(arg));
        }
        Ok(argv)
    }

    fn sandbox_profile(&self, _inv: &ToolInvocation) -> SandboxProfileRequirement {
        SandboxProfileRequirement {
            allow_network: false,
            allowed_egress_hosts: Vec::new(),
            writable_dirs: Vec::new(),
            readable_dirs: vec![
                ".".to_string(),
                "/var/lib/action-runner/.cargo/advisory-db".to_string(),
            ],
            exec_binaries: vec!["cargo".to_string(), "cargo-audit".to_string()],
            required_memlock_kb: 65_536,
        }
    }

    fn parse_metrics(
        &self,
        _stdout: &[u8],
        _stderr: &[u8],
        exit_code: i32,
    ) -> Result<ToolExecutionMetrics, MetricParseError> {
        Ok(ToolExecutionMetrics {
            exit_code,
            ..ToolExecutionMetrics::default()
        })
    }
}

/// Adapter for `cargo llvm-cov` code coverage reporting.
#[derive(Debug, Default, Clone, Copy)]
pub struct CargoLlvmCovAdapter;

impl ToolAdapter for CargoLlvmCovAdapter {
    fn name(&self) -> &'static str {
        "cargo-llvm-cov"
    }

    fn validate(&self, inv: &ToolInvocation) -> Result<(), ToolValidationError> {
        validate_invocation_base(inv)?;

        let mut i = 0;
        while i < inv.args.len() {
            if parse_flag_value(&inv.args, &mut i, Some("-p"), "--package", self.name())?.is_some()
                || parse_flag_value(&inv.args, &mut i, None, "--test", self.name())?.is_some()
            {
                i += 1;
                continue;
            }

            if let Some(out_p) =
                parse_flag_value(&inv.args, &mut i, None, "--output-path", self.name())?
            {
                validate_safe_relative_path(out_p)?;
                if let Some(ref wd) = inv.working_dir {
                    validate_path_within_working_dir(wd, out_p)?;
                }
                i += 1;
                continue;
            }

            if let Some(pct) =
                parse_flag_value(&inv.args, &mut i, None, "--fail-under-lines", self.name())?
            {
                if pct
                    .parse::<f64>()
                    .map_or(true, |v| !(0.0..=100.0).contains(&v))
                {
                    return Err(ToolValidationError::InvalidArgValue {
                        tool: self.name().to_string(),
                        flag: "--fail-under-lines".to_string(),
                        value: pct.to_string(),
                        reason: "percentage must be between 0.0 and 100.0".to_string(),
                    });
                }
                i += 1;
                continue;
            }

            let arg = &inv.args[i];
            match arg.as_str() {
                "--workspace" | "--all-features" | "--lcov" | "--json" | "--summary-only"
                | "--lib" | "--bins" => {}
                _ => {
                    return Err(ToolValidationError::UnsupportedFlag {
                        tool: self.name().to_string(),
                        flag: arg.clone(),
                    });
                }
            }
            i += 1;
        }

        Ok(())
    }

    fn build_argv(&self, inv: &ToolInvocation) -> Result<Vec<OsString>, ToolBuildError> {
        self.validate(inv)?;
        let mut argv = vec![OsString::from("cargo"), OsString::from("llvm-cov")];
        for arg in &inv.args {
            argv.push(OsString::from(arg));
        }
        Ok(argv)
    }

    fn sandbox_profile(&self, _inv: &ToolInvocation) -> SandboxProfileRequirement {
        SandboxProfileRequirement {
            allow_network: false,
            allowed_egress_hosts: Vec::new(),
            writable_dirs: vec!["target".to_string()],
            readable_dirs: vec![".".to_string()],
            exec_binaries: vec![
                "cargo".to_string(),
                "cargo-llvm-cov".to_string(),
                "rustc".to_string(),
            ],
            required_memlock_kb: 65_536,
        }
    }

    fn parse_metrics(
        &self,
        stdout: &[u8],
        stderr: &[u8],
        exit_code: i32,
    ) -> Result<ToolExecutionMetrics, MetricParseError> {
        let coverage = parse_coverage_pct(stdout, stderr)?;
        Ok(ToolExecutionMetrics {
            exit_code,
            coverage_pct: coverage,
            ..ToolExecutionMetrics::default()
        })
    }
}

/// Adapter for `cargo miri` undefined-behavior detection.
#[derive(Debug, Default, Clone, Copy)]
pub struct CargoMiriAdapter;

impl ToolAdapter for CargoMiriAdapter {
    fn name(&self) -> &'static str {
        "cargo-miri"
    }

    fn validate(&self, inv: &ToolInvocation) -> Result<(), ToolValidationError> {
        validate_invocation_base(inv)?;
        let (cargo_args, dash_dash) = split_dash_dash(&inv.args);

        let mut i = 0;
        while i < cargo_args.len() {
            if i == 0 && cargo_args[0] == "test" {
                i += 1;
                continue;
            }
            if parse_flag_value(cargo_args, &mut i, Some("-p"), "--package", self.name())?.is_some()
                || parse_flag_value(cargo_args, &mut i, None, "--test", self.name())?.is_some()
                || parse_flag_value(cargo_args, &mut i, None, "--features", self.name())?.is_some()
                || parse_flag_value(cargo_args, &mut i, None, "--target", self.name())?.is_some()
            {
                i += 1;
                continue;
            }
            let arg = &cargo_args[i];
            match arg.as_str() {
                "--workspace" | "--lib" | "--bins" | "--all-features" => {}
                _ => {
                    return Err(ToolValidationError::UnsupportedFlag {
                        tool: self.name().to_string(),
                        flag: arg.clone(),
                    });
                }
            }
            i += 1;
        }

        if let Some(dd) = dash_dash {
            for arg in dd {
                if arg != "--nocapture" && arg.starts_with('-') {
                    return Err(ToolValidationError::UnsupportedFlag {
                        tool: self.name().to_string(),
                        flag: arg.clone(),
                    });
                }
            }
        }

        Ok(())
    }

    fn build_argv(&self, inv: &ToolInvocation) -> Result<Vec<OsString>, ToolBuildError> {
        self.validate(inv)?;
        let mut argv = vec![
            OsString::from("cargo"),
            OsString::from("+nightly"),
            OsString::from("miri"),
            OsString::from("test"),
        ];

        let start_idx = if inv.args.first().map(|s| s.as_str()) == Some("test") {
            1
        } else {
            0
        };

        for arg in &inv.args[start_idx..] {
            argv.push(OsString::from(arg));
        }
        Ok(argv)
    }

    fn sandbox_profile(&self, _inv: &ToolInvocation) -> SandboxProfileRequirement {
        SandboxProfileRequirement {
            allow_network: false,
            allowed_egress_hosts: Vec::new(),
            writable_dirs: vec!["target".to_string()],
            readable_dirs: vec![
                ".".to_string(),
                "/var/lib/action-runner/.rustup".to_string(),
            ],
            exec_binaries: vec![
                "cargo".to_string(),
                "cargo-miri".to_string(),
                "miri".to_string(),
                "rustc".to_string(),
            ],
            required_memlock_kb: 65_536,
        }
    }

    fn parse_metrics(
        &self,
        stdout: &[u8],
        stderr: &[u8],
        exit_code: i32,
    ) -> Result<ToolExecutionMetrics, MetricParseError> {
        let parsed = parse_test_results(stdout, stderr)?;
        Ok(ToolExecutionMetrics {
            exit_code,
            tests_passed: parsed.passed,
            tests_failed: parsed.failed,
            tests_ignored: parsed.ignored,
            test_cases: parsed.test_cases,
            ..ToolExecutionMetrics::default()
        })
    }
}

/// Adapter for `cargo publish` package publishing to MuRegist.
#[derive(Debug, Default, Clone, Copy)]
pub struct CargoPublishAdapter;

impl ToolAdapter for CargoPublishAdapter {
    fn name(&self) -> &'static str {
        "cargo-publish"
    }

    fn validate(&self, inv: &ToolInvocation) -> Result<(), ToolValidationError> {
        validate_invocation_base(inv)?;

        let mut i = 0;
        while i < inv.args.len() {
            if parse_flag_value(&inv.args, &mut i, Some("-p"), "--package", self.name())?.is_some()
                || parse_flag_value(&inv.args, &mut i, None, "--target", self.name())?.is_some()
                || parse_flag_value(&inv.args, &mut i, None, "--features", self.name())?.is_some()
            {
                i += 1;
                continue;
            }

            if let Some(registry) =
                parse_flag_value(&inv.args, &mut i, None, "--registry", self.name())?
            {
                if registry != "muregist" {
                    return Err(ToolValidationError::InvalidArgValue {
                        tool: self.name().to_string(),
                        flag: "--registry".to_string(),
                        value: registry.to_string(),
                        reason: "only internal registry `muregist` is permitted".to_string(),
                    });
                }
                i += 1;
                continue;
            }

            let arg = &inv.args[i];
            match arg.as_str() {
                "--dry-run" | "--no-verify" | "--allow-dirty" | "--locked" | "--all-features" => {}
                _ => {
                    return Err(ToolValidationError::UnsupportedFlag {
                        tool: self.name().to_string(),
                        flag: arg.clone(),
                    });
                }
            }
            i += 1;
        }

        Ok(())
    }

    fn build_argv(&self, inv: &ToolInvocation) -> Result<Vec<OsString>, ToolBuildError> {
        self.validate(inv)?;
        let has_registry = inv
            .args
            .iter()
            .any(|a| a == "--registry" || a.starts_with("--registry="));

        let mut argv = vec![OsString::from("cargo"), OsString::from("publish")];
        if !has_registry {
            argv.push(OsString::from("--registry"));
            argv.push(OsString::from("muregist"));
        }
        for arg in &inv.args {
            argv.push(OsString::from(arg));
        }
        Ok(argv)
    }

    fn sandbox_profile(&self, _inv: &ToolInvocation) -> SandboxProfileRequirement {
        SandboxProfileRequirement {
            allow_network: true,
            allowed_egress_hosts: vec!["registry.ci.ci.internal:8444".to_string()],
            writable_dirs: vec!["target".to_string()],
            readable_dirs: vec![".".to_string()],
            exec_binaries: vec!["cargo".to_string(), "rustc".to_string()],
            required_memlock_kb: 65_536,
        }
    }

    fn parse_metrics(
        &self,
        _stdout: &[u8],
        _stderr: &[u8],
        exit_code: i32,
    ) -> Result<ToolExecutionMetrics, MetricParseError> {
        Ok(ToolExecutionMetrics {
            exit_code,
            ..ToolExecutionMetrics::default()
        })
    }
}

/// Adapter for `cargo build` compilation.
#[derive(Debug, Default, Clone, Copy)]
pub struct CargoBuildAdapter;

impl ToolAdapter for CargoBuildAdapter {
    fn name(&self) -> &'static str {
        "cargo-build"
    }

    fn validate(&self, inv: &ToolInvocation) -> Result<(), ToolValidationError> {
        validate_invocation_base(inv)?;

        let mut i = 0;
        while i < inv.args.len() {
            if parse_flag_value(&inv.args, &mut i, Some("-p"), "--package", self.name())?.is_some()
                || parse_flag_value(&inv.args, &mut i, None, "--bin", self.name())?.is_some()
                || parse_flag_value(&inv.args, &mut i, None, "--features", self.name())?.is_some()
                || parse_flag_value(&inv.args, &mut i, None, "--target", self.name())?.is_some()
            {
                i += 1;
                continue;
            }

            let arg = &inv.args[i];
            match arg.as_str() {
                "--workspace"
                | "--release"
                | "--bins"
                | "--lib"
                | "--tests"
                | "--all-targets"
                | "--all-features"
                | "--no-default-features"
                | "--locked"
                | "--offline" => {}
                _ => {
                    return Err(ToolValidationError::UnsupportedFlag {
                        tool: self.name().to_string(),
                        flag: arg.clone(),
                    });
                }
            }
            i += 1;
        }

        Ok(())
    }

    fn build_argv(&self, inv: &ToolInvocation) -> Result<Vec<OsString>, ToolBuildError> {
        self.validate(inv)?;
        let mut argv = vec![OsString::from("cargo"), OsString::from("build")];
        for arg in &inv.args {
            argv.push(OsString::from(arg));
        }
        Ok(argv)
    }

    fn sandbox_profile(&self, _inv: &ToolInvocation) -> SandboxProfileRequirement {
        SandboxProfileRequirement {
            allow_network: false,
            allowed_egress_hosts: Vec::new(),
            writable_dirs: vec!["target".to_string()],
            readable_dirs: vec![".".to_string()],
            exec_binaries: vec!["cargo".to_string(), "rustc".to_string()],
            required_memlock_kb: 65_536,
        }
    }

    fn parse_metrics(
        &self,
        _stdout: &[u8],
        _stderr: &[u8],
        exit_code: i32,
    ) -> Result<ToolExecutionMetrics, MetricParseError> {
        Ok(ToolExecutionMetrics {
            exit_code,
            ..ToolExecutionMetrics::default()
        })
    }
}
