# Anomaly Detection Application
A sample anomaly detection web application for Akri's [OPC UA Thermometer demo](https://docs.akri.sh/demos/opc-thermometer-demo).

Gets temperature values from a set of gRPC servers. It then determines whether this value is an outlier to the dataset
using the Local Outlier Factor strategy. The dataset is simply a csv with the numbers between 70-80 repeated several
times; therefore, any value significantly outside this range will be seen as an outlier. The web application serves as a
log, displaying all the temperature values and the address of the OPC UA Server that sent the values. It shows anomaly
values in red. 

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
Set the path of the `opcua_node.proto` file and generate using `grpc-tools.protoc`. `grpc-tools` should've been installed in the previous step. The following assumes the `akri` repository is in the `$HOME` directory.
```
export SRC_DIR=../../../samples/brokers/opcua-monitoring-broker
python3 -m grpc_tools.protoc -I=$SRC_DIR --python_out=. --grpc_python_out=. $SRC_DIR/opcua_node.proto
```

## Running
When running, `CONFIGURATION_NAME`, `${CONFIGURATION_NAME}_SVC_SERVICE_HOST` and
`${CONFIGURATION_NAME}_SVC_SERVICE_PORT_GRPC` environment variables must be specified. The application will call the
`GetValue` service on the endpoint `${CONFIGURATION_NAME}_SVC_SERVICE_HOST:${CONFIGURATION_NAME}_SVC_SERVICE_PORT_GRPC`
where the gRPC servers should be running.

For example, if the servers are running at `localhost:80`, run the following:
```sh
CONFIGURATION_NAME="akri-opcua"  AKRI_OPCUA_SVC_SERVICE_HOST=localhost AKRI_OPCUA_SVC_SERVICE_PORT_GRPC=80 python3 ./app.py
```