# Requesting Akri Resources

Akri discovers tiny devices, advertizes them as resources, and automatically deploys workloads to utilize those devices. The latter functionality is optional. You can use Akri solely to discover and advertize devices by omitting a broker pod image from a Configuration. Then, you can schedule your own pods, requesting the discovered Akri Instances \(which represent each tiny device\) as resource limits.

Lets walk through how this works, using the ONVIF Discovery Handler as an example. Install Akri with the ONVIF Discovery Handler and Configuration, omitting a broker pod image.

```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set onvif.discovery.enabled=true \
    --set onvif.configuration.enabled=true
```

After installing Akri and your Configuration, list all discovered instances by running `kubectl get akrii`. Note `akrii` is a short name for Akri Instance. All the instances will be named in the format `<configuration-name>-<id>`, where `id` varies whether or not the device is sharable or visible by multiple nodes. 

1. For unshared devices, `id` is a hash of a descriptor of the device and the name of the node that can see the device. For example, the `id` of an Instance representing a usb camera at devnode `/dev/video0` on a node named workerA would be `hash(/dev/video0workerA)`.
2. For shared devices, `id` is only a hash of the descriptor of the device. This way, all agents create or modify an Instance with the same name for the same device. For example, since IP cameras are sharable, the `id` for an IP camera would be `hash(uri)`.

You can change the name of the Configuration and resultant Instances to be `onvif-camera` by adding `--set onvif.configuration.name=onvif-camera` to your installation command. Now, you can schedule pods that request these Instances as resources. Assuming the Configuration name has been set to `onvif-camera`, you can request the `onvif-camera-<id>` Instance as a resource by adding the following to the PodSpec of your Deployment or Job:

```yaml
  resources:
    limits:
      akri.sh/onvif-camera-<id>: "1"
    requests:
      akri.sh/onvif-camera-<id>: "1"
```

As an example, a Deployment that would deploy an nginx broker to one of the devices discovered by the ONVIF Discovery Handler may look like this:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: onvif-camera-broker-deployment
  labels:
    app: onvif-camera-broker
spec:
  replicas: 1
  selector:
    matchLabels:
      app: onvif-camera-broker
  template:
    metadata:
      labels:
        app: onvif-camera-broker
    spec:
      containers:
      - name: onvif-camera-broker
        image: nginx
        resources:
          limits:                        
            akri.sh/onvif-camera-<id>: "1"
          requests:
            akri.sh/onvif-camera-<id>: "1"
```

Apply your Deployment to the cluster and watch the broker start to run. If you inspect the Instance of the resource you requested in your deployment, you will see one of the slots has now been reserved by the node that is currently running the broker.

```bash
kubectl apply -f deployment-requesting-onvif-camera.yaml                                  
kubectl get akrii onvif-camera-<id> -o yaml
```

