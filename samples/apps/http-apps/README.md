# HTTP Protocol Sample Device|Discovery apps

This directory provides implementations of IoT devices and a discovery service that can be used to test the Akri HTTP Protocol Broker.

This directory includes an alternative gRPC implementation of the Akri HTTP Protocol gRPC Broker and a Client too.

## Environment

```bash
export REGISTRY="ghcr.io"
export USER=[[GITHUB-USER]]
export PREFIX="http-apps"
export TAG="v1"
```

## Build

The images are built by GitHub Actions in the repository but, you may also build them yourself using:

```bash
./build.sh
```

This will generate 4 images:

+ `${PREFIX}-device`
+ `${PREFIX}-discovery`
+ `${PREFIX}-grpc-broker`
+ `${PREFIX}-grpc-client`

## Device|Discovery Services

There are two applications:

+ `device`
+ `discovery`

### Docker

You may run the images standalone:

```bash
# Create devices on ports 8000:8009
DISCOVERY=()
for PORT in {8000..8009}
do
  # Create the device on ${PORT}
  # For Docker only: name each device: device-${PORT}
  docker run \
  --rm --detach=true \
  --name=device-${PORT} \
  --publish=${PORT}:8080 \
  ${REGISTRY}/${USER}/${PREFIX}-device:${TAG} \
    --path="/"
  # Add the device to the discovery document
  DISCOVERY+=("--device=http://localhost:${PORT} ")
done

# Create a discovery server for these devices
docker run \
  --rm --detach=true \
  --name=discovery \
  --publish=9999:9999 \
  ${REGISTRY}/${USER}/${PREFIX}-discovery:${TAG} ${DISCOVERY[@]}
```

Test:

```bash
curl http://localhost:9999/
http://localhost:8000
http://localhost:8001
http://localhost:8002
http://localhost:8003
http://localhost:8004
http://localhost:8005
http://localhost:8006
http://localhost:8007
http://localhost:8008
http://localhost:8009

curl http://localhost:8006/sensor
```

To stop:

```bash
# Delete devices on ports 8000:8009
for PORT in {8000..8009}
do
  docker stop  device-${PORT}
done

# Delete discovery server
docker stop discovery
```

### Kubernetes

And most useful on Kubernetes because one (!) or more devices can be created and then discovery can be created with correct DNS names.

Ensure the `image` references are updated in `./kubernetes/device.yaml` and `./kubernetes/discovery.yaml`

```bash
for APP in "device" "discovery"
do
  IMAGE="$(docker inspect --format='{{index .RepoDigests 0}}' ${REGISTRY}/${USER}/${PREFIX}-${APP}:${TAG})"
  sed \
  --in-place \
  "s|IMAGE|${IMAGE}|g"
  ./kubernetes/${APP}.yaml
done
```

Then:

```bash

# Create one device deployment
kubectl apply --filename=./device.yaml

# But multiple Services against the single Pod
for NUM in {1..9}
do
  # Services are uniquely named
  # The service uses the Pods port: 8080
  kubectl expose deployment/device \
  --name=device-${NUM} \
  --port=8080 \
  --target-port=8080
done
service/device-1 exposed
service/device-2 exposed
service/device-3 exposed
service/device-4 exposed
service/device-5 exposed
service/device-6 exposed
service/device-7 exposed
service/device-8 exposed
service/device-9 exposed

# Create one discovery deployment
kubectl apply --filename=./discovery.yaml

# Expose Discovery as a service on its default port: 9999
# The Discovery service spec is statically configured for devices 1-9
kubectl expose deployment/discovery \
--name=discovery \
--port=9999 \
--target-port=9999

kubectl run curl --image=radial/busyboxplus:curl --stdin --tty --rm
curl http://discovery:9999
http://device-1:8080
http://device-2:8080
http://device-3:8080
http://device-4:8080
http://device-5:8080
http://device-6:8080
http://device-7:8080
http://device-8:8080
http://device-9:8080
```

Delete:

```bash
kubectl delete deployment/discovery
kubectl delete deployment/device

kubectl delete service/discovery

for NUM in {1..9}
do
  kubectl delete service/device-${NUM}
done
```

## gRPC Broker|Client

This is a Golang implementation of the Broker gRPC server and client. It is an alternative implementation to the Rust gRPC server and client found in `./samples/brokers/http/src/grpc`.

### Docker

These are containerized too:

```bash
docker run \
--rm --interactive --tty \
--net=host \
--name=grpc-broker-golang \
--env=AKRI_HTTP_DEVICE_ENDPOINT=localhost:8005 \
${REGISTRY}/${USER}/${PREFIX}-grpc-broker:${TAG} \
--grpc_endpoint=:50051
```

And:

```bash
docker run \
--rm --interactive --tty \
--net=host \
--name=grpc-client-golang \
${REGISTRY}/${USER}/${PREFIX}-grpc-client:${TAG} \
--grpc_endpoint=:50051
```

### Kubernetes

You will need to replace `IMAGE` and `SECRET` in the Kubernetes configs before you deploy them.

`SECRET` should be replaced with the value (if any) of the Kubernetes Secret that provides the token to your registry.

```bash
for APP in "broker" "client"
do
  IMAGE="$(docker inspect --format='{{index .RepoDigests 0}}' ${REGISTRY}/${USER}/${PREFIX}-grpc-${APP}:${TAG})"
  sed \
  --in-place \
  "s|IMAGE|${IMAGE}|g"
  ./kubernetes/grpc.${APP}.yaml
done
```

Then:

```bash
kubectl apply --filename=./kubernetes/gprc.broker.yaml
kubectl apply --filename=./kubernetes/grpc.client.yaml
```

