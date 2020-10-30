# End-to-End Demo deployed to [DigitalOcean](https://digitalocean.com)

This guide complements [End-to-End Demo](./end-to-end-demo.md) by providing instructions for deploying the demo to [DigitalOcean](https://digitalocean.com). This guide is comprehensive and should get you to a droplet running the End-to-End Demo that you may explore.

You will need a DigitalOcean account and you will need to have established billing to pay for the droplet used here. These instructions assume you have installed and configured [doctl](https://github.com/digitalocean/doctl).

## Environment variables

The following environment variables will be used in the remainder of this guide:

```bash
INSTANCE="akri"              # Or your preferred droplet name
REGION="sfo2"                # Or your preferred region
SSHKEYID=[[YOUR-SSH-KEY-ID]] # doctl compute ssh-key list
```

> **NOTE** If you've not added public keys to your DigitalOcean account, you will need to provide the droplet password whenever you ssh in to the droplet.

## Install

> **NOTE** You will be billed while the droplet exists

The creation of the DigitalOcean droplet uses a startup script ([link](/scripts/end_to_end_microk8s_demo.sh)). The script combines all the steps described in the [End-to-End Demo](/docs/end-to-end-demo.md)

```bash
doctl compute droplet create ${INSTANCE} \
--region ${REGION} \
--image ubuntu-20-04-x64 \
--size g-2vcpu-8gb \
--ssh-keys ${SSHKEYID} \
--tag-names microk8s,akri,end-to-end-demo \
--user-data-file=./scripts/end_to_end_microk8s_demo.sh
```

 > **NOTE** Ensure `user-data-file` points to the location of the script. If you git cloned akri, the startup script is in the `./scripts` directory.

## Check

You may ssh in to the droplet and check the state of the startup script.

If you have [`jq`](https://stedolan.github.io/jq/) installed:

```bash
IP=$(\
  doctl compute droplet list \
  --output=json | jq -r ".[]|select(.name==\"${INSTANCE}\")|.networks.v4[]|select(.type==\"public\")|.ip_address") && \
echo ${IP}
```

If not:

```bash
IP=$(doctl compute droplet list | grep ${INSTANCE} | awk '{print $3}') && \
echo ${IP}
```

Then you may ssh in to the droplet:

```bash
SSHKEY=[[/path/to/you/key]]
ssh -i ${SSHKEY} root@${IP}
```

> **NOTE** Ensure that `SSHKEY` correctly points to the location of your private key.

Then, either:

```bash
sudo journalctl --unit=cloud-* --follow
```

Or:

```bash
tail -f `var/log/syslog`
```

The script is complete when the following log line appears:

```console
service/akri-video-streaming-app created
```

## Access the End-to-End Demo

To determine the NodePort of the service, you can either ssh in to the droplet and then run the command to determine the NodePort.

Or, from your host (!) machine, you may combine the steps:

```bash
COMMAND="\
  sudo microk8s.kubectl get service/akri-video-streaming-app \
  --output=jsonpath='{.spec.ports[?(@.name==\"http\")].nodePort}'"
NODEPORT=$(\
  ssh -i ${SSHKEY} root@${IP} "${COMMAND}") && \
echo ${NODEPORT}
```

The `kubectl` command gets the `akri-video-stream-app` service as JSON and filters the output to determine the NodePort (`${NODEPORT}`) that's been assigned.

The `ssh` command runs the `kubectl` commands against the droplet.

Then we can use ssh port-forwarding to forward one of our host's (!) local ports (`${HOSTPORT}`) to the Kubernetes' service's NodePort (`{NODEPORT}`):

```bash
HOSTPORT=8888

ssh -i ${SSHKEY} root@${IP} -L ${HOSTPORT}:localhost:${NODEPORT}
```

> **NOTE** `HOSTPORT` can be the same as `NODEPORT` if this is available on your host.

The port-forwarding only works while the ssh sessions is running. So, while the previous command is running in one shell, browse the demo's HTTP endpoint:

```console
http://localhost:${HOSTPORT}/
```

> **NOTE** You'll need to manually replace `${HOSTPORT}` with the value (e.g. `8888`)

> **NOTE** The terminating `/` is important

## Tidy-up

The simplest way to tidy-up is to delete the droplet:

```bash
doctl compute droplet delete ${INSTANCE}
```

You may want to double-check that the droplet has been deleted:

```bash
doctl compute droplet list
```
