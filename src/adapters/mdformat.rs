//! Compile-time [`ToolAdapter`] implementation for mdformat Markdown formatter.

use std::ffi::OsString;

use crate::adapter::{
    SandboxProfileRequirement, ToolAdapter, ToolExecutionMetrics, ToolInvocation,
};
use crate::adapters::common::parse_flag_value;
use crate::error::{MetricParseError, ToolBuildError, ToolValidationError};
use crate::sanitize::{
    validate_invocation_base, validate_path_within_working_dir, validate_safe_relative_path,
};

/// Adapter for `mdformat` Markdown formatting with CommonMark / GFM support.
#[derive(Debug, Default, Clone, Copy)]
pub struct MdformatAdapter;

impl ToolAdapter for MdformatAdapter {
    fn name(&self) -> &'static str {
        "mdformat"
    }

    fn validate(&self, inv: &ToolInvocation) -> Result<(), ToolValidationError> {
        validate_invocation_base(inv)?;

        let mut i = 0;
        while i < inv.args.len() {
            if let Some(wrap) = parse_flag_value(&inv.args, &mut i, None, "--wrap", self.name())? {
                if wrap != "keep" && wrap != "no" && wrap.parse::<u32>().is_err() {
                    return Err(ToolValidationError::InvalidArgValue {
                        tool: self.name().to_string(),
                        flag: "--wrap".to_string(),
                        value: wrap.to_string(),
                        reason: "must be `keep`, `no`, or a positive integer".to_string(),
                    });
                }
                i += 1;
                continue;
            }

            if let Some(eol) =
                parse_flag_value(&inv.args, &mut i, None, "--end-of-line", self.name())?
            {
                if eol != "lf" && eol != "crlf" && eol != "keep" {
                    return Err(ToolValidationError::InvalidArgValue {
                        tool: self.name().to_string(),
                        flag: "--end-of-line".to_string(),
                        value: eol.to_string(),
                        reason: "must be `lf`, `crlf`, or `keep`".to_string(),
                    });
                }
                i += 1;
                continue;
            }

            let arg = &inv.args[i];
            match arg.as_str() {
                "--check" | "--number" => {}
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
        let mut argv = vec![OsString::from("mdformat")];
        let mut has_path = false;

        let mut i = 0;
        while i < inv.args.len() {
            if parse_flag_value(&inv.args, &mut i, None, "--wrap", self.name())
                .ok()
                .flatten()
                .is_some()
                || parse_flag_value(&inv.args, &mut i, None, "--end-of-line", self.name())
                    .ok()
                    .flatten()
                    .is_some()
            {
                i += 1;
                continue;
            }
            let arg = &inv.args[i];
            if !arg.starts_with('-') {
                has_path = true;
            }
            i += 1;
        }

        for arg in &inv.args {
            argv.push(OsString::from(arg));
        }
        if !has_path {
            argv.push(OsString::from("."));
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
            readable_dirs: vec![".".to_string(), "/var/lib/action-runner/.local".to_string()],
            exec_binaries: vec!["mdformat".to_string(), "python3".to_string()],
            required_memlock_kb: 32_768,
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
