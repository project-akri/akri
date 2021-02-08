#!/usr/bin/env python3

import shared_test_code
import json, os, random, string, subprocess, time, typing, yaml

from kubernetes.client.api.core_v1_api import CoreV1Api
from kubernetes import client, config
from kubernetes.client.rest import ApiException

HELM_CHART_NAME = "akri"
NAMESPACE = "default"
WEBHOOK_NAME = "akri-webhook-configuration"

# Required by Webhook (Expires: 08-Feb-2022)
CRT = "LS0tLS1CRUdJTiBDRVJUSUZJQ0FURS0tLS0tCk1JSURUVENDQWpXZ0F3SUJBZ0lRQ1crTzdFTmJrUjdRZVl6aU9ERkJRVEFOQmdrcWhraUc5dzBCQVFzRkFEQU4KTVFzd0NRWURWUVFEREFKRFFUQWVGdzB5TVRBeU1EZ3hOekV6TXpOYUZ3MHlNakF5TURneE56RXpNek5hTUFBdwpnZ0VpTUEwR0NTcUdTSWIzRFFFQkFRVUFBNElCRHdBd2dnRUtBb0lCQVFEQUs4Znkrc1R3TEZ5VVNnRy80eENECitxNVF4K2dvdWlRSnNvNUhLdTB4R25IaVdXV29vRmlEcmgrQ2duYkE2dmg5YzgzblZwcnlBaHlYcU5TVVdRZ1UKS1IyUUZhb3VjWTEwODZSWXZJSTByZFlCMWtJM2tCeW03am9wSkZ4K2JESFRyRkpjb0xrR2FNd1kweDFIZVpFTAp4SExTdWZGMnlQNWxhenBOVG5JSmVnaHJrUVNLT0Mrclk4eC9Ec2VkMUw4dmk2ZlVqYkdrZXZnNlUzUUNaYVE2CkZObnJBWnE3azJuSUg4OGtLZ01WNm1KQkxaNGdKVVNsSS9XK3JNaE5RaVlLNmdReEZGSTIvYnBFUnkwZmpOaGsKZHE5T2JXUlVjb2h1aUdEZEZjclJ3c1ZIdW82aFlSOG9nTUNlSmYxeFk3SGJGa1JWdWx0SzZEdnhmNmV6WUl5UgpBZ01CQUFHamdiVXdnYkl3RXdZRFZSMGxCQXd3Q2dZSUt3WUJCUVVIQXdFd0RBWURWUjBUQVFIL0JBSXdBREFmCkJnTlZIU01FR0RBV2dCUlVKd3FRQ3dHdUlQV0wrSVhDSjgrNlZjdk8yakJzQmdOVkhSRUJBZjhFWWpCZ2dpZGgKYTNKcExYZGxZbWh2YjJzdFkyOXVabWxuZFhKaGRHbHZiaTVrWld4bGRHVnRaUzV6ZG1PQ05XRnJjbWt0ZDJWaQphRzl2YXkxamIyNW1hV2QxY21GMGFXOXVMbVJsYkdWMFpXMWxMbk4yWXk1amJIVnpkR1Z5TG14dlkyRnNNQTBHCkNTcUdTSWIzRFFFQkN3VUFBNElCQVFCQnRwUjMxUkgwa2VHc3U3ZDlwL1BzMTRKNWoySWZlRFcxM3EzRXJtTnoKOFU5ODJ0N2YyNG0rMFVTaEh2REdFK0dPMFNhYURBdmZXd2hXK0dKRmd4ZzJvNldmUEYvRThBdnhVeDFZRU1vcwpJU2tkMS9Cd0lMMzN1ODRtemFVZE80NTFrM0tZeGZIQ2JpUUlIc3JDb3R2aE9qbUJIZU41ZjRtU3AxcTQxS1BBCmY1dW5takI2LzA1Rjc0dGNHWXZKbENjR0RWbGtFNzI3bk1TamhVZTlpWERsNFlhbmpCaml1YlhaZ0wwRFZyTWsKZVpjUkUvWEhZdFMwZDliQkdkaCtjMkRmdnJTNEVnWTdtQzlLWEY1dGRZck51b3VUR2lWcjVlUTlHNEZOOXdJbgplOTBzUzNHTUtOZ0lrQ1ZYK2FrY0VxVDAxUnVwY3FtTlg5andIdWdIeEpmaAotLS0tLUVORCBDRVJUSUZJQ0FURS0tLS0tCg=="
KEY = "LS0tLS1CRUdJTiBSU0EgUFJJVkFURSBLRVktLS0tLQpNSUlFcEFJQkFBS0NBUUVBd0N2SDh2ckU4Q3hjbEVvQnYrTVFnL3F1VU1mb0tMb2tDYktPUnlydE1ScHg0bGxsCnFLQllnNjRmZ29KMndPcjRmWFBONTFhYThnSWNsNmpVbEZrSUZDa2RrQldxTG5HTmRQT2tXTHlDTkszV0FkWkMKTjVBY3B1NDZLU1JjZm13eDA2eFNYS0M1Qm1qTUdOTWRSM21SQzhSeTBybnhkc2orWldzNlRVNXlDWG9JYTVFRQppamd2cTJQTWZ3N0huZFMvTDR1bjFJMnhwSHI0T2xOMEFtV2tPaFRaNndHYXU1TnB5Qi9QSkNvREZlcGlRUzJlCklDVkVwU1AxdnF6SVRVSW1DdW9FTVJSU052MjZSRWN0SDR6WVpIYXZUbTFrVkhLSWJvaGczUlhLMGNMRlI3cU8Kb1dFZktJREFuaVg5Y1dPeDJ4WkVWYnBiU3VnNzhYK25zMkNNa1FJREFRQUJBb0lCQUZDeUVjUjJpVHhSWkk3ZwpoTnVPL2VCdDQ4VUlMUFR0TlRUZFJlR2NwUDE1blZqdk1VRWVGQTAza1FPOHhTRTlpaHNrQmRLZkMzR1VjVzA5CitBWlRYSkVhc3M5T1NhZzNCcStWbisyak93bmo5WG5QL3Y1V0JiSVRWMWp2YStlcWgwSGJtcnBLdzJkdG1rYlMKWC9ramswVGR1Vm5EdXlHbVJTMVJXYW9jeHNZelBQZG0relIvekFyUUxpSGxla1RoYXBCL0FCOWhsS2JvSHdiUgpwVGNybEEvSHN4T2tZNyswUzR4YWFUZXFQb0YxRDFubVI0R01kWG10eC9wdTJ6Y3ByaldRQ2JxdTR2UjJHR1JzCnFwQi9iR2puK0hiVFBNbXdEbTNnVWFOdFpmaFZqWEFGZVFrZS8wSmJIWHpGRzhyNTRUOVBubnBlL2xRcnY5aXkKSC8rWTFnRUNnWUVBeHcrUkEwc2QrVTVGYm4vL0ErcFJ0Qm83WHpWOUJVYzVyZHZxdkl2YTBFTG5PSnZZSVdyeApORDAwWkZ5MUg5VzljZHdYM21aYkNpWW1RQVNHZHRBN2FaRjVaNUF5bzRvOTFlN3BvMmlJOS9UbXRkemV3MmdYCjZZdTFXVTVLZFdWSWNUZHU3SXhZT0xRWFFIclRtTDI4cC9sZldrTlFOb2hRSU5lemVlbFg1ek1DZ1lFQTl5T3oKN0wrK2hncXlyTFkvSVY5UFp1WGs5MThNbmY4NFRobDl2VldrVXRGcHU5aDRJOWdtUlp4dFROZnlBSnlSL2ZMZgoyMVl1VktKaVR2UDcyTi9QQldsaHZCVUxGTDc0N1FSWjA1Q0ZzUWN2MEJQNmdJRmVrRkc0UGt1SFlHSm1DR2NLClc3SDd1WDVKcHAwTDZla0lRN0doek02UmdTaWFUUkdYMnlza2JTc0NnWUVBa0NxTjg3eXJjS3RuVGFnVm9WaEsKNUEwN2dyRFNZc2c2MWRlNEllV0lDOXpvYU84MWtMNUxBbkp4UjE1OUx4azFvd2lyb0w2d29LRVFncnpFUmJoNQp6dk0wNGZSbE9Gd2VmSm9UUysyaGhUTXhBL1Y2d0RyYlZxR0FMYld0NTJmN0YwUDJhZ3NhdWJaMFFKYTBPS0x4Cnh4V3NJVXN2Qk0ra2VLUy9yME40RGxFQ2dZRUFsajlqYklScDdRM2w4SEJmMHdjaHU3bjZTSlhGa3V5TnE1VjkKeHpTZzBTUzFObGwxT0wxNUR3cjV2Q1R3NElSUGtXY3R4eWdPeEs5d092WEQzK2E0OGw1TjU4MlhUZ2FJU1hoQwpzQWVGNzhraHZVdllHSFlPS29rMEROTllPelczT0ZhRWlwaWJJL3R1QlZnbTNROHFVaFZVKzdGOTdscmo3QVROCklKYUs3NzhDZ1lCdTUvelkrTUYySzJTSDk2SzlUUFJZZjhJKzFPMWZ2bzVvb1o1KzRTdThMR0ZPdisyckk2eFAKdk1RK09XWXROaWh0bnpRd1AwV3FMci9QUDZ1ZE8xVEJpL2dnY3Jtd3ZuSE1FS1dCL0hxWmp5TmNmOHNMNVpXNApoTzg3RnBBUDQ0bnJHVmZjOUFsYmdHdzlTQkx0OVMycHZMdERjVDlBM2RsazJYRVJMQUFOQ1E9PQotLS0tLUVORCBSU0EgUFJJVkFURSBLRVktLS0tLQo="
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
             agent_and_controller=get_agent_and_controller(),
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


if __name__ == "__main__":
    main()
