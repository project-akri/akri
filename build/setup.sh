#!/bin/bash

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
if [ "$?" -ne "0" ]; then
    echo "Failed to apt install: $apt_dependencies"
    exit $?
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
    if [ "$?" -ne "0" ]; then
        echo "Failed to install rustup"
        exit $?
    fi
else
    echo "Found rustup"
fi

echo "Install rustfmt"
rustup component add rustfmt
if [ "$?" -ne "0" ]; then
    echo "Failed to install rustfmt"
    exit $?
fi

exit 0