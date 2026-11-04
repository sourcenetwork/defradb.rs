fn main() -> Result<(), Box<dyn std::error::Error>> {
    // No `connect` helper: it would name tonic's tokio transport; the client
    // is built on `net::grpc`.
    tonic_prost_build::configure()
        .build_transport(false)
        .compile_protos(&["proto/orbis.proto"], &["proto"])?;
    Ok(())
}
