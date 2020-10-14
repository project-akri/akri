# Credentials Passing in Akri

## Background
Many of the protocols Akri has and will implement require some sort of authentication in order to access information on the edge device. So far, Akri has assumed no security requirements for both udev and ONVIF. This One-Pager discusses how Kubernetes Secrets can be used to fill that need, highlighting the scenario of OPC UA.

## Are Kubernetes Secrets secure?
Kubernetes (K8s) Secrets provide a way to mount a secret into a PodSpec at a certain path or as environment variables. By default, they are not secure; however, there have been some recent improvements. Before K8s 1.7, Secrets were stored as base-64 encoded plain text in etcd. If an attacker got access to etcd, the secrets could be exposed. K8s 1.7 released the concept of an EncryptionConfiguration, which provides envelope configuration; however, it lives in plain text on the master nodes, which is also where the envelope encryption keys (DEK and KEK) live.<sup>1</sup> K8s 1.10 provides a solution to this open attack surface. It provides a way to manage the encryption of Secrets external to the K8s cluster in a remote KMS, which is communicated with over a unix socket. There are several implementations of Kubernetes KMS plugins by Azure, Oracle, and more.<sup>2</sup> Ultimately, Kubernetes secrets can be secure, so long as an EncryptionConfiguration is used along with a KMS plugin. This video provides a great overview of the evolution of Kubernetes Secrets.<sup>3</sup> 

## How should Secrets be passed to brokers?
Whether secrets should be passed as environment variables or mounted files could vary by protocol. Either way, they are mounted in broker Pod Specs, so it makes sense to add them to each protocol’s configuration as needed rather than as part of the Configuration CRD.

### How Secrets are used in OPC UA 
For OPC UA, a client maintains a certificate store at a certain path. So the Kubernetes Secrets only need to be passed in at the path expected by the OPC UA Client (Akri Broker). In this case, as shown in the abreviated broker PodSpec below, that path is /etc/opcua-cert/client-pki. Specifically, the client certificate can be found at /etc/opcua-cert/client-pki/own/certs/AkriClient.der. 
```
brokerPodSpec:
    containers:
    - name: akri-opcua-monitoring-broker
      volumeMounts:
      - name: credentials
        mountPath: "/etc/opcua-certs/client-pki"
        readOnly: false
    volumes:
    - name: credentials
      secret:
        secretName: opcua-broker-credentials
        items:
        - key: client_certificate
          path: own/certs/AkriClient.der
        - key: client_key
          path: own/private/AkriClient.pfx
        - key: ca_certificate
          path: trusted/certs/AkriCA.der
        - key: ca_crl
          path: trusted/crl/AkriCA.crl
```

For ONVIF, credentials are in the form of username and password. These could similarly be passed as files, whose contents are read by the broker, or they could be mounted as environment variables. 

---
1 [Kubernetes Encrypting Secret Data at Rest documentation](https://kubernetes.io/docs/tasks/administer-cluster/encrypt-data/) warns, "Storing the raw encryption key in the EncryptionConfig only moderately improves your security posture, compared to no encryption. Please use kms provider for additional security".

2 Repositories for [Azure KMS Plugin](https://github.com/Azure/kubernetes-kms) and [Oracle Vault KMS Plugin](https://github.com/oracle/kubernetes-vault-kms-plugin)


4 [Base64 is not encryption: A better story for Kubernetes Secrets](https://www.youtube.com/watch?v=f4Ru6CPG1z4)
