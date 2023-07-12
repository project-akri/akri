# OPC UA Monitoring Broker
Sample broker for for Akri's [OPC UA Thermometer demo](https://docs.akri.sh/demos/opc-thermometer-demo). Contains an OPC
UA Client that will subscribe to an OPC UA Server Node with a specific identifier and namespace index. It then serves the
value of the Node (or Variable) over gRPC for an [anomaly detection web application](../../apps/anomaly-detection-app)
to consume. 

## Running
1. Install .NET according to [.NET instructions](https://docs.microsoft.com/dotnet/install/linux-ubuntu)
1. Build
```sh
cd ./samples/brokers/opcua-monitoring-broker
dotnet build
```
1. Run, passing in the OPC UA Discovery URL for the OPC UA Server it should connect to and the identifier and namespace
   index of the OPC UA Node to monitor.
```sh
IDENTIFIER="Thermometer_Temperature" NAMESPACE_INDEX="2" OPCUA_DISCOVERY_URL_ABCDEF="opc.tcp://10.2.3.4:4556/Some/Path" dotnet run
```
