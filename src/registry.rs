//! Compile-time registry and lookup dispatch engine for [`ToolAdapter`] implementations.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::sync::Arc;

use crate::adapter::{SandboxProfileRequirement, ToolAdapter, ToolInvocation};
use crate::error::{ToolError, ToolValidationError};
use crate::sanitize::validate_invocation_base;

/// Deterministic registry mapping canonical tool identifiers to [`ToolAdapter`] instances.
#[derive(Clone, Default)]
pub struct ToolRegistry {
    adapters: BTreeMap<&'static str, Arc<dyn ToolAdapter>>,
}

impl std::fmt::Debug for ToolRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolRegistry")
            .field("registered_tools", &self.tool_names())
            .finish()
    }
}

impl ToolRegistry {
    /// Create an empty [`ToolRegistry`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            adapters: BTreeMap::new(),
        }
    }

    /// Create a [`ToolRegistry`] pre-populated with all built-in CI/CD tool adapters.
    #[must_use]
    pub fn with_builtin_adapters() -> Self {
        let mut registry = Self::new();
        crate::adapters::register_builtin_adapters(&mut registry);
        registry
    }

    /// Register a [`ToolAdapter`] keyed by its canonical [`ToolAdapter::name`].
    pub fn register(&mut self, adapter: Arc<dyn ToolAdapter>) {
        self.adapters.insert(adapter.name(), adapter);
    }

    /// Look up a registered [`ToolAdapter`] by its canonical name.
    ///
    /// # Errors
    ///
    /// Returns [`ToolValidationError::UnknownTool`] if `name` is not registered.
    pub fn get(&self, name: &str) -> Result<&dyn ToolAdapter, ToolValidationError> {
        self.adapters
            .get(name)
            .map(Arc::as_ref)
            .ok_or_else(|| ToolValidationError::UnknownTool(name.to_string()))
    }

    /// Return the sorted list of all registered canonical tool names.
    #[must_use]
    pub fn tool_names(&self) -> Vec<&'static str> {
        self.adapters.keys().copied().collect()
    }

    /// Validate `inv` against base security invariants and the target adapter's allowlist,
    /// then construct its deterministic `argv` and [`SandboxProfileRequirement`].
    ///
    /// # Errors
    ///
    /// Returns [`ToolError`] if base sanitization fails, the tool name is unknown,
    /// adapter-level flag validation fails, or `argv` construction fails.
    pub fn validate_and_build(
        &self,
        inv: &ToolInvocation,
    ) -> Result<(Vec<OsString>, SandboxProfileRequirement), ToolError> {
        validate_invocation_base(inv)?;
        let adapter = self.get(&inv.tool_name)?;
        adapter.validate(inv)?;
        let argv = adapter.build_argv(inv)?;
        let profile = adapter.sandbox_profile(inv);
        Ok((argv, profile))
    }
}
