//! Compile-time [`ToolAdapter`] implementation for Trunk WASM web application bundler.

use std::ffi::OsString;

use crate::adapter::{
    SandboxProfileRequirement, ToolAdapter, ToolExecutionMetrics, ToolInvocation,
};
use crate::adapters::common::parse_flag_value;
use crate::error::{MetricParseError, ToolBuildError, ToolValidationError};
use crate::sanitize::{
    validate_invocation_base, validate_path_within_working_dir, validate_safe_relative_path,
};

/// Adapter for `trunk` WASM web application building.
#[derive(Debug, Default, Clone, Copy)]
pub struct TrunkAdapter;

impl ToolAdapter for TrunkAdapter {
    fn name(&self) -> &'static str {
        "trunk"
    }

    fn validate(&self, inv: &ToolInvocation) -> Result<(), ToolValidationError> {
        validate_invocation_base(inv)?;

        let mut i = 0;
        while i < inv.args.len() {
            let arg = &inv.args[i];

            // Explicitly reject prohibited non-build subcommands
            if matches!(arg.as_str(), "serve" | "watch" | "clean" | "config") {
                return Err(ToolValidationError::UnsupportedFlag {
                    tool: self.name().to_string(),
                    flag: arg.clone(),
                });
            }

            // Forbid proxying, backend hooks, or config overrides
            if arg.starts_with("--proxy-backend") || arg.contains("hook") {
                return Err(ToolValidationError::UnsupportedFlag {
                    tool: self.name().to_string(),
                    flag: arg.clone(),
                });
            }

            if i == 0 && arg == "build" {
                i += 1;
                continue;
            }

            if let Some(dist) = parse_flag_value(&inv.args, &mut i, None, "--dist", self.name())? {
                validate_safe_relative_path(dist)?;
                if let Some(ref wd) = inv.working_dir {
                    validate_path_within_working_dir(wd, dist)?;
                }
                i += 1;
                continue;
            }

            if parse_flag_value(&inv.args, &mut i, None, "--public-url", self.name())?.is_some() {
                i += 1;
                continue;
            }

            if let Some(filehash) =
                parse_flag_value(&inv.args, &mut i, None, "--filehash", self.name())?
            {
                if filehash != "true" && filehash != "false" {
                    return Err(ToolValidationError::InvalidArgValue {
                        tool: self.name().to_string(),
                        flag: "--filehash".to_string(),
                        value: filehash.to_string(),
                        reason: "must be `true` or `false`".to_string(),
                    });
                }
                i += 1;
                continue;
            }

            match arg.as_str() {
                "--release" | "--offline" => {}
                path if !path.starts_with('-') => {
                    validate_safe_relative_path(path)?;
                    if let Some(ref wd) = inv.working_dir {
                        validate_path_within_working_dir(wd, path)?;
                    }
                }
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
        let mut argv = vec![OsString::from("trunk"), OsString::from("build")];
        let start_idx = if inv.args.first().map(|s| s.as_str()) == Some("build") {
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
            writable_dirs: vec!["target".to_string(), "dist".to_string()],
            readable_dirs: vec![".".to_string()],
            exec_binaries: vec![
                "trunk".to_string(),
                "cargo".to_string(),
                "rustc".to_string(),
                "wasm-bindgen".to_string(),
            ],
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
            duration_ms: 0,
            exit_code,
            tests_passed: None,
            tests_failed: None,
            warnings_count: None,
            coverage_pct: None,
            artifacts: Vec::new(),
        })
    }
}
