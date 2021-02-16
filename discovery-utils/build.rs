fn main() {
    tonic_build::configure()
        .out_dir("./src/discovery")
        .compile(&["proto/discovery.proto"], &["proto"])
        .expect("failed to compile protos");
}
