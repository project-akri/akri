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
CRT = "LS0tLS1CRUdJTiBDRVJUSUZJQ0FURS0tLS0tCk1JSURUVENDQWpXZ0F3SUJBZ0lRR05OYkJKZlN1dVJ1a3pqRk1wTzVhakFOQmdrcWhraUc5dzBCQVFzRkFEQU4KTVFzd0NRWURWUVFEREFKRFFUQWVGdzB5TVRBek1UQXdNVEU1TURCYUZ3MHlNakF6TVRBd01URTVNREJhTUFBdwpnZ0VpTUEwR0NTcUdTSWIzRFFFQkFRVUFBNElCRHdBd2dnRUtBb0lCQVFDOHdIZDZVY3ZhcG53Mlp3eDJQQzUvCjBiaGZ5eEJQaHNEZWVueTg2ZFl2SzRWajBTQmF6aFZwdUVKaHd0em1kcHJBSTR2bXBNTEt6NmVmV29mRFBmZWkKdjRuNm5zaXFoN1oyTjk3SGVDSy85SWJOcG9seDQzMmtRZWliR3h4NFRFb1VrZFFjZ1RHQ3BsNWFLQ3oxUFFXdgpzWG1TREFuVFRmaG1TakxmU3BZTk5qQUtKUUExSFRrTFJ5MmJuTy9wOVFHc1hhNzNUejZKSGcydmZHb0VWZTFhClhWd0x3SXFmbFRPY0RUSndlR3B4UysrRW15dElIdUUxb3hzek95VVdEUVYrRGIxcnV6VjVxbDhycWY4UXlEUHUKODdYNUg5dW1GL0M4MUNEaUdVWmJiMk5UeWdEWmdNRmpLZVVGaUNvWWxnU1hYYnlHYnIwUVRRMGlxMFkrSDFPNwpBZ01CQUFHamdiVXdnYkl3RXdZRFZSMGxCQXd3Q2dZSUt3WUJCUVVIQXdFd0RBWURWUjBUQVFIL0JBSXdBREFmCkJnTlZIU01FR0RBV2dCUXBSNWZiZC9STER2WkFoSjRmK0RzNWYyNHo0ekJzQmdOVkhSRUJBZjhFWWpCZ2dpZGgKYTNKcExYZGxZbWh2YjJzdFkyOXVabWxuZFhKaGRHbHZiaTVrWld4bGRHVnRaUzV6ZG1PQ05XRnJjbWt0ZDJWaQphRzl2YXkxamIyNW1hV2QxY21GMGFXOXVMbVJsYkdWMFpXMWxMbk4yWXk1amJIVnpkR1Z5TG14dlkyRnNNQTBHCkNTcUdTSWIzRFFFQkN3VUFBNElCQVFBRXRJc2JhZEM5eVc5SHhSMFE5VlBSaDhlekdVeUo5enpoUXlMcWVuOS8KcDJqU0YwRHBFVmtzMWhVYkFnRkhlMUk4Qk5McS9nSTVGb3o3WkxjQVNNcmdjVG9wSklWVW5ldnFtbzlwZ0lxLwprREtzV3NlSDZuaTgzOS9wbzFUSDdDNU5OWU4ybHFMS2xNQU84Ym5wSElDazMyRyt6RlZBSURLT0JDTHZPR3pKCmUvT09rUjBGcTRSWGxTWTdmNHA2QkhzRVVUdG1hOTFqMHFtWFdHSnRpc0UxbEhHZDE1bmFsOGhLWE1LVGRRN0EKbFR3Z2h4RTJXSzQ3dER6ald5eXZ1NmVPUFdxdlN1RFVNZzZzRXkvK01xZW9qeXI1MFZjUWxpS0JYK05xU0J3NApsMHRpMlVsVXdpZFhUWXFIM0NieUwrOTJ2b3R0alJFUU00bXpRWmN3THVwQgotLS0tLUVORCBDRVJUSUZJQ0FURS0tLS0tCg=="
KEY = "LS0tLS1CRUdJTiBSU0EgUFJJVkFURSBLRVktLS0tLQpNSUlFcEFJQkFBS0NBUUVBdk1CM2VsSEwycVo4Tm1jTWRqd3VmOUc0WDhzUVQ0YkEzbnA4dk9uV0x5dUZZOUVnCldzNFZhYmhDWWNMYzVuYWF3Q09MNXFUQ3lzK25uMXFId3ozM29yK0orcDdJcW9lMmRqZmV4M2dpdi9TR3phYUoKY2VOOXBFSG9teHNjZUV4S0ZKSFVISUV4Z3FaZVdpZ3M5VDBGcjdGNWtnd0owMDM0WmtveTMwcVdEVFl3Q2lVQQpOUjA1QzBjdG01enY2ZlVCckYydTkwOCtpUjROcjN4cUJGWHRXbDFjQzhDS241VXpuQTB5Y0hocWNVdnZoSnNyClNCN2hOYU1iTXpzbEZnMEZmZzI5YTdzMWVhcGZLNm4vRU1nejd2TzErUi9icGhmd3ZOUWc0aGxHVzI5alU4b0EKMllEQll5bmxCWWdxR0pZRWwxMjhobTY5RUUwTklxdEdQaDlUdXdJREFRQUJBb0lCQUQ3Q2hEZVF3UWFIdXQ5Zgo3ajNXRHVRRE9KbnBiQmYxUjJYeU5rMmVOdEJpV1N6eVdSNjRUVmhrb3ZYY2xCU3hOUTFVQkcyQk5SKzRZaFRUClJqYitBTHdGa2Z4YUZZRFdOUzRqcjVpRmNwQiszdCs4VXhFaVFpRitwTGdHRUxaVEw0S2RabmkvNEZWL3VmbWkKU0NpV3pMQTVnNkd6RFFWTWRKNldaMG5sZy9VS0QrK3ZadkJNOFdZZlFGMUduRWU0VTFWWGgzVHhTL28zeVBacAp4UEdheTc2NnRNNXBEVTVxcWhEYUo3TGp0RzE2cDlBOEZLb3JJWjFDSzZxSlJPT1RkMjQ2K2M1b24wUy9WZXNWCklwbmt5RksreFRHd1R0eWdtbUFmcmhPRzdGakI5Qy9YR1lNNUVTWkRic2I0R0QzUWprdC93WVhnZUk2d2tBWUUKUUl4d1VBRUNnWUVBelFUcFMzd1YycXZpMUw4dnlMczlWZ2JQYmdNcnVqR2FENDRIYnBuQ1kwcGdaZHlySTcvTwozTHc1MStWTFVVVUhGd2R3SlZPbSs3cUJWd0xvL2JuR09pU1AwNzFHc0dVVUgvTnZIRGpXaTQ2N0U2RVlzL01QCnlINW1oSDlwYlYxYkRhcnhvbUpPU2NhOHZvenpUcy9Lak1RcXRkSW1sUUkzajZGdkYwUWdhN3NDZ1lFQTY3QUMKYldGKy9YQjZSbDBSSXUwRUJSbUNmeEpHY2RJdHpyYXJ5ejdJYjdHZmhoVmJjbEtvazNuY3hPTEFwaXQrR2hGQQpvUU56REF1RVdDNXVKT2d0em10YkVXS1U3SzV4WmNLRHhqT2U0UVMwRlNDOGNYb3prK0hJZEtEQVhlT25tNWorCmFxSDU5NFRnYUx5Umg0aTl5c25iN1M0aHdrM1F0Wkl5SitWcU9BRUNnWUF5OW95VGtnWFF0TGVQRVBOczMzWncKd3dLZkl6U2tkUjRKemRGMUljMmJadXF0aDN3WFI5L0JLUnpyMlBpdS9BeTJJY2d6enlhTUhxRjJJcWdPSWpidgpUeFZkbWdoUFl1RHN6Rk9MWFdtZmlWeGhsY01SUUZObEVGNmxneEtPK0F6aFNlUUU3SkR2Yi9LTkgzWi8yZEZNCnlwcWZWZHozWDNTMlJIZmIvYmhkYndLQmdRQ0lENEkzTnhPaXArNU85S2RSN0ZabncwUk1xM1l6ZTB5cWkxWTkKN1M2MUhHdWxjbXJxWXNHaThiVDdqSlArMmhqZ1g1bFoycTN1QkRBUTRDMEI3VytVUFBIRDVZOW4yNFRuWkJYQwp0RVpDVFA3Uk82Yk9NK2ZXdFgrTnBNZW83Q1gwYzZ4Y1Rzb0psSklncE11MjNNQUVjK2djMG9iMnJ3bVA2S2cvCjAvY3dBUUtCZ1FDNXBOSmp5V0VOVUJySDFiNmNvbHpVZ0tmU3pPZ0wzN2JHKzBUNklSWGlMR2pHeHUzK3RwVkoKeUsvN0l0dW1iTTZEM1JFSTZWcWVLNGxZVUVzbW9sNjNONXc2TFhGY29Mdi9TU0VzQ2lFV0doMXFTMFpYaDN3YwpUNkZCUUlLMUdpU2V6YjZEWkQwaEFoVHdEeEtPYVJ3WDZXY2szL0VsM3laQm5tYUFocjJGQkE9PQotLS0tLUVORCBSU0EgUFJJVkFURSBLRVktLS0tLQo="
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

# Generate CA (that is valid for 5 years)
openssl req \
-nodes \
-new \
-x509 \
-days 1800 \
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
            0c:70:fc:16:98:71:01:8c:ad:d0:05:d7:b7:98:11:e6
        Signature Algorithm: sha256WithRSAEncryption
        Issuer: CN = CA
        Validity
            Not Before: Mar 10 01:09:53 2021 GMT
            Not After : Mar 10 01:09:53 2022 GMT
        Subject:
        Subject Public Key Info:
            Public Key Algorithm: rsaEncryption
                RSA Public-Key: (2048 bit)
                Modulus:
                    00:d0:1f:8b:eb:85:65:43:a0:78:90:e2:ba:47:7d:
                    bd:76:92:76:dc:82:fd:5c:46:58:ec:2f:1c:bc:db:
                    39:93:09:2f:8c:4c:13:03:b9:18:02:8a:16:62:ed:
                    6c:ee:e2:f9:c0:90:12:dc:8a:98:92:4a:83:94:e3:
                    91:99:19:0b:69:6c:bc:66:55:5a:3c:c2:d9:28:8d:
                    dd:1a:97:3e:07:7a:25:74:bb:ee:d3:69:02:60:9f:
                    15:59:a0:f5:78:fa:b5:84:78:ab:33:71:25:47:2b:
                    8b:d6:16:28:1e:8a:04:18:27:6b:ea:0a:ce:de:4e:
                    33:cd:6e:da:a2:41:4f:c1:3e:9b:1e:06:57:f3:91:
                    85:32:fd:55:65:39:11:4b:c7:b4:86:5a:f9:c3:41:
                    dd:5b:d3:05:5e:a8:56:67:ea:76:7f:1a:9d:36:ae:
                    d8:b0:cb:a6:9f:42:06:8a:3e:29:c5:48:12:d1:e6:
                    0e:a6:b2:a7:90:60:cd:c0:fd:ef:a3:7d:62:59:00:
                    9b:0f:09:18:8f:02:42:90:44:bf:d4:d3:01:79:04:
                    77:4f:31:41:2c:b7:e3:85:7d:aa:0c:f0:3e:af:e0:
                    a5:71:8e:20:8b:3f:cd:33:81:0a:00:c5:f3:c7:1f:
                    57:68:95:ce:48:b8:0d:50:f8:58:96:68:9b:b9:78:
                    76:3f
                Exponent: 65537 (0x10001)
        X509v3 extensions:
            X509v3 Extended Key Usage:
                TLS Web Server Authentication
            X509v3 Basic Constraints: critical
                CA:FALSE
            X509v3 Authority Key Identifier:
                keyid:29:47:97:DB:77:F4:4B:0E:F6:40:84:9E:1F:F8:3B:39:7F:6E:33:E3

            X509v3 Subject Alternative Name: critical
                DNS:akri-webhook-configuration.default.svc, DNS:akri-webhook-configuration.default.svc.cluster.local
    Signature Algorithm: sha256WithRSAEncryption
         56:5f:d0:7b:e7:71:2d:ec:08:8b:b7:c0:10:8f:e7:00:c4:6c:
         0b:03:73:97:64:9b:57:2a:9b:de:59:a2:95:7f:64:26:c6:8c:
         84:75:d8:af:7d:e8:ac:7b:fa:9d:bc:f5:22:59:ac:67:f2:b1:
         3d:dc:5f:82:06:b7:10:83:29:b5:97:54:b1:1c:b3:0b:e7:b6:
         c6:34:a2:48:58:df:7a:e4:1a:87:6a:10:60:21:9c:85:19:29:
         f9:6e:d4:5c:31:3a:63:e5:57:84:b1:2b:9d:37:81:1c:a6:6d:
         7a:02:c6:6a:f1:eb:b3:7c:1f:fc:fc:4f:31:16:98:1f:d2:d7:
         5c:08:9f:ad:36:ae:d1:19:8b:04:f3:0b:8f:87:4d:45:23:10:
         97:1c:c6:ed:f6:17:18:a4:77:df:70:58:78:11:29:bb:2a:c0:
         04:2a:21:e1:fb:a2:af:8b:97:62:f1:cb:f2:23:84:04:b7:b3:
         e9:ec:24:72:ff:11:38:17:48:a7:71:25:22:c2:4c:c7:3f:37:
         81:7c:6c:f6:37:9b:ff:37:85:64:74:5b:bb:00:bc:0a:85:84:
         35:e1:c4:42:11:9c:f8:a4:df:b2:1f:bb:06:af:f3:a0:2d:87:
         83:f3:51:cb:5f:4f:74:e1:09:21:37:9f:c1:4f:5f:5c:e9:91:
         84:ee:33:a6
```
