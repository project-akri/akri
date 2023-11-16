#!/bin/bash

# exit on failures
set -ex

echo "User: $(whoami)"

apt_dependencies="git curl libssl-dev pkg-config libudev-dev libv4l-dev"
echo "Install dependencies: $apt_dependencies"
if [ -x "$(command -v sudo)" ];
then
    echo "Run sudo apt install ..."
    sudo apt update
    sudo apt install -y $apt_dependencies
else
    echo "Run apt update and apt install without sudo"
    apt update
    apt install -y $apt_dependencies
fi

if ! [ -x "$(command -v rustup)" ];
then
    if [ -x "$(command -v sudo)" ];
    then
        echo "Install rustup"
        sudo curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain=1.73.0
    else
        echo "Install rustup"
        curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain=1.73.0
    fi
else
    echo "Found rustup"
fi

echo "Install rustfmt"
rustup component add rustfmt

exit 0