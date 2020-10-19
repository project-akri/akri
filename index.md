[Akri](https://github.com/deislabs/akri) lets you easily expose heterogeneous leaf devices (such as IP cameras and USB devices) as resources in a Kubernetes cluster, while also supporting the exposure of embedded hardware resources such as GPUs and FPGAs. Akri continually detects nodes that have access to these devices and schedules workloads based on them.

Simply put: you name it, Akri finds it, you use it.

# Installation

To install Akri, you need to use [Helm](https://github.com/kubernetes/helm).

The whole process is as simple as these two steps:

1. Add loghouse charts:
```sh
helm repo add akri-helm-charts https://deislabs.github.io/akri/
```

2. Install a chart.

2.1. Easy way:

```sh
helm install akri akri-helm-charts/akri
```

2.2. Using specific parameters *(check variables in chart's [values.yaml](https://github.com/deislabs/akri/blob/main/deployment/helm/values.yaml))*:

```
helm install akri akri-helm-charts/akri --set 'param=value' ...
```

More details for using parameters can be found in the [documentation about modifying an Akri installation](https://github.com/deislabs/akri/blob/main/docs/modifying-akri-installation.md).
