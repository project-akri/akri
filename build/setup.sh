#!/bin/bash

# exit on failures
set -e

echo "User: $(whoami)"

apt_dependencies="git curl libssl-dev pkg-config libudev-dev libv4l-dev"
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

which rustup > /dev/null 2>&1
if [ "$?" -ne "0" ];
then
    which sudo > /dev/null 2>&1
    if [ "$?" -eq "0" ];
    then
        echo "Install rustup"
        sudo curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain=1.41.0
    else
        echo "Install rustup"
        curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain=1.41.0
    fi
fi
echo "Install rustfmt"
rustup component add rustfmt

exit 0