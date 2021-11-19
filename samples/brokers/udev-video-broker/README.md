# Udev USB Video Broker
Sample broker for for Akri's [end to end demo](https://docs.akri.sh/demos/usb-camera-demo). It pulls video frames from
the USB camera with device node `UDEV_DEVNODE`. Then, it serves these frames over a gRPC interface. The [streaming
application](../../apps/video-streaming-app) provides an example streaming service that displays frames served by this
broker.

## Running
1. Install Rust and udev dependencies
    ```sh
    ./build/setup.sh
    ```
1. Build and run, connecting to the USB camera at `/dev/video0`
    ```sh
    cd akri/samples/brokers/udev-video-broker
    UDEV_DEVNODE=/dev/video0 cargo run
    ```