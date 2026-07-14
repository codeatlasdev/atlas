#![allow(unused)]

pub mod adapter;
pub mod loader;
pub mod manifest;

pub use adapter::PluginAgentAdapter;
pub use loader::PluginLoader;
pub use manifest::{
    ActivityDetectionConfig, LaunchTemplateConfig, PermissionsConfig, PluginManifest,
};
