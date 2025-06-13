fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::compile_protos("proto/raft.proto")?;
    println!("cargo:rerun-if-changed=proto/raft.proto");
    Ok(())
} 