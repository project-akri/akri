# Video Streaming Application
## Overview
This application serves as an example streaming service for the onvif-video-broker and udev-camera-broker. Both brokers act as gRPC services that sit on port 8083. The streaming application creates gRPC clients to connect to the services and repeatedly calls `get_frame` to get the images. It uses Flask to implement streaming.
## Limitations
This app streams images in mjpeg (Motion JPEG) format, since all browsers nativel support mjpeg. This means this application will only work on cameras that support MJPG or JPEG. The onvif-video-broker connects to the RTSP stream of the cameras, which supports JPEG; however, not all usb cameras support MJPG/JPEG. To check that your camera supports MJPG/JPEG, observe the output of `sudo  v4l2-ctl --list-formats` on the associated node.
