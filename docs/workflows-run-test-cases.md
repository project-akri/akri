# Test K3s, Kubernetes (Kubeadm) and MicroK8s

File: `/.github/workflows/run-test-cases.yml`

A GitHub workflow that:

+ runs Python-based end-to-end [tests](#Tests);
+ through 5 different Kubernetes [versions](#Versions): 1.16, 1.17, 1.18, 1.19, 1.20;
+ on 3 different Kubernetes distros: [K3s](https://k3s.io), [Kubernetes (Kubeadm)](https://kubernetes.io/docs/reference/setup-tools/kubeadm/), [MicroK8s](https://microk8s.io).

## Tests

|Name|File|Documentation|
|----|----|-----------|
|end-to-end|`/test/run-end-to-end.py`|TBD|

## Versions

Distro K3s Version 1.16 creates [Device Plugins](https://kubernetes.io/docs/concepts/extend-kubernetes/compute-storage-net/device-plugins/) sockets at `/var/lib/rancher/k3s/agent/kubelet/device-plugins` whereas Kubernetes expects these sockets to be created at `/var/lib/kubelet/device-plugins`.

See K3s issue: [Compatibility with Device Plugins #1390](https://github.com/k3s-io/k3s/issues/1390)

The fix for K3s 1.16 is to create a symbolic link from the K3s location to the Kubernetes-expected location. This is added as an exception to the workflow step for K3s:

```bash
if [ "${{ matrix.kube.runtime }}" == "K3s-1.16" ]; then
  mkdir -p /var/lib/kubelet
  if [ -d /var/lib/kubelet/device-plugins ]; then
    sudo rm -rf /var/lib/kubelet/device-plugins
  fi
  sudo ln -s /var/lib/rancher/k3s/agent/kubelet/device-plugins /var/lib/kubelet/device-plugins
fi
```

This issue was addressed in K3s version 1.17.

## Jobs|Steps

The workflow comprises two jobs (`build-containers` and `test-cases`).

## `build-containers`

`build-containers` builds container images for Akri 'controller' and 'agent' based upon the commit that triggers the workflow. Once build, these iamges are shared across the `test-cases` job, using GitHub Action [upload-artifact](https://github.com/actions/upload-artifact).

## `test-cases`

`test-cases` uses a GitHub [strategy](https://docs.github.com/en/actions/reference/workflow-syntax-for-github-actions#jobsjob_idstrategy) to run its steps across the different Kubernetes distros and versions summarized at the top of this document.

New Kubernetes distro versions may be added to the job by adding entries to `jobs.test-cases.strategy.matrix.kube`. Each array entry must include:

|Property|Description|
|--------|-----------|
|`runtime`|A unique identifier for this distro-version pair|
|`version`|A distro-specific unique identifier for the Kubernetes version|
|`crictl`|A reference to the release of [`cri-tools`](https://github.com/kubernetes-sigs/cri-tools) including `crictl` that will be used|

Notes:

+ `runtime` is used by subsequent steps as a way to determine the distro, e.g. `startsWith(matrix.kube.runtime, 'K3s')`
+ `version` is used by each distro to determine which binary, snap etc. to install. Refer to each distro's documentation to determine the value required
+ `crictl` is used by `K3s` and `MicroK8s` to determine which version of `crictl` (sic.) is must be installed. `Kubeadm` includes `crictl` and so this variable is left as `UNUSED` for this distro.

### Distro installation and Akri container images

Each distro has an installation step and a step to import the Akri `controller` and `agent` images created by the `build-containers` job.

The installation steps are identified by:

```YAML
if: startsWith(matrix.kube.runtime, ${DISTRO})
```

The installation instructions map closely with the installation instructions provided for the distro. For `K3s` and `MicroK8s`, the step includes installation of `cri-tools` so that `crictl` is available.

The container image import steps are identified by:

```YAML
if: (startsWith(github.event_name, 'pull_request')) && (startsWith(matrix.kube.runtime, ${DISTRO}))
```

### Helm and state

In order to pass state between the workflow and the Python end-to-end test scripts, temporary (`/tmp`) files are used:

|File|Description|
|----|-----------|
|`agent_log.txt`|Filename used by workflow to persist the Agent's log|
|`controller_log.txt`|Filename used by workflow to persist the Controller's log|
|`cri_args_to_test.txt`|`crictcl` configuration that is passed to `helm install` command|
|`extra_helm_args.txt`|Additionl configuration that is passed to `helm install` command|
|`helm_chart_location.txt`|Path to the Helm Chart|
|`kubeconfig_path_to_test.txt`|Path to `kubectl` cluster configuration file|
|`runtime_cmd_to_test`|Location of `kubectl` binary|
|`sleep_duration.txt`|Optional: contains the number of seconds to pause|
|`version_to_test.txt`|Akri version to test|


If you review `/test/run-end-to-end.py` and `/test/shared_test_code.py`, you will see these files referenced.

```Python3
AGENT_LOG_PATH = "/tmp/agent_log.txt"
CONTROLLER_LOG_PATH = "/tmp/controller_log.txt"
KUBE_CONFIG_PATH_FILE = "/tmp/kubeconfig_path_to_test.txt"
RUNTIME_COMMAND_FILE = "/tmp/runtime_cmd_to_test.txt"
HELM_CRI_ARGS_FILE = "/tmp/cri_args_to_test.txt"
VERSION_FILE = "/tmp/version_to_test.txt"
SLEEP_DURATION_FILE = "/tmp/sleep_duration.txt"
EXTRA_HELM_ARGS_FILE = "/tmp/extra_helm_args.txt"
HELM_CHART_LOCATION = "/tmp/helm_chart_location.txt"
```

### Tests

Of all the steps, only one is needed to run the Python end-to-end script.

stdout|stderr from the script can be logged to the workflow.

### Upload

Once the end-to-end script is complete, the workflow uses the GitHub Action [upload-artifact](https://github.com/actions/upload-artifact) again to upload `/tmp/agent_log.txt` and `/tmp/controller_log.txt` so that these remain available (for download) once the workflow completes.