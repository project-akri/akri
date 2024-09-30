#[cfg(test)]
pub mod config_for_tests {
    use super::mock_client::MockControllerKubeClient;
    use akri_shared::{akri::configuration::Configuration, k8s::api::MockApi, os::file};
    use chrono::DateTime;
    use k8s_openapi::api::core::v1::Pod;
    use kube::{api::ObjectList, ResourceExt};
    use log::trace;

    pub type PodList = ObjectList<Pod>;

    pub fn configure_find_config(
        mock: &mut MockControllerKubeClient,
        config_name: &'static str,
        config_namespace: &'static str,
        result_file: &'static str,
        result_error: bool,
    ) {
        trace!("mock.expect_find_configuration config_name:{}", config_name);
        mock.config
            .expect_namespaced()
            .return_once(move |_| {
                let mut mock_api: MockApi<Configuration> = MockApi::new();
                mock_api
                    .expect_get()
                    .times(1)
                    .withf(move |name| name == config_name)
                    .returning(move |_| {
                        if result_error {
                            Err(kube::Error::Api(kube::error::ErrorResponse {
                                status: "Failure".to_string(),
                                message: format!("configurations.akri.sh {config_name} not found"),
                                reason: "NotFound".to_string(),
                                code: akri_shared::k8s::ERROR_NOT_FOUND,
                            }))
                        } else {
                            let config_json = file::read_file_to_string(result_file);
                            let config: Configuration = serde_json::from_str(&config_json).unwrap();
                            Ok(Some(config))
                        }
                    });
                Box::new(mock_api)
            })
            .withf(move |namespace| namespace == config_namespace);
    }

    pub fn configure_find_pods(
        mock_api: &mut MockApi<Pod>,
        pod_selector: &'static str,
        _namespace: &'static str,
        result_file: &'static str,
        result_error: bool,
    ) {
        trace!(
            "mock.expect_find_pods_with_label pod_selector:{}",
            pod_selector
        );
        mock_api
            .expect_list()
            .times(1)
            .withf(move |lp| lp.label_selector.as_ref().unwrap_or(&String::new()) == pod_selector)
            .returning(move |_| {
                if result_error {
                    Err(kube::Error::Api(kube::error::ErrorResponse {
                        status: "Failure".to_string(),
                        message: format!("pods {pod_selector} not found"),
                        reason: "NotFound".to_string(),
                        code: akri_shared::k8s::ERROR_NOT_FOUND,
                    }))
                } else {
                    let pods_json = file::read_file_to_string(result_file);
                    let pods: PodList = serde_json::from_str(&pods_json).unwrap();
                    Ok(pods)
                }
            });
    }

    pub fn configure_find_pods_with_phase(
        mock_api: &mut MockApi<Pod>,
        pod_selector: &'static str,
        result_file: &'static str,
        specified_phase: &'static str,
    ) {
        trace!(
            "mock.expect_find_pods_with_label pod_selector:{}",
            pod_selector
        );
        mock_api
            .expect_list()
            .times(1)
            .withf(move |lp| lp.label_selector.as_ref().unwrap_or(&String::new()) == pod_selector)
            .returning(move |_| {
                let pods_json = file::read_file_to_string(result_file);
                let phase_adjusted_json = pods_json.replace(
                    "\"phase\": \"Running\"",
                    &format!("\"phase\": \"{}\"", specified_phase),
                );
                let pods: PodList = serde_json::from_str(&phase_adjusted_json).unwrap();
                Ok(pods)
            });
    }

    pub fn configure_find_pods_with_phase_and_start_time(
        mock_api: &mut MockApi<Pod>,
        pod_selector: &'static str,
        result_file: &'static str,
        specified_phase: &'static str,
        start_time: DateTime<chrono::Utc>,
    ) {
        trace!(
            "mock.expect_find_pods_with_label pod_selector:{}",
            pod_selector
        );
        mock_api
            .expect_list()
            .times(1)
            .withf(move |lp| lp.label_selector.as_ref().unwrap_or(&String::new()) == pod_selector)
            .returning(move |_| {
                let pods_json = file::read_file_to_string(result_file);
                let phase_adjusted_json = pods_json.replace(
                    "\"phase\": \"Running\"",
                    &format!("\"phase\": \"{}\"", specified_phase),
                );
                let start_time_adjusted_json = phase_adjusted_json.replace(
                    "\"startTime\": \"2020-02-25T20:48:03Z\"",
                    &format!(
                        "\"startTime\": \"{}\"",
                        start_time.format("%Y-%m-%dT%H:%M:%SZ")
                    ),
                );
                let pods: PodList = serde_json::from_str(&start_time_adjusted_json).unwrap();
                Ok(pods)
            });
    }

    pub fn configure_find_pods_with_phase_and_no_start_time(
        mock_api: &mut MockApi<Pod>,
        pod_selector: &'static str,
        result_file: &'static str,
        specified_phase: &'static str,
    ) {
        trace!(
            "mock.expect_find_pods_with_label pod_selector:{}",
            pod_selector
        );
        mock_api
            .expect_list()
            .times(1)
            .withf(move |lp| lp.label_selector.as_ref().unwrap_or(&String::new()) == pod_selector)
            .returning(move |_| {
                let pods_json = file::read_file_to_string(result_file);
                let phase_adjusted_json = pods_json.replace(
                    "\"phase\": \"Running\"",
                    &format!("\"phase\": \"{}\"", specified_phase),
                );
                let start_time_adjusted_json =
                    phase_adjusted_json.replace("\"startTime\": \"2020-02-25T20:48:03Z\",", "");
                let pods: PodList = serde_json::from_str(&start_time_adjusted_json).unwrap();
                Ok(pods)
            });
    }

    pub fn configure_add_pod(
        mock_api: &mut MockApi<Pod>,
        pod_name: &'static str,
        _pod_namespace: &'static str,
        label_id: &'static str,
        label_value: &'static str,
        error: bool,
    ) {
        trace!("mock.expect_create_pod pod_name:{}", pod_name);
        mock_api
            .expect_apply()
            .withf(move |pod_to_create, _| {
                pod_to_create.name_unchecked() == pod_name
                    && pod_to_create.labels().get(label_id).unwrap() == label_value
            })
            .returning(move |pod, _| match error {
                false => Ok(pod),
                true => Err(kube::Error::Api(kube::error::ErrorResponse {
                    status: "Failure".to_string(),
                    message: format!("pods {pod_name} not created"),
                    reason: "NotFound".to_string(),
                    code: akri_shared::k8s::ERROR_NOT_FOUND,
                })),
            });
    }

    pub fn configure_remove_pod(
        mock_api: &mut MockApi<Pod>,
        pod_name: &'static str,
        pod_namespace: &'static str,
    ) {
        trace!(
            "mock.expect_remove_pod pod_name:{} pod_namespace:{}",
            pod_name,
            pod_namespace
        );
        mock_api
            .expect_delete()
            .times(1)
            .withf(move |pod_to_remove| pod_to_remove == pod_name)
            .returning(move |_| Ok(either::Left(Pod::default())));
    }
}

#[cfg(test)]
pub mod mock_client {
    use akri_shared::akri::{configuration::Configuration, instance::Instance};
    use akri_shared::k8s::api::{Api, IntoApi, MockIntoApi};
    use k8s_openapi::api::batch::v1::Job;
    use k8s_openapi::api::core::v1::{Node, Pod, Service};

    #[derive(Default)]
    pub struct MockControllerKubeClient {
        pub instance: MockIntoApi<Instance>,
        pub config: MockIntoApi<Configuration>,
        pub job: MockIntoApi<Job>,
        pub pod: MockIntoApi<Pod>,
        pub service: MockIntoApi<Service>,
        pub node: MockIntoApi<Node>,
    }

    impl IntoApi<Instance> for MockControllerKubeClient {
        fn all(&self) -> Box<dyn Api<Instance>> {
            self.instance.all()
        }

        fn namespaced(&self, namespace: &str) -> Box<dyn Api<Instance>> {
            self.instance.namespaced(namespace)
        }

        fn default_namespaced(&self) -> Box<dyn Api<Instance>> {
            self.instance.default_namespaced()
        }
    }

    impl IntoApi<Configuration> for MockControllerKubeClient {
        fn all(&self) -> Box<dyn Api<Configuration>> {
            self.config.all()
        }

        fn namespaced(&self, namespace: &str) -> Box<dyn Api<Configuration>> {
            self.config.namespaced(namespace)
        }

        fn default_namespaced(&self) -> Box<dyn Api<Configuration>> {
            self.config.default_namespaced()
        }
    }

    impl IntoApi<Job> for MockControllerKubeClient {
        fn all(&self) -> Box<dyn Api<Job>> {
            self.job.all()
        }

        fn namespaced(&self, namespace: &str) -> Box<dyn Api<Job>> {
            self.job.namespaced(namespace)
        }

        fn default_namespaced(&self) -> Box<dyn Api<Job>> {
            self.job.default_namespaced()
        }
    }

    impl IntoApi<Pod> for MockControllerKubeClient {
        fn all(&self) -> Box<dyn Api<Pod>> {
            self.pod.all()
        }

        fn namespaced(&self, namespace: &str) -> Box<dyn Api<Pod>> {
            self.pod.namespaced(namespace)
        }

        fn default_namespaced(&self) -> Box<dyn Api<Pod>> {
            self.pod.default_namespaced()
        }
    }

    impl IntoApi<Service> for MockControllerKubeClient {
        fn all(&self) -> Box<dyn Api<Service>> {
            self.service.all()
        }

        fn namespaced(&self, namespace: &str) -> Box<dyn Api<Service>> {
            self.service.namespaced(namespace)
        }

        fn default_namespaced(&self) -> Box<dyn Api<Service>> {
            self.service.default_namespaced()
        }
    }

    impl IntoApi<Node> for MockControllerKubeClient {
        fn all(&self) -> Box<dyn Api<Node>> {
            self.node.all()
        }

        fn namespaced(&self, _namespace: &str) -> Box<dyn Api<Node>> {
            // TODO: handle error here -- no namespaced scope for Node
            self.node.all()
        }

        fn default_namespaced(&self) -> Box<dyn Api<Node>> {
            // TODO: handle error here -- no namespaced scope for Node
            self.node.all()
        }
    }
}
