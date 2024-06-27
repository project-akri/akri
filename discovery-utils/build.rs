fn main() {
    tonic_build::configure()
        .out_dir("./src/discovery")
        .protoc_arg("--experimental_allow_proto3_optional")
        .compile(&["proto/discovery.proto"], &["proto"])
        .expect("failed to compile protos");
}
