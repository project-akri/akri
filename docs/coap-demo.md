# CoAP demo

The Constrained Application Protocol (CoAP) is a specialized web transfer protocol with constrained nodes and constrained (e.g., low-power, lossy) networks. This demo will show how Akri can discover CoAP devices and allow cluster applications to communicate with them. To do so, the built-in Akri CoAP Broker exposes CoAP resources as REST resources via HTTP.

The demo consists of the following steps:

1. Initialize a CoAP device with two resources: temperature and light brightness.
2. Start an Akri installation
3. Deploy a Kubernetes application that requests the temperature via HTTP

## Creating CoAP devices

If you don't have existing devices running CoAP, you can create a CoAP server by yourself on the running machine.

We will use Rust and [coap-rs](https://github.com/Covertness/coap-rs) in this section, but any language implementation of CoAP is fine (e.g. [node-coap](https://github.com/mcollina/node-coap)). The final example can be found in `/samples/app/coap-device`.

1. Create a new Rust project anywhere you like

    ```
    cargo new coap-device
    ```

2. Add the following dependencies to `Cargo.toml`

    ```toml
    [dependencies]
    coap = "0.11.0"
    coap-lite = "0.5"
    tokio = "1.8.1"
    ```

3. Implement a simple CoAP server in `src/main.rs`, which exposes the `/sensors/temp` and `/sensors/light` resources.

  ```rs
  #![feature(async_closure)]

  use coap::Server;
  use coap_lite::{ContentFormat, RequestType as Method, ResponseType as Status};
  use tokio::runtime::Runtime;

  fn main() {
      let addr = "0.0.0.0:5683";

      Runtime::new().unwrap().block_on(async move {
          let mut server = Server::new(addr).unwrap();
          println!("CoAP server on {}", addr);

          server
              .run(async move |request| {
                  let method = request.get_method().clone();
                  let path = request.get_path();
                  let mut response = request.response?;

                  match (method, path.as_str()) {
                      (Method::Get, "well-known/core") => {
                          response
                              .message
                              .set_content_format(ContentFormat::ApplicationLinkFormat);
                          response.message.payload =
                              br#"</sensors/temp>;rt="oic.r.temperature";if="sensor",
                          </sensors/light>;rt="oic.r.light.brightness";if="sensor""#
                                  .to_vec();
                      }
                      (Method::Get, "sensors/temp") => {
                          response
                              .message
                              .set_content_format(ContentFormat::TextPlain);
                          response.message.payload = b"42".to_vec();
                      }
                      (Method::Get, "sensors/light") => {
                          response
                              .message
                              .set_content_format(ContentFormat::TextPlain);
                          response.message.payload = b"100".to_vec();
                      }
                      _ => {
                          response.set_status(Status::NotFound);
                          response.message.payload = b"Not found".to_vec();
                      }
                  }

                  Some(response)
              })
              .await
              .unwrap();
      });
  }
  ```

4. Run the server

  ```
  cargo run
  ```

It's not necessary, but you can check if the server is correctly up and running creating a separate Rust project for a CoAP client with the following `main.rs`:

```rs
use coap::CoAPClient;

fn main() {
    let url = "coap://192.168.1.126:5683/sensors/temp"; // Properly set your machine's IP
    let response = CoAPClient::get(url).unwrap();
    println!("Server reply: {}", String::from_utf8(response.message.payload).unwrap());
}
```

## Running Akri

Now it is time to install the Akri using Helm. When installing Akri, we can specify that we want to deploy the CoAP Discovery Handlers by setting the helm value `coap.discovery.enabled=true`. We also specify that we want to create a CoAP Configuration with `--set coap.configuration.enabled=true`.

In the Configuration, we need to specify our CoAP server's static IP address. Otherwise, the Discovery Handler's default behaviour is to use the broadcast method: `--set coap.configuration.discoveryDetails.staticIpAddresses[0] = "192.168.1.126"`.

The final CLI commands should look like this:

  ```sh
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set coap.discovery.enabled=true \
    --set coap.configuration.enabled=true \
    --set coap.configuration.discoveryDetails.staticIpAddresses[0] = "192.168.1.126"
```

The Akri Discovery Handler will discover the CoAP server and create an instance. 


```sh
kubectl get pods
```

The result should look like this:

```
NAME                                          READY   STATUS    RESTARTS   AGE
akri-agent-daemonset-p8w8z                    1/1     Running   0          75s
akri-coap-discovery-daemonset-sq6pt           1/1     Running   0          75s
akri-controller-deployment-5bc76f77d8-9ldws   1/1     Running   0          75s
kind-control-plane-akri-coap-920f97-pod       1/1     Running   0          64s
```

To inspect more of the elements of Akri:

- Run `kubectl get crd`, and you should see the CRDs listed.
- Run `kubectl get akric`, and you should see `akri-coap`. 
- The instances can be seen by running `kubectl get akrii` and
  further inspected by running `kubectl get akrii akri-coap-<ID> -o yaml`

## Deploy a Kubernetes application which requests the temperature via HTTP

The Akri CoAP built-in Broker Pod is automatically deployed when an instance is created. The Broker exposes a RESTful endpoint which translates HTTP request to CoAP requests. To inspect the associated Kubernetes Service, run `kubectl get crd`.

```
NAME                   TYPE        CLUSTER-IP      EXTERNAL-IP   PORT(S)   AGE
akri-coap-920f97-svc   ClusterIP   10.96.190.197   <none>        80/TCP    10m
akri-coap-svc          ClusterIP   10.96.87.25     <none>        80/TCP    10m
```

We can now deploy a simple application which just allows to send normal HTTP requests using the terminal.

```
kubectl run curl --image=radial/busyboxplus:curl -i --tty

[ root@curl:/ ]$ curl http://akri-coap-920f97-svc/sensors/temp
```

