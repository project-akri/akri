/// This generates Device Plugin code (in v1beta1.rs) from pluginapi.proto
fn main() {
    tonic_build::configure()
        .build_client(true)
        .out_dir("./src/plugin_manager")
        .compile_protos(
            &["./proto/pluginapi.proto", "./proto/podresources.proto"],
            &["./proto"],
        )
        .expect("failed to compile protos");
}
