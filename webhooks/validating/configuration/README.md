# Akri Admission Controller (Webhook) for validating Akri Configurations

This Admission Controller (Webhook) validates Akri Configuration files.

The HTTP service that implements the Webhook must be configured to use TLS. The Webhook expects its TLS certificate and private key to be stored within a Kubernetes [Secret](https://kubernetes.io/docs/concepts/configuration/secret/#tls-secrets).

It is recommended to use [`cert-manager`](https://cert-manager.io) in Kubernetes. `cert-manager` makes it easy to generate TLS certificates and private keys and, because it's a Kubernetes-native app, `cert-manager` stores these in Kubernetes Secrets. You may use a self-signed (!) CA with `cert-manager` and certificates signed by this CA will work with the Webhook.

If you wish to install the Webhook, before installing the Helm Chart for Akri, you will need to have PEM-encoded versions of CA certificate, Webhook certificate and private key. The Webhook handler expects a Secret, with the same name (!), containing its certificate and private key, to exist in the Namespace where it will be deployed.

If you're using `cert-manager` and have an `Issuer` called `ca`, you may generate a Secret for a Webhook called `${WEBHOOK}` in Namespace `${NAMESPACE}` with the following commands:

```bash
WEBHOOK="akri-webhook-configuration" # Default name if not provided
NAMESPACE="default"

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
```

> **NOTE** You must provide the above with a `${NAMESPACE}` even if the value is `default` so that it may construct qualified DNS for the Webhook Service.

When Kubernetes is configured to use the Webhook, it requires the base64-encoded PEM certificate of the CA. The CA certificate may be obtained from the Webhook's certificate using:

```bash
CABUNDLE=$(\
  kubectl get secret/${WEBHOOK} \
  --namespace=${NAMESPACE} \
  --output=jsonpath="{.data.ca\.crt}") && echo ${CABUNDLE}
```

Now you may proceed to install the Helm Chart for Akri, enabling the Webhook and providing the `CABUNDLE`:

```bash
WEBHOOK=...
NAMESPACE=...
CABUNDLE=...

helm install webhook akri-helm-charts/akri \
--namespace=${DEFAULT} \
--set=webhookConfiguration.enabled=true \
--set=webhookConfiguration.name=${WEBHOOK} \
--set=webhookConfiguration.caBundle=${CABUNDLE} \
--set=webhookConfiguration.image.repository=ghcr.io/project-akri/akri/webhook-configuration \
--set=webhookConfiguration.image.tag=v1
```
