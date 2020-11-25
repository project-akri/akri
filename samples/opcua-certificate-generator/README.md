# OPC UA Certificate Generator
This .NET Console application has been created to simplify the process of creating a Certificate Authority (CA) and X.509 v3 certificates issued by that CA for an OPC UA Client and Server(s). It has been provided to create the certificates needed to enable security in the [OPC UA End-to-End Demo](../../docs/opcua-demo.md#creating-x.509-v3-certificates). 
## Running the Application
The application takes in three command line arguments: the path at which to store the certificates, the number of server certificates to create, and the IP address of the host machine for each of the servers. This address is added to the Subject Alternative Names section of the certificate. It is best practice
for that address to reflect where the server will ultimately be run, but it is not required. The program will also create a certificate for the Akri Monitoring broker. 

For example, running `dotnet run /home/user/opcua-certs 2 10.0.0.1 10.0.0.1` will generate a CA certificate and crl, certificates for two OPC UA Servers running on a host with IP address 10.0.0.1, a certificate for an OPC UA Client, and store them at the path /home/user/opcua-certs.

After running the application, the /home/user/opcua-certs directory should look similar to the following:
```
/home/user/opcua-certs
├── ca
│   ├── certs
│   │   └── SomeCA [DC9BBEA17DEF08AA829EB0D1BD1575EB59160695].der
│   ├── crl
│   │   └── SomeCA [DC9BBEA17DEF08AA829EB0D1BD1575EB59160695].crl
│   └── private
│       └── SomeCA [DC9BBEA17DEF08AA829EB0D1BD1575EB59160695].pfx
├── SomeServer0
│   ├── own
│   │   ├── certs
│   │   │   └── SomeServer0 [2ABB5440474F4B126BAA404AFA6981F1BE4CFA52].der
│   │   └── private
│   │       └── SomeServer0 [2ABB5440474F4B126BAA404AFA6981F1BE4CFA52].pfx
│   └── trusted
│       ├── certs
│       │   └── SomeCA [DC9BBEA17DEF08AA829EB0D1BD1575EB59160695].der
│       └── crl
│           └── SomeCA [DC9BBEA17DEF08AA829EB0D1BD1575EB59160695].crl
├── SomeServer1
│   ├── own
│   │   ├── certs
│   │   │   └── SomeServer1 [C0B6F7D8130134D1730A03E7A8FD667C3455A9FD].der
│   │   └── private
│   │       └── SomeServer1 [C0B6F7D8130134D1730A03E7A8FD667C3455A9FD].pfx
│   └── trusted
│       ├── certs
│       │   └── SomeCA [DC9BBEA17DEF08AA829EB0D1BD1575EB59160695].der
│       └── crl
│           └── SomeCA [DC9BBEA17DEF08AA829EB0D1BD1575EB59160695].crl
└── AkriBroker
    ├── own
    │   ├── certs
    │   │   └── AkriBroker [988CF9C439CB28982BC3B462ECC530C00162CE43].der
    │   └── private
    │       └── AkriBroker [988CF9C439CB28982BC3B462ECC530C00162CE43].pfx
    └── trusted
        ├── certs
        │   └── SomeCA [DC9BBEA17DEF08AA829EB0D1BD1575EB59160695].der
        └── crl
            └── SomeCA [DC9BBEA17DEF08AA829EB0D1BD1575EB59160695].crl
```