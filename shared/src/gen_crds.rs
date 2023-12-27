use akri_shared::akri::instance;
use kube::CustomResourceExt;

pub fn main() {
    println!(
        "{}",
        serde_yaml::to_string(&instance::Instance::crd()).unwrap()
    );
}
