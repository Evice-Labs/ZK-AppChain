// crates/solver-service/build.rs

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_prost_build::configure()
        .build_server(false) // Kita hanya butuh Client, bukan Server
        .build_client(true)
        .compile_protos(&["../../proto/service.proto"], &["../../proto"])?;
    Ok(())
}
