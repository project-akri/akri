#!/bin/bash
# based on https://grpc.io/docs/tutorials/basic/python/
python3 -m grpc_tools.protoc -I./ --python_out=. --grpc_python_out=. camera.proto
