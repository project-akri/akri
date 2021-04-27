# Prometheus Monitoring with Akri
To enable a deeper understanding of the state of an Akri deployment and Node resource usage by Akri containers, Akri
exposes metrics with Prometheus. This document will cover:
* Installing Prometheus
* Enabling Prometheus with Akri
* Visualizing metrics with Grafana
* Akri's currently exposed metrics
* Exposing metrics from an Akri Broker Pod

## Installing Prometheus
In order to expose Akri's metrics, Prometheus must be deployed to your cluster. If you already have Prometheus running
on your cluster, you can skip this step. 

Prometheus is comprised of many components. Instead of manually deploying all the components, the entire kube-prometheus
stack can be deployed via its [Helm
chart](https://github.com/prometheus-community/helm-charts/tree/main/charts/kube-prometheus-stack). It includes the
Prometheus operator, node exporter, built in Grafana support, and more. 
1. Get the kube-prometheus stack Helm repo.
    ```sh
    helm repo add prometheus-community https://prometheus-community.github.io/helm-charts
    helm repo add stable https://charts.helm.sh/stable
    helm repo update
    ```
1. Install the chart, specifying what namespace you want Prometheus to run in. It does not have to be the same namespace
   in which you are running Akri. For example, it may be in a namespace called `monitoring` as in the command below. [By
   default](https://github.com/prometheus-community/helm-charts/tree/main/charts/kube-prometheus-stack#prometheusioscrape),
   Prometheus only discovers PodMonitors within its namespace. This should be disabled by setting
   `podMonitorSelectorNilUsesHelmValues` to `false` so that Akri's custom PodMonitors can be discovered. Additionally,
   the Grafana service can be exposed to the host by making it a NodePort service. It may take a minute or so to deploy
   all the components.
    ```sh
    helm install prometheus prometheus-community/kube-prometheus-stack \
       --set grafana.service.type=NodePort \
       --set prometheus.prometheusSpec.podMonitorSelectorNilUsesHelmValues=false \
       --namespace monitoring
    ```
    > **Note**: The Prometheus dashboard can also be exposed to the host by adding `--set
    > prometheus.service.type=NodePort`. **Note**: If intending to [expose metrics from a Broker
    > Pod](#exposing-metrics-from-an-akri-broker-pod) via a ServiceMonitor also set
    > `serviceMonitorSelectorNilUsesHelmValues` to `false`.

## Enabling Prometheus in Akri
The Akri Controller and Agent publish metrics to port 8080 at a `/metrics` endpoint. However, these cannot be accessed
by Prometheus without creating PodMonitors, which are custom resources that tell Prometheus which Pods to monitor. These
components can all be automatically created and deployed via Helm by setting `--set prometheus.enabled=true` when
installing Akri. 

Install Akri and expose the Controller and Agent's metrics to Prometheus by running:
```sh
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set prometheus.enabled=true
```
> **Note**: This documentation assumes you are using vanilla Kubernetes. Be sure to reference the [user
> guide](./user-guide.md) to determine whether the distribution you are using requires crictl path configuration.

## Visualizing metrics with Grafana
Now that Akri's metrics are being exposed to Prometheus, they can be visualized in Grafana. 
1. Determine the port that the Grafana Service is running on, specifying the namespace if necessary, and save it for the
   next step.
    ```sh
    kubectl get service/prometheus-grafana  --namespace=monitoring --output=jsonpath='{.spec.ports[?(@.name=="service")].nodePort}' && echo
    ```
1. SSH port forwarding can be used to access Grafana. Open a new terminal, and enter your ssh command to access the machine
   running Akri and Prometheus followed by the port forwarding request. The following command will use port 50000 on the
   host. Feel free to change it if it is not available. Be sure to replace `<Grafana Service port>` with the port number
   outputted in the previous step.
    ```sh
    ssh someuser@<IP address> -L 50000:localhost:<Grafana Service port>
    ```
1. Navigate to `http://localhost:50000/` and enter Grafana's default username `admin` and password `prom-operator`. 
   Once logged in, the username and password can be changed in account settings. Now, 
   you can create a Dashboard to display the Akri metrics. 

## Akri's currently exposed metrics
Akri uses the [Rust Prometheus client library](https://github.com/tikv/rust-prometheus) to expose metrics. It exposes
all the [default process metrics](https://prometheus.io/docs/instrumenting/writing_clientlibs/#process-metrics), such as
Agent or Controller total CPU time usage (`process_cpu_seconds_total`) and RAM usage (`process_resident_memory_bytes`),
along with the following custom metrics, all of which are prefixed with `akri`. 
| Metric Name | Metric Type | Metric Source | Buckets |
|---|---|---|---|
| akri_instance_count | IntGaugeVec | Agent | Configuration, shared | 
| akri_broker_pod_count | IntGaugeVec | Controller | Configuration, Node |

## Exposing metrics from an Akri Broker Pod
Metrics can also be published by Broker Pods and exposed to Prometheus. This workflow is not unique to Akri and is
equivalent to exposing metrics from any deployment to Prometheus. Using the [appropriate Prometheus client
library](https://prometheus.io/docs/instrumenting/clientlibs/) for your broker, expose some metrics. Then, deploy a
Service to expose the metrics, specifying the name of the associated Akri Configuration as a selector
(`akri.sh/configuration: <Akri Configuration>`), since the Configuration name is added as a label to all the Broker Pods
by the Akri Controller. Finally, deploy a ServiceMonitor that selects for the previously mentioned service. This tells
Prometheus which service(s) to discover.

### Example: Exposing metrics from the udev video sample Broker
As an example, an `akri_frame_count` metric has been created in the sample
[udev-video-broker](../samples/brokers/udev-video-broker). Like the Agent and Controller, it publishes both the default
process metrics and the custom `akri_frame_count` metric to port 8080 at a `/metrics` endpoint. 

1. Akri can be installed with the udev Configuration, filtering for only usb video cameras and specifying a
   Configuration name of `akri-udev-video`, by running:
    ```sh
    helm repo add akri-helm-charts https://deislabs.github.io/akri/
    helm install akri akri-helm-charts/akri \
        --set udev.enabled=true \
        --set udev.name=akri-udev-video \
        --set udev.udevRules[0]='KERNEL=="video[0-9]*"' \
        --set udev.brokerPod.image.repository="ghcr.io/deislabs/akri/udev-video-broker"
    ```
    > **Note**: This instruction assumes you are using vanilla Kubernetes. Be sure to reference the [user
    > guide](./user-guide.md) to determine whether the distribution you are using requires crictl path configuration.


    > **Note**: Also, expose the Agent and Controller's Prometheus metrics by adding `--set prometheus.enabled=true`.


    > **Note**: If Prometheus is running in a different namespace as Akri and was not enabled to discover
    > ServiceMonitors in other namespaces when installed, upgrade your Prometheus Helm installation to set
    > `prometheus.prometheusSpec.serviceMonitorSelectorNilUsesHelmValues` to `false`.
    > ```sh
    > helm upgrade prometheus prometheus-community/kube-prometheus-stack \
    >   --set grafana.service.type=NodePort \
    >   --set prometheus.prometheusSpec.podMonitorSelectorNilUsesHelmValues=false \
    >   --set prometheus.prometheusSpec.serviceMonitorSelectorNilUsesHelmValues=false \
    >   --namespace monitoring 
    > ```
1. Then, create a Service for exposing these metrics, targeting all Pods labeled with the Configuration name
   `akri-udev-video`. 
    ```yaml
    apiVersion: v1
    kind: Service
    metadata:
    name: akri-udev-video-broker-metrics
    labels:
        app: akri-udev-video-broker-metrics
    spec:
    selector:
        akri.sh/configuration: akri-udev-video
    ports:
    - name: metrics
      port: 8080
    type: ClusterIP
    ```
    > **Note**: The metrics also could have been exposed by adding the metrics port to the Configuration level service
    > in the udev Configuration. 
1. Apply the Service to your cluster.
    ```sh
    kubectl apply -f akri-udev-video-broker-metrics-service.yaml
    ```
1. Create the associated ServiceMonitor. Note how the selector matches the app name of the Service.
    ```yaml
    apiVersion: monitoring.coreos.com/v1
    kind: ServiceMonitor
    metadata:
    name: akri-udev-video-broker-metrics
    labels:
        release: prometheus
    spec:
    selector:
        matchLabels:
        app: akri-udev-video-broker-metrics
    endpoints:
    - port: metrics
    ```
1. Apply the ServiceMonitor to your cluster.
    ```sh
    kubectl apply -f akri-udev-video-broker-metrics-service-monitor.yaml
    ```
1. The frame count metric reports the number of video frames that have been requested by some application. It will
   remain at zero unless an application is deployed that utilizes the video Brokers. Deploy the Akri sample streaming
   application by running the following:
    ```sh
    kubectl apply -f https://raw.githubusercontent.com/deislabs/akri/main/deployment/samples/akri-video-streaming-app.yaml
    watch kubectl get pods
    ```