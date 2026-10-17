//! Atlas Mobile native transport: an iroh P2P transport exposed to Flutter via
//! flutter_rust_bridge 2.x. Run `flutter_rust_bridge_codegen generate` (in
//! `apps/mobile`) after changing the public API in `api.rs`.

pub mod api;
mod frb_generated;
