fn main() {
    tonic_build::configure()
        .build_client(true)
        .out_dir("./src/util")
        .compile(&["./proto/camera.proto"], &["./proto"])
        .expect("failed to compile protos");
}
