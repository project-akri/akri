# ONVIF Video Broker
Sample broker for for Akri's [ONVIF Configuration](https://docs.akri.sh/discovery-handlers/onvif). Pulls video frames
from the rtsp stream of the ONVIF camera at `ONVIF_DEVICE_SERVICE_URL`. Then, it serves these frames over a gRPC
interface. 

## Running
1. Install .NET according to [.NET instructions](https://docs.microsoft.com/dotnet/core/install/linux-ubuntu)
1. Install [opencvsharp](https://github.com/shimat/opencvsharp), the OpenCV wrapper for .NET
1. Build
    ```sh
    cd ./samples/brokers/onvif-video-broker
    dotnet build
    ```
1. Run the broker, passing in the ONVIF service URL for the camera it should pull frames from.
    ```sh
    ONVIF_DEVICE_SERVICE_URL=http://10.1.2.3:1000/onvif/device_service dotnet run
    ```

    The broker will attempt to connect to the OPC UA server without credentials unless it sees certificates at
    `/etc/opcua-certs/client-pki`. The [OPC UA certificate generator](../../opcua-certificate-generator) can be used to
    generate certificates with the same root certificate for both the OPC UA server and this broker.