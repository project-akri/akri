/// This generates Device Plugin code (in v1beta1.rs) from pluginapi.proto
fn main() {
    tonic_build::configure()
        .build_client(true)
        .out_dir("./src/util")
        .compile(&["./proto/pluginapi.proto"], &["./proto"])
        .expect("failed to compile protos");
}
