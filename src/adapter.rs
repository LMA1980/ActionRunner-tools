//! Core `ToolAdapter` trait and structured invocation, sandbox, and metrics types.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{MetricParseError, ToolBuildError, ToolValidationError};

/// Structured representation of a requested tool execution.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Default)]
pub struct ToolInvocation {
    /// Canonical tool identifier (e.g., `"cargo-clippy"`, `"cargo-test"`).
    pub tool_name: String,
    /// Structured command-line arguments passed to the tool adapter.
    pub args: Vec<String>,
    /// Optional workspace working directory.
    pub working_dir: Option<PathBuf>,
    /// Environment variables passed to the tool invocation.
    pub env_vars: BTreeMap<String, String>,
}

impl ToolInvocation {
    /// Create a new [`ToolInvocation`] with the given tool name and argument list.
    #[must_use]
    pub fn new(
        tool_name: impl Into<String>,
        args: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            tool_name: tool_name.into(),
            args: args.into_iter().map(Into::into).collect(),
            working_dir: None,
            env_vars: BTreeMap::new(),
        }
    }

    /// Attach a working directory to the invocation.
    #[must_use]
    pub fn with_working_dir(mut self, working_dir: impl Into<PathBuf>) -> Self {
        self.working_dir = Some(working_dir.into());
        self
    }

    /// Insert an environment variable into the invocation.
    #[must_use]
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_vars.insert(key.into(), value.into());
        self
    }
}

/// Declarative sandbox requirements advertised by a [`ToolAdapter`].
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Default)]
pub struct SandboxProfileRequirement {
    /// Whether the tool is permitted outbound network access.
    pub allow_network: bool,
    /// Explicit allowlist of outbound hosts permitted when `allow_network` is `true`.
    pub allowed_egress_hosts: Vec<String>,
    /// Workspace or temporary directories that the tool may write to.
    pub writable_dirs: Vec<String>,
    /// System, toolchain, or workspace directories that the tool may read from.
    pub readable_dirs: Vec<String>,
    /// Executables that the sandboxed process is permitted to `execve`.
    pub exec_binaries: Vec<String>,
    /// Required locked-memory budget (`RLIMIT_MEMLOCK`) in KiB.
    pub required_memlock_kb: u64,
}

/// Structured metrics extracted after a tool execution completes.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Default)]
pub struct ToolExecutionMetrics {
    /// Execution duration in milliseconds.
    pub duration_ms: u64,
    /// Process exit status code.
    pub exit_code: i32,
    /// Number of unit/integration tests that passed, if applicable.
    pub tests_passed: Option<u32>,
    /// Number of unit/integration tests that failed, if applicable.
    pub tests_failed: Option<u32>,
    /// Number of compiler or linter warnings emitted, if applicable.
    pub warnings_count: Option<u32>,
    /// Line or region test coverage percentage (`0.0..=100.0`), if applicable.
    pub coverage_pct: Option<f64>,
    /// Relative paths or identifiers of generated build/coverage artifacts.
    pub artifacts: Vec<String>,
}

/// Core interface for compile-time typed CI/CD tool execution without shell interpolation.
pub trait ToolAdapter: Send + Sync {
    /// Canonical adapter identifier (e.g., `"cargo-clippy"`, `"cargo-llvm-cov"`).
    fn name(&self) -> &'static str;

    /// Validate all arguments, paths, and environment variables in `inv`.
    ///
    /// # Errors
    ///
    /// Returns [`ToolValidationError`] if any argument, path, or environment variable
    /// violates the adapter's strict allowlist or sanitization rules.
    fn validate(&self, inv: &ToolInvocation) -> Result<(), ToolValidationError>;

    /// Construct a deterministic `argv` vector suitable for direct `execve` without a shell.
    ///
    /// The first element of the returned vector is the executable name or path, followed
    /// by fixed subcommands and validated flags.
    ///
    /// # Errors
    ///
    /// Returns [`ToolBuildError`] if validation fails or the `argv` vector cannot be constructed.
    fn build_argv(&self, inv: &ToolInvocation) -> Result<Vec<OsString>, ToolBuildError>;

    /// Return the declarative sandbox profile required to execute `inv`.
    fn sandbox_profile(&self, inv: &ToolInvocation) -> SandboxProfileRequirement;

    /// Parse structured execution metrics from `stdout`, `stderr`, and `exit_code`.
    ///
    /// # Errors
    ///
    /// Returns [`MetricParseError`] if `stdout` or `stderr` is not valid UTF-8 or fails
    /// expected format parsing.
    fn parse_metrics(
        &self,
        stdout: &[u8],
        stderr: &[u8],
        exit_code: i32,
    ) -> Result<ToolExecutionMetrics, MetricParseError>;
}

impl std::fmt::Debug for dyn ToolAdapter + '_ {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolAdapter")
            .field("name", &self.name())
            .finish()
    }
}
