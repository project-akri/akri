#!/bin/bash

# exit on failures
set -e


apt_dependencies="git curl libssl-dev pkg-config libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-plugins-ugly gstreamer1.0-libav libgstrtspserver-1.0-dev libges-1.0-dev libudev-dev libv4l-dev"

echo "User: $(whoami)"

echo "Install rustfmt"
rustup component add rustfmt

echo "Install dependencies: $apt_dependencies"
which sudo > /dev/null 2>&1
if [ "$?" -eq "0" ];
then
    echo "Run sudo apt install ..."
    sudo apt update
    sudo apt install -y $apt_dependencies
else
    echo "Run apt update and apt install without sudo"
    apt update
    apt install -y $apt_dependencies
fi

