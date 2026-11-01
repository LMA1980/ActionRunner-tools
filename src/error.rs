//! Error types for tool validation, deterministic argv building, and metric parsing.

use thiserror::Error;

/// Errors raised when validating a [`crate::adapter::ToolInvocation`] or its arguments.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ToolValidationError {
    /// The requested tool name is not registered in the [`crate::registry::ToolRegistry`].
    #[error("unknown or unregistered tool: `{0}`")]
    UnknownTool(String),

    /// A shell metacharacter or operator was detected in an argument or environment variable.
    #[error("shell metacharacter `{token}` detected in `{arg}`")]
    ShellMetacharacterDetected {
        /// The offending argument or environment string.
        arg: String,
        /// The prohibited metacharacter or token sequence.
        token: String,
    },

    /// A prohibited shell wrapper, interpreter, or wrapper flag was detected.
    #[error("prohibited shell wrapper or execution flag: `{0}`")]
    ProhibitedShellWrapper(String),

    /// Directory traversal (`..`) or symlink escape out of the workspace was detected.
    #[error("path traversal detected in `{0}`")]
    PathTraversalDetected(String),

    /// An absolute path was supplied where a workspace-relative path is required.
    #[error("absolute path is forbidden: `{0}`")]
    AbsolutePathForbidden(String),

    /// An ASCII control character (including NUL, CR, or LF) was detected.
    #[error("control character detected in `{0}`")]
    ControlCharacterDetected(String),

    /// A flag or subcommand is not on the adapter's strict allowlist.
    #[error("unsupported flag `{flag}` for tool `{tool}`")]
    UnsupportedFlag {
        /// Canonical tool adapter name.
        tool: String,
        /// The rejected flag or argument.
        flag: String,
    },

    /// A required argument or flag value was omitted.
    #[error("missing required argument `{expected}` for tool `{tool}`")]
    MissingRequiredArg {
        /// Canonical tool adapter name.
        tool: String,
        /// Description or name of the expected argument.
        expected: String,
    },

    /// A flag value failed type, range, or identifier validation.
    #[error("invalid value `{value}` for flag `{flag}` in tool `{tool}`: {reason}")]
    InvalidArgValue {
        /// Canonical tool adapter name.
        tool: String,
        /// Flag name.
        flag: String,
        /// Provided value.
        value: String,
        /// Explanation of why the value was rejected.
        reason: String,
    },
}

/// Errors raised when constructing the deterministic argument vector for a tool.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ToolBuildError {
    /// Argument validation failed prior to or during `argv` construction.
    #[error(transparent)]
    Validation(#[from] ToolValidationError),
}

/// Errors raised when parsing structured execution metrics from tool output.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum MetricParseError {
    /// Tool output contained invalid UTF-8 sequences.
    #[error("tool output is not valid UTF-8")]
    InvalidUtf8,

    /// Tool output did not match the expected structure.
    #[error("malformed tool output: {0}")]
    MalformedOutput(String),

    /// JSON diagnostic parsing failed.
    #[error("failed to parse JSON output: {0}")]
    Json(String),
}

/// Top-level error type encompassing validation, argv building, and metric parsing.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ToolError {
    /// Invocation or argument validation failed.
    #[error(transparent)]
    Validation(#[from] ToolValidationError),

    /// Deterministic `argv` construction failed.
    #[error(transparent)]
    Build(#[from] ToolBuildError),

    /// Output metric parsing failed.
    #[error(transparent)]
    MetricParse(#[from] MetricParseError),
}
