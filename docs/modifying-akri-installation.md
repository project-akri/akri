# Modifying a Akri Installation
The [onvif](./onvif-sample.md) and [udev](./udev-sample.md) documentation explains how to deploy Akri for a specific
protocol Configuration. This documentation elaborates upon them, covering the following:
1. Starting Akri without any Configurations
1. Deploying multiple Configurations
1. Modifying a deployed Configuration
1. Adding another Configuration to a cluster
1. Deleting a Configuration from a cluster

## Starting Akri without any Configurations
To install Akri without any protocol Configurations, run this:
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set useLatestContainers=true
```
This will start the Akri controller and deploy Akri Agents.

## Deploying multiple Configurations using `helm install`
If you want your end application to consume frames from both IP cameras and locally attached cameras, Akri can be
installed from the start with both the ONVIF and udev Configurations like so:
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set useLatestContainers=true \
    --set onvifVideo.enabled=true \
    --set udevVideo.enabled=true
```
You can confirm that both a akri-onvif-video and akri-udev-video Configuration have been created by running:
``` bash
kubectl get akric
```

## Modifying a deployed Configuration
An already deployed Configuration can be modified in one of two ways:
1. Using the `helm upgrade` command
2. Generating, modifying, and applying a Configuration yaml

### Using `helm upgrade` 
A Configuration can be modified by using the `helm upgrade` command. It upgrades an existing release according to the
values provided, only updating what has changed. Simply modify your `helm install` command to reflect the new **desired
state** of Akri and replace `helm install` with `helm upgrade`. Using the ONVIF protocol implementation as an example,
say an IP camera with IP address 10.0.0.1 is malfunctioning and should be filtered out of discovery, the following
command could be run:
```bash 
helm upgrade akri akri-helm-charts/akri \
    --set useLatestContainers=true \
    --set onvifVideo.enabled=true \
    --set onvifVideo.ipAddresses.action=Exclude \
    --set onvifVideo.ipAddresses.items[0]=10.0.0.1
```
Note that the command is not simply `helm upgrade --set onvifVideo.ipAddresses.items[0]=10.0.0.1`; rather, it includes
all the old settings along with the new one. 

Helm will create a new ONVIF Configuration and apply it to the cluster.
When Agent sees that a Configuration has been updated, it deletes all Instances associated with that Configuration and
the controller brings down all associated broker pods. Then, new instances and broker pods are created. Therefore, the
command above will bring down all ONVIF broker pods and then bring them all back up except for the ones servicing the IP
camera at IP address 10.0.0.1.

### Generating, modifying, and applying a Configuration yaml
Helm allows us to parametrize the commonly modified fields in our configuration files and we have provided many (to see
them, run `helm inspect values akri-helm-charts/akri`).  For more advanced configuration changes that are not aided by
our Helm chart, we suggest creating a Configuration file using helm and then manually modifying it.

For example, to create an ONVIF Configuration file, run the following. (To instead create a udev Configuration,
substitute `onvifVideo.enabled` with `udevVideo.enabled`.)
```bash
helm template akri akri-helm-charts/akri \
    --set useLatestContainers=true \
    --set onvifVideo.enabled=true \
    --set controller.enabled=false \
    --set agent.enabled=false > configuration.yaml
```
Once you have modified the yaml file, you can apply the new Configuration to the cluster with standard kubectl like
this:
```bash
kubectl apply -f configuration.yaml
```

#### Modifying the brokerPodSpec
The `brokerPodSpec` property is a full
[PodSpec](https://kubernetes.io/docs/reference/generated/kubernetes-api/v1.18/#podspec-v1-core) and can be modified as
such.  For example, if you wanted to allow the master Node to potentially have a protocol broker Pod scheduled to it,
you could modify the Configuration, ONVIF in this case, like so:
```yaml
spec:
  brokerPodSpec:
    containers:
    - name: akri-onvif-video-broker
      image: "ghcr.io/deislabs/akri/onvif-video-broker:latest-dev"
      imagePullPolicy: Always
      resources:
        limits:
          "{{PLACEHOLDER}}" : "1"
    tolerations:
      - key: node-role.kubernetes.io/master
        effect: NoSchedule
```

Another reason one might modify the brokerPodSpec would be to add some resource limits.  To do this, you can modify the
Configuration like this:
```yaml
spec:
  brokerPodSpec:
    containers:
    - name: akri-onvif-video-broker
      image: "ghcr.io/deislabs/akri/onvif-video-broker:latest-dev"
      imagePullPolicy: Always
      resources:
        requests:
          memory: 30Mi
          cpu: 100m
        limits:
          memory: 50Mi
          cpu: 200m
          "{{PLACEHOLDER}}" : "1"
```

**Note:** the `{{PLACEHOLDER}}` limit will be used by Akri to utilize this Configuration's Instances' capacity.

#### Modifying instanceServiceSpec or configurationServiceSpec
The `instanceServiceSpec` and `configurationServiceSpec` properties are full
[ServiceSpecs](https://kubernetes.io/docs/reference/generated/kubernetes-api/v1.18/#servicespec-v1-core) and can be
modified as such.  The simplest reason to modify either might be to specify different ports (perhaps 8085 and 8086):
```yaml
spec:
  instanceServiceSpec:
    ports:
    - name: grpc
      port: 8085
      targetPort: 8083
  configurationServiceSpec:
    ports:
    - name: grpc
      port: 8086
      targetPort: 8083
```

Note: the simple properties of `instanceServiceSpec` and `configurationServiceSpec` (like name, port, targetPort) can be
set using Helm's `--set` command (`--set onvifVideo.instanceService.targetPort=90`).

## Adding another Configuration to a cluster
Another Configuration can be added to an existing Akri installation using `helm upgrade` or manually using `helm
template` and kubectl.
### Adding additional Configurations using `helm upgrade`
Another Configuration can be added to the cluster by using `helm upgrade`. If you originally installed just the ONVIF
Configuration and now also want to discover local cameras via udev, as well, simply run the following:
```bash
helm upgrade akri akri-helm-charts/akri \
    --set useLatestContainers=true \
    --set onvifVideo.enabled=true \
    --set udevVideo.enabled=true
```
### Adding additional Configurations manually
An additional Configuration can also be added to an existing Akri installation using the same process of using `helm
template` to generate a Configuration and then using kubectl to apply it as in the ["Generating, modifying, and applying
a Configuration yaml"](#generating-modifying-and-applying-a-configuration-yaml) section above.

## Deleting a Configuration from a cluster
If an operator no longer wants Akri to discover devices defined by a Configuration, they can delete the Configuration
and all associated broker pods will automatically be brought down. This can be done with `helm upgrade` or kubectl.
### Deleting a Configuration using `helm upgrade`
A Configuration can be deleted from a cluster using `helm upgrade`. For example, if both ONVIF and udev Configurations
have been installed in a cluster, the udev Configuration can be deleted by only specifying the ONVIF Configuration in a
`helm upgrade` command like the following:
```bash
helm upgrade akri akri-helm-charts/akri \
    --set useLatestContainers=true \
    --set onvifVideo.enabled=true 
```
### Deleting a Configuration using kubectl
A configuration can also be deleted using kubectl. To list all applied Configurations, run `kubectl get akric`. If both
udev and ONVIF Configurations have been applied with capacities of 5. The output should look like the following:
```bash
NAME                CAPACITY   AGE
akri-onvif-video   5          3s
akri-udev-video    5          16m
```
To delete the ONVIF Configuration and bring down all ONVIF broker pods, run:
```bash 
kubectl delete akric akri-onvif-video
```