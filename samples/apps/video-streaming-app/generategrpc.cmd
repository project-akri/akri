:: based on https://grpc.io/docs/tutorials/basic/python/
python -m grpc_tools.protoc -I./ --python_out=. --grpc_python_out=. camera.proto