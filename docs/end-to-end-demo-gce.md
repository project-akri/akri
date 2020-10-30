# End-to-End Demo deployed to [Google Compute Engine](https://cloud.google.com/compute)

This guide complements [End-to-End Demo](./end-to-end-demo.md) by providing instructions for deploying the demo to [Google Compute Engine](https://cloud.google.com/compute). This guide is comprehensive and should get you to an instance running the End-to-End Demo that you may explore.

You will need a Google account and you must have [enabled billing](https://support.google.com/googleapi/answer/6158867) in order to pay for the machine type used here.

## Environment variables

The following environment variables will be used in the remainder of this guide:

```bash
PROJECT=[[YOUR-PROJECT]]
BILLING=[[YOUR-BILLING]]
INSTANCE="akri"      # Or your preferred instance name
TYPE="e2-standard-2" # Or your preferred machine type
ZONE="us-west1-c"    # Or your preferred zone
```

You may list your billing accounts using `gcloud alpha billing accounts list`, you will need a value from the column `ACCOUNT_ID`. Alternatively, if you know you have only one billing account, you may:

```bash
BILLING=$(gcloud alpha billing accounts list --format="value(name)")
```

## Optional: Create Google Cloud Platform (GCP) project

If you wish to use an existing Google Cloud Platform (GCP) project, then you should skip this step.

```bash
gcloud projects create ${PROJECT}
gcloud beta billing projects link ${PROJECT} \
--billing-account=${BILLING}

gcloud services enable compute.googleapis.com \
--project=${PROJECT}
```

## Install

> **NOTE** You will be billed while this instance is running.

The creation of the Compute Engine instance uses a startup script ([link](/scripts/end_to_end_microk8s_demo.sh)). The script combines all the steps described in the [End-to-End Demo](/docs/end-to-end-demo.md).

```bash
gcloud compute instances create ${INSTANCE} \
--machine-type=${TYPE} \
--preemptible \
--tags=microk8s,akri,end-to-end-demo \
--image-family=ubuntu-minimal-2004-lts \
--image-project=ubuntu-os-cloud \
--zone=${ZONE} \
--project=${PROJECT} \
--metadata-from-file=startup-script=./scripts/end_to_end_microk8s_demo.sh
```

> **NOTE** Ensure the `startup-script` points to the location of the file. If you git cloned akri, the startup script is in the scripts directory.

## Optional: Check

You may ssh in to the instance and check the state of the startup script.

```bash
gcloud compute ssh ${INSTANCE} \
--zone=${ZONE} \
--project=${PROJECT}
```

Then, either:

```bash
sudo journalctl --unit=google-startup-scripts.service --follow
```

Or:

```bash
tail -f `var/log/syslog`
```

The script is complete when the following log line appears:

```console
INFO startup-script: service/akri-video-streaming-app created
```

## Access the End-to-End Demo

We need to determine the NodePort of the `akri-video-streaming-app` service now running on Kubernetes. You can either ssh in to the instance and then run the command to determine the NodePort. Or, you may combine the steps:

```bash
COMMAND="\
  sudo microk8s.kubectl get service/akri-video-streaming-app \
  --output=jsonpath='{.spec.ports[?(@.name==\"http\")].nodePort}'"
NODEPORT=$(\
  gcloud compute ssh ${INSTANCE} \
  --zone=${ZONE} \
  --project=${PROJECT} \
  --command="${COMMAND}") && echo ${NODEPORT}
```

The `kubectl` command gets the `akri-video-stream-app` service as JSON and filters the output to determine the NodePort (`${NODEPORT}`) that's been assigned.

The `gcloud compute ssh` command runs the `kubectl` commands against the instance.

Then you can use ssh port-forwarding to forward one of your host's (!) local ports (`${HOSTPORT}`) to the Kubernetes' service's NodePort (`{NODEPORT}`):

```bash
HOSTPORT=8888

gcloud compute ssh ${INSTANCE} \
--zone=${ZONE} \
--project=${PROJECT} \
--ssh-flag="-L ${HOSTPORT}:localhost:${NODEPORT}
```

> **NOTE** `HOSTPORT` can be the same as `NODEPORT` if this is available on your host.

The port-forwarding only works while the ssh sessions is running. So, while the previous command is running in one shell, browse the demo's HTTP endpoint: 

```console
http://localhost:${HOSTPORT}/
```

> **NOTE** You'll need to manually replace `${HOSTPORT}` with the value (e.g. `8888`).

> **NOTE** The terminating `/` is important.

## Tidy-Up

The simplest way to tidy-up is to delete the Compute Engine instance:

```bash
gcloud compute instances delete ${INSTANCE} \
--zone=${ZONE} \
--project=${PROJECT}
```

If you wish to delete the entire Google Cloud Platform project:

```bash
gcloud projects delete ${PROJECT}
```

> **WARNING** Both these commands are irrevocable.