#[cfg(test)]
pub mod config_for_tests {
    use akri_shared::{
        akri::{
            configuration::Configuration,
            instance::{Instance, InstanceList, InstanceSpec},
        },
        k8s::MockKubeInterface,
        os::file,
    };
    use k8s_openapi::api::core::v1::{Pod, Service};
    use kube::api::ObjectList;
    use log::trace;

    pub type PodList = ObjectList<Pod>;
    pub type ServiceList = ObjectList<Service>;

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
                    // Return error that instance could not be found
                    Err(anyhow::anyhow!(kube::Error::Api(
                        kube::error::ErrorResponse {
                            status: "Failure".to_string(),
                            message: "instances.akri.sh \"akri-blah-901a7b\" not found".to_string(),
                            reason: "NotFound".to_string(),
                            code: akri_shared::k8s::ERROR_NOT_FOUND,
                        }
                    )))
                } else {
                    let instance_json = file::read_file_to_string(result_file);
                    let instance: Instance = serde_json::from_str(&instance_json).unwrap();
                    Ok(instance)
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
    fn listify_kube_object(node_json: &str) -> String {
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
            let list: InstanceList = serde_json::from_str(&instance_list_json).unwrap();
            Ok(list)
        });
    }

    pub fn configure_update_instance(
        mock: &mut MockKubeInterface,
        instance_to_update: InstanceSpec,
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
                    Err(None.ok_or_else(|| anyhow::anyhow!("failure"))?)
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
                    Err(None.ok_or_else(|| anyhow::anyhow!("failure"))?)
                } else {
                    let config_json = file::read_file_to_string(result_file);
                    let config: Configuration = serde_json::from_str(&config_json).unwrap();
                    Ok(config)
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
                    Err(None.ok_or_else(|| anyhow::anyhow!("failure"))?)
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
                svc_to_create.metadata.name.as_ref().unwrap() == svc_name
                    && svc_to_create
                        .metadata
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
                    Err(None.ok_or_else(|| anyhow::anyhow!("failure"))?)
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
                    Err(None.ok_or_else(|| anyhow::anyhow!("failure"))?)
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
        error: bool,
    ) {
        trace!("mock.expect_create_pod pod_name:{}", pod_name);
        mock.expect_create_pod()
            .times(1)
            .withf(move |pod_to_create, namespace| {
                pod_to_create.metadata.name.as_ref().unwrap() == pod_name
                    && pod_to_create
                        .metadata
                        .labels
                        .as_ref()
                        .unwrap()
                        .get(label_id)
                        .unwrap()
                        == label_value
                    && namespace == pod_namespace
            })
            .returning(move |_, _| match error {
                false => Ok(()),
                true => Err(anyhow::format_err!("create pod error")),
            });
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
