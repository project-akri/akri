# Run Webhook End-to-End Tests

File: `/tests/run-webhook.py`

Complements existing Python-based end-to-end test [script](/test/run-end-to-end.py) with a script to test Akri configured to use the Configuration Admission Controller Webhook ([README](/webhooks/validating/configuration/README.me)).

The Webhook validates Akri Configurations, permitting (semantically) valid Configurations to be applied to a cluster and prohibiting (semantically) invalid Configurations.

In order to create an end-to-end test including the Webhook:

1. Akri (including the Webhook) is deployed to a cluster
1. A valid Configuration is applied and, confirmed to have been applied by retrieval
1. An invalid Configuration is applied and, confirmed to have been trapped by the Webhook by catching an (API) exception
1. The cluster is deleted.

## ImagePullSecrets

When running the script outside of the GitHub Actions workflow, you may need to configure the Kubernetes cluster to access a private registry, for example GitHub Container Registry (aka GHCR). The simplest way to authenticate to a private registry is to create a Secret (e.g. `${SECRET}`) containing the credentials in the Namespace(s) and configure Helm to reference the Secret when deploying Akri: `--set=imagePullSecrets[0].name=${SECRET}`

## Configuration

The Webhook requires a certificate and key. The certificate must correctly reference the Webhook's Kubernetes' service name through its Subject Alternate Name (SAN) configuration. 

The test includes 2 certificates (and their associate keys). Both require that the Webhook's name (`WEBHOOK_NAME`) be `akri-webhook-configuration`.

The script is configured to use the first cert|key pair in the `default` namespace with a Service name: `akri-webhook-configuration.default.svc.cluster.local`. The second cert|key pair is for the `deleteme` namespace (see below) for Service name: `akri-webhook-configuration.deleteme.svc.cluster.local`.

If you wish to use a different Webhook name or namespace, you will need to generate a new cert|key pair, then reconfigure the script using these and the CA. See [Generate Certificate|Key](#Generate-CertKey).

The GitHub Actions workflow applies end-to-end tests to the test cluster's `default` namespace. This script permits non-`default` clusters to be used, for example when testing the script locally. To further simplify this process and avoid having to create a certificate and key for the Webhook, a certificate is provided that works with a namespace named `deleteme`. If you would like to use `deleteme` instead of `default` namespace:

+ Ensure the namespace exists: `kubectl create namespace deleteme`
+ Update the script `namespace="deleteme"`
+ Replace the value of `CRT` and `KEY` in the script with those below

```Python
# CRT|KEY defined (DNS) for `akri-webhook-configuration.deleteme.svc`
CRT = "LS0tLS1CRUdJTiBDRVJUSUZJQ0FURS0tLS0tCk1JSURUVENDQWpXZ0F3SUJBZ0lRUW4zSVpvZStLby9DQnllSDBuaXJJVEFOQmdrcWhraUc5dzBCQVFzRkFEQU4KTVFzd0NRWURWUVFEREFKRFFUQWVGdzB5TVRBeU1URXhOek0zTVRGYUZ3MHlNakF5TVRFeE56TTNNVEZhTUFBdwpnZ0VpTUEwR0NTcUdTSWIzRFFFQkFRVUFBNElCRHdBd2dnRUtBb0lCQVFERmxOZ3kxNWJjOG1EeURINk5QQVdQCnFmUGVUY0VCN2NQYjliaGNTYzVaK0F0V2FHWk8rM2RKb1pFdGkwN01lNW9qa3p4WkNLMk41NXcxL0k2SWR3K00KQzJKQlFtYitiR1lMMjJOdFhwdXQxMXpyVWpNM0t5emlZUkhxVE5iSWdITEREV2l4QWt0UG56TGVPZnp0UXlOSwpUTGNZTXpLT3hybkpyai9YWjhiU2RYRUNwakREM3BIVEdjcWVkQjdpWTB5ZVJ0MmJYMFI3MU9sMlJIaFkrUFdPCjhwb3N4STNQeUV4VW1LZU4vMDhpMSs4dWRLV0R0Mm4velNsRExKS2ZFTFJJZTI1T0kvOURldjlUWnZWeTVtYWcKR0RyZ0d4VlFlVG9XVFNMTXNZK3l6ODFudWhlTTRkUldKbGl0azRPbnFZdlpHcFVDQ3BFeGhPZkR6a1RKcElGbApBZ01CQUFHamdiVXdnYkl3RXdZRFZSMGxCQXd3Q2dZSUt3WUJCUVVIQXdFd0RBWURWUjBUQVFIL0JBSXdBREFmCkJnTlZIU01FR0RBV2dCUlVKd3FRQ3dHdUlQV0wrSVhDSjgrNlZjdk8yakJzQmdOVkhSRUJBZjhFWWpCZ2dpZGgKYTNKcExYZGxZbWh2YjJzdFkyOXVabWxuZFhKaGRHbHZiaTVrWld4bGRHVnRaUzV6ZG1PQ05XRnJjbWt0ZDJWaQphRzl2YXkxamIyNW1hV2QxY21GMGFXOXVMbVJsYkdWMFpXMWxMbk4yWXk1amJIVnpkR1Z5TG14dlkyRnNNQTBHCkNTcUdTSWIzRFFFQkN3VUFBNElCQVFDTklGUnVHSHdjVnRWTXlhTEZqTW5BSktBQlNVL2hEOTlhTnJsRUU1aTQKRGkyeDExYUVFNVFkWS9RdnE3bXYzUk1RL2Y1NEZpYjVETURpSG50Z0F1ZHlTajZtT1pBUG1TMVFXTVo4QlhlOQphTzJMWVczYnBmQUIwSytFUkJ4NWRwdXBoYWZYR2hNR09VeGtMelNucUptS0lhSmF2V3JyYTV1cFd0dExDVDRpCjhFenNnb25ESzA5Si9WanBnYWhFUW1jMjBmcytHZ3QvNThEdmZuMSttMG4zNGVpakc1MWx4eVM3aWkwQi9WdkMKVE55WUYweWtSTWJrRWM5YzRkdHc5bnNiZHI5WFNIZFpFSFIxaDZUcnpldlRFQzlteU91UGw3V0tUaG1SVE5qWApkWGNTVkZtb1VpbDJDbGNxd001Q2c2TGd6Y0k4Zm10VlNVeVVGYmZwUkNYeQotLS0tLUVORCBDRVJUSUZJQ0FURS0tLS0tCg=="
KEY = "LS0tLS1CRUdJTiBSU0EgUFJJVkFURSBLRVktLS0tLQpNSUlFcGdJQkFBS0NBUUVBeFpUWU10ZVczUEpnOGd4K2pUd0ZqNm56M2szQkFlM0QyL1c0WEVuT1dmZ0xWbWhtClR2dDNTYUdSTFl0T3pIdWFJNU04V1FpdGplZWNOZnlPaUhjUGpBdGlRVUptL214bUM5dGpiVjZicmRkYzYxSXoKTnlzczRtRVI2a3pXeUlCeXd3MW9zUUpMVDU4eTNqbjg3VU1qU2t5M0dETXlqc2E1eWE0LzEyZkcwblZ4QXFZdwp3OTZSMHhuS25uUWU0bU5NbmtiZG0xOUVlOVRwZGtSNFdQajFqdkthTE1TTno4aE1WSmluamY5UEl0ZnZMblNsCmc3ZHAvODBwUXl5U254QzBTSHR1VGlQL1Ezci9VMmIxY3VabW9CZzY0QnNWVUhrNkZrMGl6TEdQc3MvTlo3b1gKak9IVVZpWllyWk9EcDZtTDJScVZBZ3FSTVlUbnc4NUV5YVNCWlFJREFRQUJBb0lCQVFEQ3NqMnBQQkNKZ0w1UApSa2llVy9zTzZtWkpOVTF2M1NBWGJEZFRtZGNoaU8rRElqVk90eldBOVJqZVRGeEYyN2EwUDY1RC9lMG4zSWR1Ckc0VklyQ3BCMGlYc01NYlZCM1EzVXVUVExWc3pIdm1OV2Q3bUNrR2NnaExwVXZhRGRTK2hUV0ZRcS9ZU2E4bncKZWl2bWtUWUJUVDlQTllRb2RXTTJmZUtqSEx3clBaaE9aTFlOdWQ5TDcxV3FQdEdXU2xRR0JUU2dwZnYrd2UrLwprWVNrRnd1MnZaYXdCa2c0ZHFCSWE1YUxVYmUvVlRmZW9EOFFlb1p3MlNKMkszNE04OElrakFsV1RYSDlYaU15CnZrYjgzYmxjRUVIVHE1L1JBWExMK1kxVkEzR0plYVVHVzB1WGJsWHVqRGFDWW0rQ2RZZ3gyNllWeUwvUnlmUDYKZ2hKSU9VS0JBb0dCQVArdC9TaU5sY1FhcCtkZ2t2Sy84cGl4NGxSZWU4QnBJZzNzaWg4a0NkNnozenlsaVIzQgpFcGNlMTFTTm8raVN2YjZGcDVLUTB1MnI2YlVJU25STWhyVkRLa1ljd0lqdE5acWx0S1ZFUjZ3OXFrVkVHeEdwClozZHprSElUclR4Kzh2MVM5dnFBVzc1U3FTNlNacDArUWZjOS9uVkZnM3ZWV05QTGlQb1VWNFk1QW9HQkFNWFUKT0Y4TUgyNHg2Tm5sWmtWWGRzeGZXSlgyamtFSGwzS3NuMlJVM3FtMXNucEpBaG14Y3BYTmE5VEdFWGRsNjdQYQo1QUxxU3NkbzVsZTFFRzhQbWppRHR2NmJ3ZWEzSHZDREFXblhTK3JwM2g2UkJaRGw1eU9ycmJ1TzZsT1FTc3hPCnF3a3ZuenFMQlNUWThKdGNQV0JZajhPaktZWmZxMTJLallPZmpEU05Bb0dCQUtTRWxoTlVGM3hhRXBRbFppamgKTGY3bTUxV1dmbGF1ejRUYUlYNHNPRldldEJSWUI4U25pWWpJQlpLWW1WRjdxckEvWERaSkRoQjB3Q3NHckxIcwowL2txd0xiZ3BWcjJGN25zeWpKVm56RExkUmFnM2pJZEtVQ0prZloxaHRFWWRzNWVaaUdHR29KNnVmWUhxaE9nCkRkNURlOHFGOGpicWJ2L0pSZGgwNG1TeEFvR0JBTU1DZHVzaXpSellNQndUS1NSem1wVE43RW92dUh6Y0dldWQKeEtXbmo3S2xmS0ZVdExCVkhvb1M3QmZiZzc0NkJ3WE5ZWFNLTmxxcHlsNXRDeDBmdVR1Nmd6b3FtaEp2TXgyTgpWbWhhSmVrVXpyTTg2OHF4Qm85QUhjdEVqekwraXUwcEl5cXorZmRBc1Rwb2E0NEtlQ293UXM5c1dIT3dmUUdCCm9ndzh5MzNGQW9HQkFNcFhXQkdJWkZpazk1amhkQ1A5aUdoZVAzMTVLRlZ6NnVycGFtMWcxMmNlT28rNjJ6MUQKVHNscWlWZW1EOU92dVJ3aHFSVldzMkc3Sk95Nys4RFhWREhZd0xTOGRpZmNnVGtaakxnTHlDeHpzQ0xBbVpyTgp0Qkx1dWtoTVlCY2hOMHRLK05Pa05zdGVCZkIzR0RKdDhlUUV2WkRkT0plUlNweTMxNHNPQWppUgotLS0tLUVORCBSU0EgUFJJVkFURSBLRVktLS0tLQo="
```

## Explanation

`/test/run-wehook.py` comprises 2 functions: `main` and `do_test`.

The script uses functions (shared with `/test/run-end-to-end.py`) defined in `/test/shared_test_code.py`

The script depends on [Python Client for Kubernetes](https://pypi.org/project/kubernetes/). This SDK is used by the script(s) to access Kubernetes cluster resources during the test. However, the shared functions use `sudo kubectl logs` through Python's `os.system` function to obtain logs.

No SDK is used for Helm. Helm commands are effected through Python's `os.system` function.

### `main`

`main` determines the location of the Helm Chart and then assembles the correct `helm install` command to install Akri. In addition to Akri's `agent` and `controller`, the script configures Helm to include the Webhook. Like the `agent` and `controller`, the Webhook is configurable using Helm's `--set` flag to override Akri's Chart's `values.yaml` settings. Specifically, the Webhook is enabled (by default it is disabled), the name, defined by the constant `WEBHOOK_NAME` is used and the CA certificate used to sign the Webhook's certificate is given to the cluster so that it may validate the Webhook's certificate.

> **NOTE** `WEBHOOK_NAME=akri-webhook-configuration` which is the default value defined in `values.yaml`. Although redundant, it is provided here to be more intentional.

```python
def get_webhook_helm_config() -> str:
    webhook = "\
    --set=webhookConfiguration.enabled=true \
    --set=webhookConfiguration.name={name} \
    --set=webhookConfiguration.caBundle={cabundle} \
    ".format(
        name=WEBHOOK_NAME,
        cabundle=CA_BUNDLE,
    )
    print("Webhook configuration:\n{}".format(webhook))
    return webhook
```

Once the Helm Chart is installed, the function calls `do_test`. Regardless of whether `do_test` succeeds, the Helm Chart is uninstalled|deleted and the script outputs any exception thrown by `do_test`.

### `do_test`

`do_test` shares some tests with `/test/run-end-to-end.py`, namely by checking whether Akri's CRDs (`Configuration`, `Instance`) were successfully created by the Helm Chart installation, and whether the deployment is in the correct state, namely whether there is an Akri Agent and an Akri Controller running. If both tests pass, the function proceeds.

The Webhook is manifest by a Deployment that produces a ReplicaSet that manages a single Pod. `do_test` effects `kubectl describe` commands for each of these resources and outputs the results to the stdout.

Then, `do_test` applies a valid Configuration to the cluster. It does this using the Kubernetes SDK. First to apply (create) the Configuration and then to get the resource. It outputs the result to stdout before deleting the Configuration.

Then, `do_test` applies an invalid Configuration to the cluster. The Configuration is syntactically correct but semantically incorrect; it is valid YAML but an invalid Configuration. Without the Webhook, the cluster will accept this Configuration. With the Webhook, the Configuration should be rejected. The test is similar to the test for a valid Configuration, except this time the function expects an API exception to be thrown by the Kubernetes API.

The Webhooks' logs are retrieved and persisted to `WEBHOOK_LOG_PATH = "/tmp/webhook_log.txt"`. When run under GitHub Actions, the workflow persists this log file.

## `subprocess` vs. `os`

Python (3.x) deprecated `os` and replaced it with `subprocess`. The Webhook script uses `subprocess` rather than `os` because `subprocess` appears to work more cleanly with GitHub Actions and correctly placing stdout and stderr after the commands as they are run. Using `os` with GitHub Actions (as is done by `/test/shared_test_code.py`) causes the stdout (and stderr) to be displayed at the beginning of the workflow output.

The Webhook Python script wraps `subprocess.run` in a function called `run`:

```python
def run(command):
    print("Executing: {}".format(command))
    result = subprocess.run(command,
                            shell=True,
                            capture_output=True,
                            text=True)
    print("returncode: {}".format(result.returncode))
    if result.stdout:
        print("stdout:")
        print(result.stdout)
    if result.stderr:
        print("stderr:")
        print(result.stderr)
```

## Generate Certificate|Key

```bash
NAMESPACE="deleteme"
kubectl create namespace ${NAMESPACE} 

WEBHOOK="akri-webhook-configuration" # Default name if not provided

# Generate CA
openssl req \
-nodes \
-new \
-x509 \
-keyout ./secrets/ca.key \
-out ./secrets/ca.crt \
-subj "/CN=CA"

# Create Secret
kubectl create secret tls ca \
--namespace=${NAMESPACE} \
--cert=./secrets/ca.crt \
--key=./secrets/ca.key

# Create Issuer using this Secret
echo "
apiVersion: cert-manager.io/v1
kind: Issuer
metadata:
  name: ca
  namespace: ${NAMESPACE}
spec:
  ca:
    secretName: ca
" | kubectl apply --filename=-

# Create Certificate using this CA
echo "
apiVersion: cert-manager.io/v1
kind: Certificate
metadata:
  name: ${WEBHOOK}
  namespace: ${NAMESPACE}
spec:
  secretName: ${WEBHOOK}
  duration: 8760h
  renewBefore: 720h
  isCA: false
  privateKey:
    algorithm: RSA
    encoding: PKCS1
    size: 2048
  usages:
    - server auth
  dnsNames:
  - ${WEBHOOK}.${NAMESPACE}.svc
  - ${WEBHOOK}.${NAMESPACE}.svc.cluster.local
  issuerRef:
    name: ca
    kind: Issuer
    group: cert-manager.io
" | kubectl apply --filename=-

# Check
kubectl get certificate/${WEBHOOK} --namespace=${NAMESPACE}

# Delete Certificate (to stop Secret being recreated)
kubectl delete certificate/${WEBHOOK} --namespace=${NAMESPACE}

# Retrieve cert-manager generated certificates and key
CRT=$(\
  kubectl get secret/${WEBHOOK} \
  --namespace=${NAMESPACE} \
  --output=jsonpath="{.data.tls\.crt}") && echo ${CRT}

KEY=$(\
  kubectl get secret/${WEBHOOK} \
  --namespace=${NAMESPACE} \
  --output=jsonpath="{.data.tls\.key}") && echo ${KEY}

CABUNDLE=$(\
  kubectl get secret/${WEBHOOK} \
  --namespace=${NAMESPACE} \
  --output=jsonpath="{.data.ca\.crt}") && echo ${CABUNDLE}
```

## Validate

> **NOTE** Certificate is bound to `akri-webhook-configuration.default.svc`

```bash
echo ${CRT} \
| base64 --decode \
| openssl x509 -in - -noout -text
```

Yields:

```console
Certificate:
    Data:
        Version: 3 (0x2)
        Serial Number:
            b4:6e:54:8d:2a:ad:ea:77:3f:30:8a:3b:00:da:7b:2b
        Signature Algorithm: sha256WithRSAEncryption
        Issuer: CN = CA
        Validity
            Not Before: Feb  9 19:06:58 2021 GMT
            Not After : Feb  9 19:06:58 2022 GMT
        Subject: 
        Subject Public Key Info:
            Public Key Algorithm: rsaEncryption
                RSA Public-Key: (2048 bit)
                Modulus:
                    00:b8:bf:13:b7:44:db:c9:f6:22:d1:c9:06:4b:43:
                    db:56:8d:b0:e5:2f:e0:95:52:6a:47:ee:1a:04:64:
                    03:66:30:54:c8:7f:1d:5a:24:b2:a7:3f:c8:4e:be:
                    8b:7f:89:58:e8:d5:8f:5b:c8:c6:3c:80:b2:b6:dc:
                    c8:81:34:c1:66:78:55:40:17:e2:2d:6c:50:73:9e:
                    c3:ce:f9:aa:14:ff:b2:06:50:20:29:17:c0:e8:7e:
                    cd:93:c2:67:34:b5:26:96:88:2c:71:30:87:d9:47:
                    f7:e3:fa:36:a8:c8:9f:f4:1e:aa:e6:01:d6:ec:77:
                    97:e3:e7:be:d1:dc:a2:c1:91:2a:12:86:ab:cd:6b:
                    88:08:2e:bb:d9:ec:09:42:16:5e:28:82:1d:fc:9e:
                    9d:cf:f9:38:e8:96:25:6e:63:ed:3b:cd:8b:51:64:
                    75:f4:d7:04:cc:37:f6:24:31:eb:b6:31:e5:00:1a:
                    e5:b2:54:88:23:fd:a6:43:d9:ba:2c:30:ff:8f:cf:
                    e1:a3:24:28:f0:2a:4c:f2:08:9f:70:83:41:f7:ec:
                    6d:01:d2:9c:4c:d3:15:ef:6c:b8:a9:55:75:95:47:
                    cd:34:5c:48:ff:8a:4f:49:3e:03:97:5d:e4:84:8f:
                    30:a7:99:aa:99:92:19:a3:41:a3:88:59:96:ec:0c:
                    7a:01
                Exponent: 65537 (0x10001)
        X509v3 extensions:
            X509v3 Extended Key Usage: 
                TLS Web Server Authentication
            X509v3 Basic Constraints: critical
                CA:FALSE
            X509v3 Authority Key Identifier: 
                keyid:54:27:0A:90:0B:01:AE:20:F5:8B:F8:85:C2:27:CF:BA:55:CB:CE:DA

            X509v3 Subject Alternative Name: critical
                DNS:akri-webhook-configuration.default.svc, DNS:akri-webhook-configuration.default.svc.cluster.local
    Signature Algorithm: sha256WithRSAEncryption
         3a:b3:c6:0c:db:da:70:96:ef:08:f2:7f:80:fa:3f:ff:7d:ab:
         78:9c:0c:df:86:bf:ee:b8:08:9c:2f:79:41:a8:a5:8e:99:62:
         10:15:55:2c:b3:79:1c:1c:89:11:7f:6a:67:ca:bc:ad:88:9b:
         33:b5:4c:32:b2:09:79:98:f3:f9:c4:6f:bc:b1:62:83:6b:16:
         70:e1:f5:df:75:84:cc:18:91:e8:f1:78:36:58:59:62:00:c7:
         63:38:46:45:fb:c8:92:8a:33:e2:ea:9c:34:07:16:b7:69:da:
         88:14:2f:53:85:13:d9:80:e5:8a:29:d5:dd:76:e0:08:87:d3:
         fd:d3:8c:3c:66:0b:75:cf:ab:35:05:f9:07:52:4f:b3:2d:25:
         65:23:43:9a:21:f9:6d:ce:3a:fd:0a:44:0d:f6:9c:7f:5f:82:
         df:ee:95:76:e4:6f:ff:b7:07:b8:51:a7:a1:3e:ce:ca:b8:7f:
         b8:75:e9:0d:23:dd:1e:8f:42:09:ef:4f:f0:cc:f4:0e:5c:0f:
         85:32:51:cf:81:ff:4e:b1:0b:3a:5b:ed:7a:75:7b:c2:0a:54:
         f9:0a:f6:d3:2c:15:0e:a7:30:b1:52:b8:85:8b:1f:4f:8a:51:
         f9:6e:90:03:87:04:3c:d9:df:46:02:da:4c:2f:23:06:6f:b1:
         9c:5e:cd:80
```
