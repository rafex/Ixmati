fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=../../proto");
    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(
            &[
                "../../proto/ixmati/v1/common.proto",
                "../../proto/ixmati/v1/write.proto",
                "../../proto/ixmati/v1/read.proto",
            ],
            &["../../proto"],
        )?;
    Ok(())
}
