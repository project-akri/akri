#!/usr/bin/env python3

import shared_test_code
import os, subprocess

from kubernetes import client, config
from kubernetes.client.rest import ApiException

HELM_CHART_NAME = "akri"
NAMESPACE = "default"
WEBHOOK_NAME = "akri-webhook-configuration"

# Required by Webhook
# DNS: `akri-webhook-configuration.default.svc`
# Expires: 09-Feb-2022
CRT = "LS0tLS1CRUdJTiBDRVJUSUZJQ0FURS0tLS0tCk1JSURURENDQWpTZ0F3SUJBZ0lSQUxSdVZJMHFyZXAzUHpDS093RGFleXN3RFFZSktvWklodmNOQVFFTEJRQXcKRFRFTE1Ba0dBMVVFQXd3Q1EwRXdIaGNOTWpFd01qQTVNVGt3TmpVNFdoY05Nakl3TWpBNU1Ua3dOalU0V2pBQQpNSUlCSWpBTkJna3Foa2lHOXcwQkFRRUZBQU9DQVE4QU1JSUJDZ0tDQVFFQXVMOFR0MFRieWZZaTBja0dTMFBiClZvMnc1Uy9nbFZKcVIrNGFCR1FEWmpCVXlIOGRXaVN5cHovSVRyNkxmNGxZNk5XUFc4akdQSUN5dHR6SWdUVEIKWm5oVlFCZmlMV3hRYzU3RHp2bXFGUCt5QmxBZ0tSZkE2SDdOazhKbk5MVW1sb2dzY1RDSDJVZjM0L28ycU1pZgo5QjZxNWdIVzdIZVg0K2UrMGR5aXdaRXFFb2Fyeld1SUNDNjcyZXdKUWhaZUtJSWQvSjZkei9rNDZKWWxibVB0Ck84MkxVV1IxOU5jRXpEZjJKREhydGpIbEFCcmxzbFNJSS8ybVE5bTZMREQvajgvaG95UW84Q3BNOGdpZmNJTkIKOSt4dEFkS2NUTk1WNzJ5NHFWVjFsVWZOTkZ4SS80cFBTVDREbDEza2hJOHdwNW1xbVpJWm8wR2ppRm1XN0F4NgpBUUlEQVFBQm80R3pNSUd3TUJNR0ExVWRKUVFNTUFvR0NDc0dBUVVGQndNQk1Bd0dBMVVkRXdFQi93UUNNQUF3Ckh3WURWUjBqQkJnd0ZvQVVWQ2NLa0FzQnJpRDFpL2lGd2lmUHVsWEx6dG93YWdZRFZSMFJBUUgvQkdBd1hvSW0KWVd0eWFTMTNaV0pvYjI5ckxXTnZibVpwWjNWeVlYUnBiMjR1WkdWbVlYVnNkQzV6ZG1PQ05HRnJjbWt0ZDJWaQphRzl2YXkxamIyNW1hV2QxY21GMGFXOXVMbVJsWm1GMWJIUXVjM1pqTG1Oc2RYTjBaWEl1Ykc5allXd3dEUVlKCktvWklodmNOQVFFTEJRQURnZ0VCQURxenhnemIybkNXN3dqeWY0RDZQLzk5cTNpY0ROK0d2KzY0Q0p3dmVVR28KcFk2WlloQVZWU3l6ZVJ3Y2lSRi9hbWZLdksySW16TzFUREt5Q1htWTgvbkViN3l4WW9OckZuRGg5ZDkxaE13WQprZWp4ZURaWVdXSUF4Mk00UmtYN3lKS0tNK0xxbkRRSEZyZHAyb2dVTDFPRkU5bUE1WW9wMWQxMjRBaUgwLzNUCmpEeG1DM1hQcXpVRitRZFNUN010SldValE1b2grVzNPT3YwS1JBMzJuSDlmZ3QvdWxYYmtiLyszQjdoUnA2RSsKenNxNGY3aDE2UTBqM1I2UFFnbnZUL0RNOUE1Y0Q0VXlVYytCLzA2eEN6cGI3WHAxZThJS1ZQa0s5dE1zRlE2bgpNTEZTdUlXTEgwK0tVZmx1a0FPSEJEelozMFlDMmt3dkl3WnZzWnhlellBPQotLS0tLUVORCBDRVJUSUZJQ0FURS0tLS0tCg=="
KEY = "LS0tLS1CRUdJTiBSU0EgUFJJVkFURSBLRVktLS0tLQpNSUlFb3dJQkFBS0NBUUVBdUw4VHQwVGJ5ZllpMGNrR1MwUGJWbzJ3NVMvZ2xWSnFSKzRhQkdRRFpqQlV5SDhkCldpU3lwei9JVHI2TGY0bFk2TldQVzhqR1BJQ3l0dHpJZ1RUQlpuaFZRQmZpTFd4UWM1N0R6dm1xRlAreUJsQWcKS1JmQTZIN05rOEpuTkxVbWxvZ3NjVENIMlVmMzQvbzJxTWlmOUI2cTVnSFc3SGVYNCtlKzBkeWl3WkVxRW9hcgp6V3VJQ0M2NzJld0pRaFplS0lJZC9KNmR6L2s0NkpZbGJtUHRPODJMVVdSMTlOY0V6RGYySkRIcnRqSGxBQnJsCnNsU0lJLzJtUTltNkxERC9qOC9ob3lRbzhDcE04Z2lmY0lOQjkreHRBZEtjVE5NVjcyeTRxVlYxbFVmTk5GeEkKLzRwUFNUNERsMTNraEk4d3A1bXFtWklabzBHamlGbVc3QXg2QVFJREFRQUJBb0lCQVFDZlJNTkhoUXFDTXpyVApacDJSZDE5NVg4KzMxYTJrclpkSWlhRk9WYmFFZTNnc0hVSDl1NU4xRWt5cWJpU3UvNFp4dStMS091MkRyV1BrCnQ3UDNoN2FQazMvVE1JUGhxdlkwcHhPaHRLVUhVMlJ6Z3RJbSt2NW9zU0NqbUw0R3Q0RWIxeXVSTFVpQWJrWHIKK1lMendYbjhLQkFuR0VEa1BUbnAxWmt4TFNmMi9LakZXcDV1eklqME1BbGFGdWczcy9nVm5tVVYxMTQrZEx0RgoyWGYrTGVnY1ZQUTlkbm5iQ0hsdXAvSEU5VlJaS25vS1RQR0wvNTdUUnJSaFFSRU93dVZ0NjhWRm1wOWdidzlaClQ1MndUc1N0UkRKblFYVVBQY2NyN0ppbng3TDVYSVYySFZ6UlpWcVRreVloRTJORkE2UGl1dHBJV2ZhenJZOUQKUHlpbXR2SzlBb0dCQVBDc051djNNTkladjh4bEE4SjJCdmFUU0llb0wyYUdOZy9tNUthTW5tOVJhK0NNYmx5bAoxbi9uZDFwQ045RTlQRERHdkp3LzF4dk9aN2ZQeGlNQmd6ZW9kWEk5Z3c2d3VJeGx2bW9HUDJxcW9mUmxNbSt6CitNMVYzbmZIVVl2Y0JFMlpGbWh0NVRqRU4yMmFnOEtPQ0FYZ2lmc1dsV3p4V1U4V1hKOXJIRHNMQW9HQkFNU0QKRXFoN1hSMjNaMXRsRlc1Ly9mT3ovUmNHTm5wTWRvVEZlemt2MVRqc3RjNWZYT0FVUHhGMXVNT0VLNHlsOVY0bQpEOEtKRGNkZ2sxclhTQ3dGakF2YlZOdEdjQ0dJay82S1NyWXJmRFVZTmM1MjFBa0VIZlpLUVJVYlRlZjg3c1hwCmVhZGtoUE1IT09acHN3RmxnTUs1aTl2cmF1a00yb21naXZZZ2RlYWpBb0dBS3MwRnEyczNoSFhOMVVTMXFYU2kKQW1IcENTOFExdlBSVTN5bGR6VVV6QWszM1NROFVEK3g2T2M2STVRWkp4M3p3VnptbUFjR2MweCt4NEtzNHZiVwo1aVFRVnZPM2hmcEpwN1pFYWNpWXFKaVYyc2ZRYzJzWE9UVW5MamdGT1pFME5yU2Q5bzVzc0c2OHlNSXM0b0d0CnpaWEVGQ0pOQ3FYVlV5cFA2STM4NUVjQ2dZQjlZUHVJajUwcmxwYlZVenRIVTFaZUpScDNsRGt4OHBNenh5UUYKcXFVcU9xME16UDllNE13VWdiMnUwU2RRQjVyenhPa05QNUNSQXVkQmNGWFY4SHdZSElxWmxPbDZHOEFCQ1k3OQpoK1Vwb3hiQmNrTjZ0U3ZBdGtPc0NjMjlGRDNyL0Rqb09sUXhFd3lVeGgrMTVtTXUybCtIb3o2RkR2Um9Gd3hTCldRZWdiUUtCZ0NHUmhBcUtORXJUQm5uNXlPWUFnVkJtdkxjQk4xV3NEOVBGSDJXT3VJbUZWamtscTJyeFNBbW0KbTExWU9Yam1FWU9LTE1taEhwRVQ3MFJTeXd5UDcxa05ucjFjVVk5U2FXQmFxMS9sc2MxQkh4b05rYjJCNHVqVwo0L0ZUZ3JuUGlvRkNHT2IvaElCWXNRblh1VXp1ejJwKzBKdGJPblZodktGWDdJejV6Rm55Ci0tLS0tRU5EIFJTQSBQUklWQVRFIEtFWS0tLS0tCg=="
CA_BUNDLE = "LS0tLS1CRUdJTiBDRVJUSUZJQ0FURS0tLS0tCk1JSUMrekNDQWVPZ0F3SUJBZ0lVQmFHTGpXNFB6eE84S0RzS2dvRkRGbnk0WjZBd0RRWUpLb1pJaHZjTkFRRUwKQlFBd0RURUxNQWtHQTFVRUF3d0NRMEV3SGhjTk1qRXdNakEwTWpFd01qUXpXaGNOTWpFd016QTJNakV3TWpRegpXakFOTVFzd0NRWURWUVFEREFKRFFUQ0NBU0l3RFFZSktvWklodmNOQVFFQkJRQURnZ0VQQURDQ0FRb0NnZ0VCCkFMWFpBNVkxc3FIU1pKS1RpTWh0aHJCbno2YzdzeTk1eFBHZzgwYm1rVXNlb3FmazU3dnJuRTE1NXZOOXlqVGkKYmFHRVZzR1prdjFKdnpaWFhST3hpOUNESlVOSXN5ZW5rdjdSbklIV3BCekZYTktwMlZIS1hGNERwV1BiOFJCcApndHNHaU9rYS83cHVYV3hqem5NRStiOUtvVVRYb1o4ek5XQURQSE9rNGFuUk11QmliUTNoNWdQbDArdWJRY0pQCnMvYUdVc21XdWNOOXlIV0kzYXAzY1NCeFloZUZQallDVnVMeitrMFVXaEFkQnlIWjVxaHNINWEvSUgzdGIwaWoKZ0RBM3FvWTVJZ3l0TEpzOXNiblpsTVBURW44SzFtbk9uOENqNlNaQ2ppTHZYMml5WkRKSjY0UEpYSHpUd2NzWAo5YTloUDl4aURpNjBFcDk3dDRuSHlla0NBd0VBQWFOVE1GRXdIUVlEVlIwT0JCWUVGRlFuQ3BBTEFhNGc5WXY0CmhjSW56N3BWeTg3YU1COEdBMVVkSXdRWU1CYUFGRlFuQ3BBTEFhNGc5WXY0aGNJbno3cFZ5ODdhTUE4R0ExVWQKRXdFQi93UUZNQU1CQWY4d0RRWUpLb1pJaHZjTkFRRUxCUUFEZ2dFQkFLZUhNdjFXbkEweC8rM0dHNDBnNjZ6SQpYZlljQlRTWUszZCtRT2E0OGlZWjBENEdBSWhLNnpvYWxVcVpQSCs4U1g0Zy9GOS9OdktLano1MnJQRWNEM2FqClRkN2QralZzaVFLVVlVTnd2OFlBSllXaGZINGYzYjBCb1d5K3FOVEFLMW84ZHlBa3gyNDd4cGJOc1p2OWhkUzMKNUN6YlpXRE5LZXVpazdZcHNVMzJON25qRjVZOE4xMmhGbXNBNGlHSEZvTTAzK3QxU3Fsb1Q1NUp4YXpXTzJTdQpyUXF3dDRBM2RvTGorMlh2N0RyVjRBWGhDdzRidE82MytsUCtYd2ZocWs3ajM1SW9aVExFLzRiM1FFczcwMnl2CllpMXJ3bkNlSVF6L1AxYTNJc2UyS3R5OC9EWHNqMkhRUHpuZCt4L1ptS3ZVVFVwRzJNaXhYREtRT3pLR3Z5WT0KLS0tLS1FTkQgQ0VSVElGSUNBVEUtLS0tLQo="

SECRET = {
    "apiVersion": "v1",
    "kind": "Secret",
    "metadata": {
        "name": WEBHOOK_NAME,
        "namespace": NAMESPACE,
    },
    "type": "kubernetes.io/tls",
    "data": {
        "ca.crt": CA_BUNDLE,
        "tls.crt": CRT,
        "tls.key": KEY,
    }
}

GROUP = "akri.sh"
VERSION = "v0"
KIND = "Configuration"
NAME = "broker"

RESOURCES = {"limits": {"{{PLACEHOLDER}}": "1"}}

SERVICE = {
    "type": "ClusterIP",
    "ports": [{
        "name": "name",
        "port": 0,
        "targetPort": 0,
        "protocol": "TCP"
    }]
}

TEMPLATE = {
    "apiVersion": "{}/{}".format(GROUP, VERSION),
    "kind": KIND,
    "metadata": {
        "annotations": {
            "kubectl.kubernetes.io/last-applied-configuration": ""
        },
        "creationTimestamp": "2021-01-01T00:00:00Z",
        "generation": 1,
        "managedFields": [],
        "name": NAME,
        "uid": "00000000-0000-0000-0000-000000000000"
    },
    "spec": {
        "protocol": {
            "debugEcho": {
                "descriptions": ["foo", "bar"],
                "shared": True
            }
        },
        "brokerPodSpec": {
            "containers": [{
                "name": "test-broker",
                "image": "nginx:latest",
                "imagePullPolicy": "Always",
            }],
        },
        "instanceServiceSpec": SERVICE,
        "configurationServiceSpec": SERVICE,
        "capacity": 1
    }
}


def main():
    print("End-to-end test using validating webhook")

    # If this is a PUSH, the test needs to wait for the new containers to be
    # built/pushed.  In this case, the workflow will set /tmp/sleep_duration.txt to
    # the number of seconds to sleep.
    # If this is a MANUALLY triggerd or a PULL-REQUEST, no new containers will
    # be built/pushed, the workflows will not set /tmp/sleep_duration.txt and
    # this test will execute immediately.
    shared_test_code.initial_sleep()

    # Webhook expects TLS-containing Secret (of the same name) mounted as a volume
    kubeconfig_path = shared_test_code.get_kubeconfig_path()
    print("Loading k8s config: {}".format(kubeconfig_path))
    config.load_kube_config(config_file=kubeconfig_path)

    print("Creating Secret: {namespace}/{name}".format(namespace=NAMESPACE,
                                                       name=WEBHOOK_NAME))
    client.CoreV1Api().create_namespaced_secret(body=SECRET,
                                                namespace=NAMESPACE)

    # Update Helm and install this version's chart
    os.system("helm repo update")

    # Get version of akri to test
    test_version = shared_test_code.get_test_version()
    print("Testing version: {}".format(test_version))

    shared_test_code.major_version = "v" + test_version.split(".")[0]
    print("Testing major version: {}".format(shared_test_code.major_version))

    helm_chart_location = shared_test_code.get_helm_chart_location()
    print("Get Akri Helm chart: {}".format(helm_chart_location))

    cri_args = shared_test_code.get_cri_args()
    print("Providing Akri Helm chart with CRI args: {}".format(cri_args))

    extra_helm_args = shared_test_code.get_extra_helm_args()
    print("Providing Akri Helm chart with extra helm args: {}".format(
        extra_helm_args))

    helm_install_command = "\
    helm install {chart_name} {location} \
    --namespace={namespace} \
    --set=agent.allowDebugEcho=true \
    {webhook_config} \
    {cri_args} \
    {helm_args} \
    --debug\
    ".format(chart_name=HELM_CHART_NAME,
             location=helm_chart_location,
             namespace=NAMESPACE,
             webhook_config=get_webhook_helm_config(),
             cri_args=cri_args,
             helm_args=extra_helm_args)
    print("Helm command: {}".format(helm_install_command))
    os.system(helm_install_command)

    res = False
    try:
        res = do_test()
    except Exception as e:
        print(e)
        res = False
    finally:
        # Best effort cleanup work
        try:
            # Save Agent and controller logs
            shared_test_code.save_agent_and_controller_logs(
                namespace=NAMESPACE)
        finally:
            # Delete akri and check that controller and Agent pods deleted
            os.system("\
                helm delete {chart_name} \
                --namespace={namespace}\
                ".format(
                chart_name=HELM_CHART_NAME,
                namespace=NAMESPACE,
            ))
            # Delete Akri CRDs
            client.ApiextensionsV1Api().delete_custom_resource_definition(
                "{kind}.{group}".format(kind="configurations", group=GROUP),
                body=client.V1DeleteOptions())
            client.ApiextensionsV1Api().delete_custom_resource_definition(
                "{kind}.{group}".format(kind="instances", group=GROUP),
                body=client.V1DeleteOptions())
            # Delete Webhook Secret
            client.CoreV1Api().delete_namespaced_secret(name=WEBHOOK_NAME,
                                                        namespace=NAMESPACE)
            if res:
                # Only test cleanup if the test has succeeded up to now
                if not shared_test_code.check_akri_state(
                        0, 0, 0, 0, 0, 0, namespace=NAMESPACE):
                    print(
                        "Akri not running in expected state after helm delete")
                    raise RuntimeError("Scenario Failed")

    if not res:
        raise RuntimeError("Scenario Failed")


def do_test() -> bool:
    kubeconfig_path = shared_test_code.get_kubeconfig_path()
    print("Loading k8s config: {}".format(kubeconfig_path))
    config.load_kube_config(config_file=kubeconfig_path)

    # Get kubectl command
    kubectl_cmd = shared_test_code.get_kubectl_command()

    # Ensure Helm Akri installation applied CRDs and set up Agent and Controller
    print("Checking for CRDs")
    if not shared_test_code.crds_applied():
        print("CRDs not applied by helm chart")
        return False

    print("Checking for initial Akri state")

    if not shared_test_code.check_akri_state(1, 1, 0, 0, 0, 0):
        print("Akri not running in expected state")
        os.system("sudo {} get pods,services,akric,akrii --show-labels".format(
            kubectl_cmd))
        return False

    # Enumerate Webhook resources
    print("Debugging:")

    # run("sudo {kubectl} get deployment/{service} \
    #     --namespace={namespace} \
    #     --output=json".format(kubectl=kubectl_cmd,
    #                           service=WEBHOOK_NAME,
    #                           namespace=NAMESPACE))
    # run("sudo {kubectl} get service/{service} \
    #     --namespace={namespace} \
    #     --output=json".format(kubectl=kubectl_cmd,
    #                           service=WEBHOOK_NAME,
    #                           namespace=NAMESPACE))
    # run("sudo {kubectl} get validatingwebhookconfiguration/{service} \
    #     --namespace={namespace} \
    #     --output=json".format(kubectl=kubectl_cmd,
    #                           service=WEBHOOK_NAME,
    #                           namespace=NAMESPACE))

    print("POSTing to Webhook")
    run("{kubectl} run curl \
        --stdin --tty --rm \
        --image=curlimages/curl \
        -- \
            --insecure \
            --request POST \
            --header 'Content-Type: application/json' \
            https://{service}.{namespace}.svc/validate".format(
        kubectl=kubectl_cmd, service=WEBHOOK_NAME, namespace=NAMESPACE))

    print("Webhook logs")
    run("sudo {kubectl} logs deployment/{service} \
        --namespace={namespace}".format(kubectl=kubectl_cmd,
                                        service=WEBHOOK_NAME,
                                        namespace=NAMESPACE))

    print("Deployment:")
    run("sudo {kubectl} describe deployment/{service}\
        --namespace={namespace}".format(kubectl=kubectl_cmd,
                                        service=WEBHOOK_NAME,
                                        namespace=NAMESPACE))

    print("ReplicaSet:")
    run("sudo {kubectl} describe replicaset \
        --selector=app={service} \
        --namespace={namespace}".format(kubectl=kubectl_cmd,
                                        service=WEBHOOK_NAME,
                                        namespace=NAMESPACE))

    print("Pod:")
    run("sudo {kubectl} describe pod \
        --selector=app={service} \
        --namespace={namespace}".format(kubectl=kubectl_cmd,
                                        service=WEBHOOK_NAME,
                                        namespace=NAMESPACE))

    print("Node:")
    result = subprocess.run(
        "sudo {kubectl} describe node".format(kubectl=kubectl_cmd),
        shell=True,
        capture_output=True,
        text=True)
    print("stdout:")
    print(result.stdout)

    # Apply Valid Akri Configuration
    print("Applying Valid Akri Configuration")

    # Use the template and place resources in the correct location
    body = TEMPLATE
    body["spec"]["brokerPodSpec"]["containers"][0]["resources"] = RESOURCES

    api = client.CustomObjectsApi()
    api.create_namespaced_custom_object(group=GROUP,
                                        version=VERSION,
                                        namespace=NAMESPACE,
                                        plural="configurations",
                                        body=body)

    # Check
    print("Retrieving Akri Configuration")
    akri_config = api.get_namespaced_custom_object(group=GROUP,
                                                   version=VERSION,
                                                   name=NAME,
                                                   namespace=NAMESPACE,
                                                   plural="configurations")
    print(akri_config)

    # Delete
    api.delete_namespaced_custom_object(
        group=GROUP,
        version=VERSION,
        name=NAME,
        namespace=NAMESPACE,
        plural="configurations",
        body=client.V1DeleteOptions(),
    )

    # Apply Invalid Akri Configuration
    res = False
    try:
        print("Applying Invalid (!) Akri Configuration")

        # Use the template but(!) place resources in an incorrect location
        body = TEMPLATE
        body["spec"]["brokerPodSpec"]["resources"] = RESOURCES

        api.create_namespaced_custom_object(group=GROUP,
                                            version=VERSION,
                                            namespace=NAMESPACE,
                                            plural="configurations",
                                            body=body)
    except ApiException as e:
        print("As expected, Invalid Akr Configuration generates API Exception")
        print("Status Code: {} [{}]", e.status, e.reason)
        print("Response: {}".format(e.body))
        res = True
    else:
        print("Expected APIException but none was thrown. This is an error!")

        # Debugging: check the Webhook's logs
        print("Webhook logs")
        run("sudo {kubectl} logs deployment/{service} --namespace={namespace}".
            format(kubectl=kubectl_cmd,
                   service=WEBHOOK_NAME,
                   namespace=NAMESPACE))

        res = False

    print("Akri Validating Webhook test: {}".format(
        "Success" if res else "Failure"))
    return res


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


def run(command):
    print("Executing: {}".format(command))
    result = subprocess.run(command,
                            shell=True,
                            capture_output=True,
                            text=True)
    print("returncode: {}".format(result.returncode))
    print("stdout:")
    print(result.stdout)
    print("stderr:")
    print(result.stderr)


if __name__ == "__main__":
    main()
