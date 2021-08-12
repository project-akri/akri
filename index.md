[Akri](https://github.com/deislabs/akri) lets you easily expose heterogeneous leaf devices (such as IP cameras and USB devices) as resources in a Kubernetes cluster, while also supporting the exposure of embedded hardware resources such as GPUs and FPGAs. Akri continually detects nodes that have access to these devices and schedules workloads based on them.

Simply put: you name it, Akri finds it, you use it.

# Install Akri using Helm

Helm documentation can be found [here](https://github.com/kubernetes/helm).  Once Helm has been installed, Akri can be installed in 2 steps.

First, add Akri Helm charts:
```sh
helm repo add akri-helm-charts https://deislabs.github.io/akri/
```

Then install Akri in your cluster:

Akri can be installed by simply accepting the Helm chart defaults.  This will install and start the Akri controller and agents:

```sh
helm install akri akri-helm-charts/akri
```

Alternatively, Akri can be configured using specific parameters (find the available parameters in [values.yaml](https://github.com/deislabs/akri/blob/main/deployment/helm/values.yaml)):

```
helm install akri akri-helm-charts/akri --set 'param=value' ...
```

For more information on installing Akri, see Akri's [user guide](https://docs.akri.sh/user-guide/getting-started).
