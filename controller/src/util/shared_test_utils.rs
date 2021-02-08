#[cfg(test)]
pub mod config_for_tests {
    use akri_shared::{
        akri::{
            configuration::KubeAkriConfig,
            instance::{Instance, KubeAkriInstance, KubeAkriInstanceList},
        },
        k8s::MockKubeInterface,
        os::file,
    };
    use k8s_openapi::api::core::v1::{PodSpec, PodStatus, ServiceSpec, ServiceStatus};
    use kube::api::{Object, ObjectList};
    use log::trace;

    pub type PodObject = Object<PodSpec, PodStatus>;
    pub type PodList = ObjectList<PodObject>;
    pub type ServiceObject = Object<ServiceSpec, ServiceStatus>;
    pub type ServiceList = ObjectList<ServiceObject>;

    pub fn configure_find_instance(
        mock: &mut MockKubeInterface,
        instance_name: &'static str,
        instance_namespace: &'static str,
        result_file: &'static str,
        result_error: bool,
    ) {
        trace!("mock.expect_find_instance instance_name:{}", instance_name);
        mock.expect_find_instance()
            .times(1)
            .withf(move |name, namespace| name == instance_name && namespace == instance_namespace)
            .returning(move |_, _| {
                if result_error {
                    Err(None.ok_or("failure")?)
                } else {
                    let dci_json = file::read_file_to_string(result_file);
                    let dci: KubeAkriInstance = serde_json::from_str(&dci_json).unwrap();
                    Ok(dci)
                }
            });
    }

    const LIST_PREFIX: &str = r#"
{
    "apiVersion": "v1",
    "items": ["#;
    const LIST_SUFFIX: &str = r#"
    ],
    "kind": "List",
    "metadata": {
        "resourceVersion": "",
        "selfLink": ""
    }
}"#;
    fn listify_kube_object(node_json: &String) -> String {
        format!("{}\n{}\n{}", LIST_PREFIX, node_json, LIST_SUFFIX)
    }

    pub fn configure_get_instances(
        mock: &mut MockKubeInterface,
        result_file: &'static str,
        listify_result: bool,
    ) {
        trace!("mock.expect_get_instances namespace:None");
        mock.expect_get_instances().times(1).returning(move || {
            let json = file::read_file_to_string(result_file);
            let instance_list_json = if listify_result {
                listify_kube_object(&json)
            } else {
                json
            };
            let list: KubeAkriInstanceList = serde_json::from_str(&instance_list_json).unwrap();
            Ok(list)
        });
    }

    pub fn configure_update_instance(
        mock: &mut MockKubeInterface,
        instance_to_update: Instance,
        instance_name: &'static str,
        instance_namespace: &'static str,
        result_error: bool,
    ) {
        trace!(
            "mock.expect_update_instance name:{} namespace:{} error:{}",
            instance_name,
            instance_namespace,
            result_error
        );
        mock.expect_update_instance()
            .times(1)
            .withf(move |instance, name, namespace| {
                name == instance_name
                    && namespace == instance_namespace
                    && instance.nodes == instance_to_update.nodes
                    && instance.device_usage == instance_to_update.device_usage
            })
            .returning(move |_, _, _| {
                if result_error {
                    Err(None.ok_or("failure")?)
                } else {
                    Ok(())
                }
            });
    }

    pub fn configure_find_config(
        mock: &mut MockKubeInterface,
        config_name: &'static str,
        config_namespace: &'static str,
        result_file: &'static str,
        result_error: bool,
    ) {
        trace!("mock.expect_find_configuration config_name:{}", config_name);
        mock.expect_find_configuration()
            .times(1)
            .withf(move |name, namespace| name == config_name && namespace == config_namespace)
            .returning(move |_, _| {
                if result_error {
                    Err(None.ok_or("failure")?)
                } else {
                    let dcc_json = file::read_file_to_string(result_file);
                    let dcc: KubeAkriConfig = serde_json::from_str(&dcc_json).unwrap();
                    Ok(dcc)
                }
            });
    }

    pub fn configure_find_services(
        mock: &mut MockKubeInterface,
        svc_selector: &'static str,
        result_file: &'static str,
        result_error: bool,
    ) {
        trace!("mock.expect_find_services svc_selector:{}", svc_selector);
        mock.expect_find_services()
            .times(1)
            .withf(move |selector| selector == svc_selector)
            .returning(move |_| {
                if result_error {
                    Err(None.ok_or("failure")?)
                } else {
                    let svcs_json = file::read_file_to_string(result_file);
                    let svcs: ServiceList = serde_json::from_str(&svcs_json).unwrap();
                    Ok(svcs)
                }
            });
    }
    pub fn configure_add_service(
        mock: &mut MockKubeInterface,
        svc_name: &'static str,
        namespace: &'static str,
        label_id: &'static str,
        label_value: &'static str,
    ) {
        trace!(
            "mock.expect_create_service name:{}, namespace:{}, [{}={}]",
            &svc_name,
            &namespace,
            &label_id,
            &label_value
        );
        mock.expect_create_service()
            .withf(move |svc_to_create, ns| {
                svc_to_create
                    .metadata
                    .as_ref()
                    .unwrap()
                    .name
                    .as_ref()
                    .unwrap()
                    == svc_name
                    && svc_to_create
                        .metadata
                        .as_ref()
                        .unwrap()
                        .labels
                        .as_ref()
                        .unwrap()
                        .get(label_id)
                        .unwrap()
                        == label_value
                    && ns == namespace
            })
            .returning(move |_, _| Ok(()));
    }

    pub fn configure_remove_service(
        mock: &mut MockKubeInterface,
        svc_name: &'static str,
        svc_namespace: &'static str,
    ) {
        trace!(
            "mock.expect_remove_service svc_name:{}, svc_namespace={}",
            svc_name,
            svc_namespace
        );
        mock.expect_remove_service()
            .times(1)
            .withf(move |svc_to_remove, namespace| {
                svc_to_remove == svc_name && namespace == svc_namespace
            })
            .returning(move |_, _| Ok(()));
    }

    pub fn configure_update_service(
        mock: &mut MockKubeInterface,
        svc_name: &'static str,
        svc_namespace: &'static str,
        result_error: bool,
    ) {
        trace!(
            "mock.expect_update_service name:{} namespace:{} error:{}",
            svc_name,
            svc_namespace,
            result_error,
        );
        mock.expect_update_service()
            .times(1)
            .withf(move |_svc, name, namespace| name == svc_name && namespace == svc_namespace)
            .returning(move |_, _, _| {
                if result_error {
                    Err(None.ok_or("failure")?)
                } else {
                    Ok(())
                }
            });
    }

    pub fn configure_find_pods(
        mock: &mut MockKubeInterface,
        pod_selector: &'static str,
        result_file: &'static str,
        result_error: bool,
    ) {
        trace!(
            "mock.expect_find_pods_with_label pod_selector:{}",
            pod_selector
        );
        mock.expect_find_pods_with_label()
            .times(1)
            .withf(move |selector| selector == pod_selector)
            .returning(move |_| {
                if result_error {
                    Err(None.ok_or("failure")?)
                } else {
                    let pods_json = file::read_file_to_string(result_file);
                    let pods: PodList = serde_json::from_str(&pods_json).unwrap();
                    Ok(pods)
                }
            });
    }

    pub fn configure_add_pod(
        mock: &mut MockKubeInterface,
        pod_name: &'static str,
        pod_namespace: &'static str,
        label_id: &'static str,
        label_value: &'static str,
    ) {
        trace!("mock.expect_create_pod pod_name:{}", pod_name);
        mock.expect_create_pod()
            .times(1)
            .withf(move |pod_to_create, namespace| {
                pod_to_create
                    .metadata
                    .as_ref()
                    .unwrap()
                    .name
                    .as_ref()
                    .unwrap()
                    == pod_name
                    && pod_to_create
                        .metadata
                        .as_ref()
                        .unwrap()
                        .labels
                        .as_ref()
                        .unwrap()
                        .get(label_id)
                        .unwrap()
                        == label_value
                    && namespace == pod_namespace
            })
            .returning(move |_, _| Ok(()));
    }

    pub fn configure_remove_pod(
        mock: &mut MockKubeInterface,
        pod_name: &'static str,
        pod_namespace: &'static str,
    ) {
        trace!(
            "mock.expect_remove_pod pod_name:{} pod_namespace:{}",
            pod_name,
            pod_namespace
        );
        mock.expect_remove_pod()
            .times(1)
            .withf(move |pod_to_remove, namespace| {
                pod_to_remove == pod_name && namespace == pod_namespace
            })
            .returning(move |_, _| Ok(()));
    }
}
