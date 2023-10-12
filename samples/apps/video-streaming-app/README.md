# Video Streaming Application
## Overview
This application serves as an example streaming service for the [ONVIF broker](../../brokers/onvif-video-broker) and
[USB camera broker](../../brokers/udev-video-broker). It is used in Akri's [end to end
demo](https://docs.akri.sh/demos/usb-camera-demo). Both brokers act as gRPC services that sit on port 8083. The
streaming application creates gRPC clients to connect to the services and repeatedly calls `get_frame` to get the
images. It uses Flask to implement streaming.
## Limitations
This app streams images in mjpeg (Motion JPEG) format, since all browsers natively support mjpeg. This means this
application will only work on cameras that support MJPG or JPEG. The onvif-video-broker connects to the RTSP stream of
the cameras, which supports JPEG; however, not all usb cameras support MJPG/JPEG. To check that your camera supports
MJPG/JPEG, observe the output of `sudo  v4l2-ctl --list-formats` on the associated node.

## Dependencies
> Note: using a virtual environment is recommended with pip

Install pip:
```
sudo apt-get install -y python3-pip
```
Navigate to this directory and use pip to install all dependencies in `requirements.txt`.
```
pip install -r requirements.txt
```

To clean up, simply run `pip uninstall -r requirements.txt -y`.

## Generating Protobuf Code
Generate using `grpc-tools.protoc`. `grpc-tools` should've been installed in the previous step. 
```
python3 -m grpc_tools.protoc -I./ --python_out=. --grpc_python_out=. camera.proto
```

## Running
The streaming application works in two modes. 
1. Explicitly target a set of cameras by setting `CAMERA_COUNT`, a service to target all cameras (`CAMERAS_SOURCE_SVC`),
   and services for each individual camera (`CAMERA1_SOURCE_SVC` to `CAMERA${CAMERA_COUNT}_SOURCE_SVC`) 
```sh
CAMERA_COUNT="2"  CAMERAS_SOURCE_SVC=10.2.2.2 CAMERA1_SOURCE_SVC=10.1.2.3 CAMERA2_SOURCE_SVC=10.2.3.4 python3 ./app.py
```
2. Target all services of an Akri Configuration. The application will query for services prefixed with the Configuration
   name.
```sh
CONFIGURATION_NAME="akri-udev" python3 ./app.py
```