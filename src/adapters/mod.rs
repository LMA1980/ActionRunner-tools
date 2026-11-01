//! Built-in compile-time [`crate::adapter::ToolAdapter`] implementations.

use std::sync::Arc;

use crate::registry::ToolRegistry;

pub mod cargo;
pub(crate) mod common;
pub mod mdformat;
pub mod trunk;

pub use cargo::{
    CargoAuditAdapter, CargoBenchAdapter, CargoBuildAdapter, CargoClippyAdapter, CargoFmtAdapter,
    CargoLlvmCovAdapter, CargoMiriAdapter, CargoPublishAdapter, CargoTestAdapter,
};
pub use mdformat::MdformatAdapter;
pub use trunk::TrunkAdapter;

/// Register all 11 built-in CI/CD tool adapters into `registry`.
pub fn register_builtin_adapters(registry: &mut ToolRegistry) {
    registry.register(Arc::new(CargoFmtAdapter));
    registry.register(Arc::new(CargoClippyAdapter));
    registry.register(Arc::new(CargoTestAdapter));
    registry.register(Arc::new(CargoBenchAdapter));
    registry.register(Arc::new(CargoAuditAdapter));
    registry.register(Arc::new(CargoLlvmCovAdapter));
    registry.register(Arc::new(CargoMiriAdapter));
    registry.register(Arc::new(CargoPublishAdapter));
    registry.register(Arc::new(CargoBuildAdapter));
    registry.register(Arc::new(TrunkAdapter));
    registry.register(Arc::new(MdformatAdapter));
}
