//! Build script for tasker-shared crate
//!
//! Compiles Protocol Buffer definitions for gRPC services when the `grpc-api` feature is enabled.
//! Generated code is output to `$OUT_DIR/tasker.v1.rs` and included via `include!` macro in
//! `src/proto/mod.rs`.
//!
//! Proto files are located in `../proto/tasker/v1/` relative to this crate.
//!
//! # Protocol Buffer Compiler
//!
//! This build script requires the `protoc` compiler to be installed on the system.
//! On macOS, install via: `brew install protobuf`
//!
//! If `protoc` is not found, the build will fail with an error message explaining
//! how to install it.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Only compile protos when grpc-api feature is enabled
    #[cfg(feature = "grpc-api")]
    {
        use std::path::PathBuf;

        // Find the workspace root (where proto/ directory lives)
        let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
        let workspace_root = manifest_dir.parent().expect("tasker-shared must be in workspace");
        let proto_root = workspace_root.join("proto");

        // Verify proto directory exists
        if !proto_root.exists() {
            panic!(
                "Proto directory not found at {:?}. Expected proto files at proto/tasker/v1/",
                proto_root
            );
        }

        // List of proto files to compile
        let proto_files = [
            "tasker/v1/common.proto",
            "tasker/v1/tasks.proto",
            "tasker/v1/steps.proto",
            "tasker/v1/templates.proto",
            "tasker/v1/analytics.proto",
            "tasker/v1/dlq.proto",
            "tasker/v1/health.proto",
            "tasker/v1/config.proto",
        ];

        // Convert to full paths and verify each exists
        let proto_paths: Vec<PathBuf> = proto_files
            .iter()
            .map(|f| {
                let path = proto_root.join(f);
                if !path.exists() {
                    panic!("Proto file not found: {:?}", path);
                }
                path
            })
            .collect();

        // Configure tonic-build
        tonic_build::configure()
            // Generate server code
            .build_server(true)
            // Generate client code
            .build_client(true)
            // Generate transport implementations
            .build_transport(true)
            // Include file descriptor set for reflection
            .file_descriptor_set_path(
                PathBuf::from(std::env::var("OUT_DIR")?)
                    .join("tasker_descriptor.bin"),
            )
            // Emit rerun-if-changed directives
            .emit_rerun_if_changed(true)
            // Compile with proto_path set to proto root
            .compile_protos(&proto_paths, &[&proto_root])?;

        // Emit rerun-if-changed for the proto directory
        println!("cargo:rerun-if-changed={}", proto_root.display());
        for proto in &proto_files {
            println!(
                "cargo:rerun-if-changed={}",
                proto_root.join(proto).display()
            );
        }
    }

    Ok(())
}
