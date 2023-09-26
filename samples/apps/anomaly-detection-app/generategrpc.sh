#!/bin/bash
# based on https://grpc.io/docs/tutorials/basic/python/
ABS_PATH=$(dirname $(readlink -e ../../../samples/brokers/opcua-monitoring-broker/opcua_node.proto))
echo "absolute path to proto file is $ABS_PATH"
python3 -m grpc_tools.protoc -I./ --python_out=. --grpc_python_out=. opcua_node.proto --proto_path=$ABS_PATH
