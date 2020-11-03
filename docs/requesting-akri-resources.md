# Requesting Resources Advertized by Akri
Akri discovers tiny devices, advertizes them as resources, and automatically deploys workloads to utilize those devices.
The latter functionality is optional. You can use Akri solely to discover and advertize devices by omitting a broker pod
image from a Configuration. Then, you can schedule your own pods, requesting the discovered Akri Instances (which
represent each tiny device) as resource limits. 

Lets walk through how this works for some protocol named `protocolA`. Install Akri with the `protocolA` Configuration,
omitting a broker pod image. Note, `protocolA` must be a supported Akri discovery protocol -- currently udev or ONVIF. 
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri-dev \
    --set useLatestContainers=true \
    --set protocolA.enabled=true
```
After installing Akri and your Configuration, list all discovered instances by running `kubectl get akrii`. Note `akrii`
is a short name for Akri Instance. All the instances will be named in the format `<configuration-name>-<id>`, where `id`
varies whether or not the device is sharable or visible by multiple nodes.
1. For unshared devices, `id` is a hash of a descriptor of the device and the name of the node that can see the device.
   For example, the `id` of an Instance representing a usb camera at devnode `/dev/video0` on a node named workerA would
   be `hash(/dev/video0workerA)`.
1. For shared devices, `id` is only a hash of the descriptor of the device. This way, all agents create or modify an
   Instance with the same name for the same device. For example, since IP cameras are sharable, the `id` for an IP camera
   would be `hash(uri)`. 
   
You can change the name of the Configuration and resultant Instances to be `protocolA-device` by adding `--set protocolA.name=protocolA-device` to your installation command. Now, you can schedule pods that request these Instances as resources. Assuming the Configuration name has been set to `protocolA-device`, you can request the `protocolA-device-<id>` Instance as a resource by adding the following to the PodSpec of your Deployment or Job:
```yaml
  resources:
    limits:
      akri.sh/protocolA-device-<id>: "1"
    requests:
      akri.sh/protocolA-device-<id>: "1"
```
As an example, a Deployment that would deploy an nginx broker to one of the devices discovered by `protocolA` may look
like this:
```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: protocolA-broker-deployment
  labels:
    app: protocolA-broker
spec:
  replicas: 1
  selector:
    matchLabels:
      app: protocolA-broker
  template:
    metadata:
      labels:
        app: protocolA-broker
    spec:
      containers:
      - name: protocolA-broker
        image: nginx
        securityContext:
          privileged: true
        resources:
          limits:                        
            akri.sh/protocolA-device-<id>: "1"
          requests:
            akri.sh/protocolA-device-<id>: "1"
```
Apply your Deployment to the cluster and watch the broker start to run. If you inspect the Instance of the resource you
requested in your deployment, you will see one of the slots has now been reserved by the node that is currently running
the broker.
```sh
kubectl apply -f deployment-requesting-protocolA-device.yaml                                  
kubectl get akrii protocolA-device-<id> -o yaml
```