#!/usr/bin/env bash

sudo snap install microk8s --classic --channel=1.18/stable

sudo microk8s status --wait-ready

sudo microk8s enable dns helm3 rbac

echo "--allow-privileged=true" | sudo tee -a /var/snap/microk8s/current/args/kube-apiserver

sudo microk8s stop && microk8s start

KUBERNETES_DISTRO="--set kubernetesDistro=microk8s"

sudo apt update
sudo apt -y install linux-modules-extra-$(uname -r)
sudo apt -y install dkms

curl http://deb.debian.org/debian/pool/main/v/v4l2loopback/v4l2loopback-dkms_0.12.5-1_all.deb -o v4l2loopback-dkms_0.12.5-1_all.deb
sudo dpkg -i v4l2loopback-dkms_0.12.5-1_all.deb
sudo modprobe v4l2loopback exclusive_caps=1 video_nr=1,2

sudo apt-get install -y \
  libgstreamer1.0-0 gstreamer1.0-tools gstreamer1.0-plugins-base \
  gstreamer1.0-plugins-good gstreamer1.0-libav

sudo gst-launch-1.0 -v videotestsrc pattern=ball ! "video/x-raw,width=640,height=480,framerate=10/1" ! avenc_mjpeg ! v4l2sink device=/dev/video1 &

sudo gst-launch-1.0 -v videotestsrc pattern=smpte horizontal-speed=1 ! "video/x-raw,width=640,height=480,framerate=10/1" ! avenc_mjpeg ! v4l2sink device=/dev/video2 &

sudo microk8s.helm3 repo add akri-helm-charts https://project-akri.github.io/akri/
sudo microk8s.helm3 install akri akri-helm-charts/akri-dev \
  $KUBERNETES_DISTRO \
  --set useLatestContainers=true \
  --set udev.enabled=true \
  --set udev.name=akri-udev-video \
  --set udev.udevRules[0]='KERNEL=="video[0-9]*"' \
  --set udev.brokerPod.image.repository="ghcr.io/project-akri/akri/udev-video-broker:latest-dev"

until sudo microk8s.kubectl wait pod --for=condition=ready --selector=akri.sh/configuration=akri-udev-video
do
  echo "Waiting for udev video brokers to become ready"
  sleep 10s
done

sudo microk8s.kubectl apply \
--filename=https://raw.githubusercontent.com/project-akri/akri/main/deployment/samples/akri-video-streaming-app.yaml
