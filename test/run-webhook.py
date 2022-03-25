#!/usr/bin/env python3

import shared_test_code
import os, subprocess

from kubernetes import client, config
from kubernetes.client.rest import ApiException

HELM_CHART_NAME = "akri"
NAMESPACE = "default"
WEBHOOK_NAME = "akri-webhook-configuration"
WEBHOOK_LOG_PATH = "/tmp/webhook_log.txt"

# Required by Webhook - renew by following the "Generate Certificate|Key" section of run-webhook.md with $NAMESPACE=default
# DNS: `akri-webhook-configuration.default.svc`
# Expires: Mar 24 20:21:16 2027 GMT
CRT = "LS0tLS1CRUdJTiBDRVJUSUZJQ0FURS0tLS0tCk1JSURTekNDQWpPZ0F3SUJBZ0lRRy9rU3NNQm94YW82cDgzZjhsM3VJekFOQmdrcWhraUc5dzBCQVFzRkFEQU4KTVFzd0NRWURWUVFEREFKRFFUQWVGdzB5TWpBek1qVXlNREl4TVRaYUZ3MHlOekF6TWpReU1ESXhNVFphTUFBdwpnZ0VpTUEwR0NTcUdTSWIzRFFFQkFRVUFBNElCRHdBd2dnRUtBb0lCQVFEUmp6UlhKM0QwY2JhQ1dpZEswbmdmClVheEgwZDliTnhQSlVCL01ZRHdtRUJyZFJ2R3lTT3pGKytsUVVjb0VVdmp4OTRpWmxNamlVZ3JUblk3UXRFSTgKUXQxY1VZcFZVOE1VTEh6RFU5VEFtUzZRVTZkQTEzT0Ivakx3ZG1TWkV4aWRnTGtGT1FYOHphUVM0RlJRUDg0agoxRzd6dnZHUHBWSlFjQm12SEJtT0twRy9Fc0JmbUZ2Y0dMdWNOTk1CMzNlNStuZ1hVZU9KMVBaeVJiMTBDbmpLCmF2dWdBRGcrMTY3Q3JrdVFpSnIyYm9FbGRlME82T1VCTFF6aXJxb2FqOXArbkY1SFZweitmRSt3ZkxnbWtPaC8KbE9ia3BVaWUrd2NTMndFQmJwSWFaZ2NZRXUzN1lOYnQyZTBhQm5tcmtKU1c1NGFwd1VYRFI4Ym9TMkF1c1pFWgpBZ01CQUFHamdiTXdnYkF3RXdZRFZSMGxCQXd3Q2dZSUt3WUJCUVVIQXdFd0RBWURWUjBUQVFIL0JBSXdBREFmCkJnTlZIU01FR0RBV2dCVFliMDV0STVVQmRhcnBmNWc5b2oxaE5XOE5vakJxQmdOVkhSRUJBZjhFWURCZWdpWmgKYTNKcExYZGxZbWh2YjJzdFkyOXVabWxuZFhKaGRHbHZiaTVrWldaaGRXeDBMbk4yWTRJMFlXdHlhUzEzWldKbwpiMjlyTFdOdmJtWnBaM1Z5WVhScGIyNHVaR1ZtWVhWc2RDNXpkbU11WTJ4MWMzUmxjaTVzYjJOaGJEQU5CZ2txCmhraUc5dzBCQVFzRkFBT0NBUUVBVzJCRHBPdlAxc3JhbVNoWlNHQnF5SlVxdWF1bWIzdzBtRFl2K0ZTT3J5OUYKYTd1Mm1USGdGMUtISDEyWnlKeWZxTEpPTkRSUWMyc2M1UHBQL1YxMStuOXhveS90cG9QM3RzMjBuNGg4MzdTMwpnNUtJQ25IaGFmM2RTdHFUSkd5aFhFYmNvakk4U0lTQmZMSjhXd0hxU1N2T2FVTWxqVWVlb21nZmZaU1RQMk93CmJ2a3VrUW1WRUVGLytlb0xkd0MvNkFiMkgxNjJTTFBaS2ZiNXJsOEpManRHVk9odmprSFJkUk1iRzJmTWtoY0sKWHFFbEwvM1V2aWMrY1hUZExqQUxIYXo0ck9vZzBBTktwYzB3djNIT1czMlp5S25hK0VCTnk4K2NNbVFnTGhxOQo0OExIK09xRDBpODRGaDVSbE82TDhqaWlHT3BKL0hxOTNnQklHOGwrU1E9PQotLS0tLUVORCBDRVJUSUZJQ0FURS0tLS0tCg=="
KEY = "LS0tLS1CRUdJTiBSU0EgUFJJVkFURSBLRVktLS0tLQpNSUlFcEFJQkFBS0NBUUVBMFk4MFZ5ZHc5SEcyZ2xvblN0SjRIMUdzUjlIZld6Y1R5VkFmekdBOEpoQWEzVWJ4CnNranN4ZnZwVUZIS0JGTDQ4ZmVJbVpUSTRsSUswNTJPMExSQ1BFTGRYRkdLVlZQREZDeDh3MVBVd0prdWtGT24KUU5kemdmNHk4SFprbVJNWW5ZQzVCVGtGL00ya0V1QlVVRC9PSTlSdTg3N3hqNlZTVUhBWnJ4d1pqaXFSdnhMQQpYNWhiM0JpN25EVFRBZDkzdWZwNEYxSGppZFQyY2tXOWRBcDR5bXI3b0FBNFB0ZXV3cTVMa0lpYTltNkJKWFh0CkR1amxBUzBNNHE2cUdvL2FmcHhlUjFhYy9ueFBzSHk0SnBEb2Y1VG01S1ZJbnZzSEV0c0JBVzZTR21ZSEdCTHQKKzJEVzdkbnRHZ1o1cTVDVWx1ZUdxY0ZGdzBmRzZFdGdMckdSR1FJREFRQUJBb0lCQUhrUFpZbER1N2s3UjlnZQpCTHp3d1h3MlRuUmZCYzFJRUNJb0szYUIwYjJiYUNtVXBtUDhST3hMRHduYmRmenhnZWNtdkw4Y2VNQmw3T003CkRobjdTSmhQZUZtd3NWMkJ1aHlaWnFuZ2IvT2ppb2JPREwwa3VoSEtxOXJHU204ejNQQ0FRR0tJQXJGOGl1QnMKdjhoc1U4WFhIeEdvcVJ2MndZcStkOWYxUDc0a1JLNjZjU0hMMjRsUDZ3WTVVVXBLcXBiNVd6ZWJGWDJGbXFJNQpCcm52SU1HUk5RSGs0UTNvUXRNaFI3RGZSUE5uUjREaWdveEZaejRNV2VGMStqRHh2czZIU25zSVhlM1hxWVF5CnVmUmFHV3Fic25DNmlOZWFxMUpPNlQ0ejJhR2ZVTTB4YjIwOWowWEZFYWRqd0pJWGRVdUgxOVlBMVozaThtUTUKdzdEK0xsRUNnWUVBMHZiRG1kK1JvMCtXclhPV2VLa3ZScmZqQ0xYL29yZ25MR0gxUjFoSFZNa0c3N0YwS1AyYQpQb0VqYm1PSVR4Q1BDanpaN2d0K0xCZ1JRY0VKcUxDVXhXMjZkMFR1T3VXMllJM3NLR0NpeU5PRzhkMWhGaEYrCmhrV3hEbGVGRmdWSEQ5clpWaFQrU0NJVmN0MUFsazJWSVZJeEp4elNFU0xVcDVSSnlNS3FBTWNDZ1lFQS9rdXUKdFZ4ZklPcFdldlJPclBwTFlabSttUTJZWVBKaWhEUFVUeVFHcjVjakNRUk9wSmJ4Ym5LdnArZlpWaUYwdDZHTApFWXg5MGg5Q2FzY1hsRHVEc3MxaXpUZkhnRVFzUWQ5dXJ5ZG9RWTZsbWtoamxxT2tJbCtwbGZkUUx1ODR3SkxlCkpLbGNPYTc4dGs1RFRqQzdmUHd4RUJMcnVCVjZjMEs0M05WQnZ4OENnWUFhcUJXVkp0dkhML0pSSG03ZjlqakUKRGM1Qk5vWURzSk02bDNJZnZyYmNycjRTb1hDVkVWNWhFWDVCbjVBRXRZbnRlRHp0U0VSOEc5cHFYWkx6M3NRZApvanpTZjBJKzdQRzdoNU5Va3NsZTZPTi9Ra0xYUUFTbHdMNmJtbEYxczlzRDFOcHJkeUdlU2JnK0dGamw1UTIzCjlTUEMxbkJ3dTk3MUFkYkU2RndFMXdLQmdRREJvUU1nMlhzZDV4RitnZlErUmorTHk3T1Rld1NpSFMzaW1FeDcKRG1XQTRrWXRJWGg0WHU3ck9LeUQzMGhnQ3cyQ25hRDA5ZE1BWWdrQ29TSlZIcFFEVzl4MWdwbUlFMkRYcjdmcAo2c201MFZKTGpmODJ2dGZGeksybW9UQU83Tng3MWRrTWRXRGlFMW9kdnE0RkpabzlheEk0dVE5L2xlc3RSSXJhCnJBOXA1UUtCZ1FDWXpiUWlVUmNlbEJMQjR6cWE0Y05UY1lOd2puU0hYWDNLZXp6WEN5Z0lhRXN4Z044bThRMmkKbVhyOGVWeWRTdEh3QmNjRWZqTW1PVytiSkx0UEJlTDgyNk42ZVlvNzlneUtlSm1qQWhkay8xYmxPMDhiRU1UYgoyN212VUxvSCtCWDFrckhFRXlGQTk3bk15NlczcFlXdE5kSnZoT25mWmRZRVFJUk9wSHJRc2c9PQotLS0tLUVORCBSU0EgUFJJVkFURSBLRVktLS0tLQo="
# Expires: Feb 27 18:09:49 2027 GMT
CA_BUNDLE = "LS0tLS1CRUdJTiBDRVJUSUZJQ0FURS0tLS0tCk1JSUMrekNDQWVPZ0F3SUJBZ0lVVUcrVXVmN1N2bXRzWmVDV2FVMmF0Rlc4bXV3d0RRWUpLb1pJaHZjTkFRRUwKQlFBd0RURUxNQWtHQTFVRUF3d0NRMEV3SGhjTk1qSXdNekkxTVRnd09UUTVXaGNOTWpjd01qSTNNVGd3T1RRNQpXakFOTVFzd0NRWURWUVFEREFKRFFUQ0NBU0l3RFFZSktvWklodmNOQVFFQkJRQURnZ0VQQURDQ0FRb0NnZ0VCCkFMUDdyWjJYcFRORkJyUHRKY2pvejlxOHFscE9OL0ZQUWJlTnE2Y3QzTGZrZkx4VnlpWFFBRllndWlXME10c1MKeUxrMHN1ZFFOT3VrNEE2aDErTitzSFhVUXNCaElDTXJDZmdkeW02b3BTcjVwK1JqSzRock9Hak5yZXQxOTBlWgp3ZXI3S0wxTVdlT0FHQUtVa3JKQVc5V1B3T3dubUZ6Z282Y2pZQmNCU0t5R3lHNVdCUHFOTEd5WFpRcG0zYTFsCnd1OVVqT1U5Vm1CVTJMUHRDc3BkNldJbGQzUjI4Y0NpbTM2SXEzUmJPTFFTWWEyVGp6NHFRU2luWXpTak5sb2wKWkJ2R0l6MVluZ3VHUDFxbHM0MFUvdkthcmxoV2ozMUNOenNCRk9WWXN2eEg1bEZDOStWU3pXVWVnZXlPS0l2RQozYXlkZmQxL0daK1N2M0FlYkt0cDBuY0NBd0VBQWFOVE1GRXdIUVlEVlIwT0JCWUVGTmh2VG0wamxRRjFxdWwvCm1EMmlQV0UxYncyaU1COEdBMVVkSXdRWU1CYUFGTmh2VG0wamxRRjFxdWwvbUQyaVBXRTFidzJpTUE4R0ExVWQKRXdFQi93UUZNQU1CQWY4d0RRWUpLb1pJaHZjTkFRRUxCUUFEZ2dFQkFJWStUb3E0VVJEZFk4UVhaVDZ4VW5YZAo1UThEcEhIbGh1UjVSN0JRQzhOZlFVWGgrQ3pBRnRpMi8vZDhjdWFiU3B5QlZmVG1yTEs0L2VDcmtZWmhhek50Ck43TlN6K1E3bzBjVm1Hbys2R2Rlb3NnOCtDWms1b3llbm1TSHh6NHllWlcyNXFYVXd4dCtZcjJQZlRSV0x2MmsKNkFDV3ZNTUJzUlNzSlNKUHpVRVBnb2xGYkdJeEMweENKWk1kYTliRlo0MHErVEZ5ZmI1V1dQdzNaeHRNdHN0NgpiQTFUTXRBQ3Q5MmthVk42SVh1TWc4NkhYQzNXQTNEWFZiTWxwZ0FORk1pQ05CSDFFZzZVSWlDTjhRc3JYd0hpClR1aEU1VGNEVStHVHlSNVk0ZHdKajN2UHJpS1NzN1F6Y0wvVEd6TFdZeU1UR1JiQlBya00yUzlmWWYvcit0RT0KLS0tLS1FTkQgQ0VSVElGSUNBVEUtLS0tLQo="
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
        "discoveryHandler": {
            "name": "debugEcho",
            "discoveryDetails": "{\"descriptions\": [\"foo\",\"bar\"]}"
        },
        "brokerSpec": {
            "brokerPodSpec": {
                "containers": [{
                    "name": "test-broker",
                    "image": "nginx:stable-alpine",
                    "imagePullPolicy": "Always",
                }],
            }
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

    k8s_distro_arg = shared_test_code.get_k8s_distro_arg()
    print("Providing Akri Helm chart with K8s distro arg: {}".format(k8s_distro_arg))

    extra_helm_args = shared_test_code.get_extra_helm_args()
    print("Providing Akri Helm chart with extra helm args: {}".format(
        extra_helm_args))

    helm_install_command = "\
    helm install {chart_name} {location} \
    --namespace={namespace} \
    --set=agent.full=true \
    --set=agent.allowDebugEcho=true \
    {webhook_config} \
    {k8s_distro_arg} \
    {helm_args} \
    --debug\
    ".format(chart_name=HELM_CHART_NAME,
             location=helm_chart_location,
             namespace=NAMESPACE,
             webhook_config=get_webhook_helm_config(),
             k8s_distro_arg=k8s_distro_arg,
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
        run("sudo {kubectl} get pods,services,akric,akrii --show-labels".
            format(kubectl=kubectl_cmd))
        return False

    # Enumerate Webhook resources
    print("Debugging:")

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

    # Apply Valid Akri Configuration
    print("Applying Valid Akri Configuration")

    # Use the template and place resources in the correct location
    body = TEMPLATE
    body["spec"]["brokerSpec"]["brokerPodSpec"]["containers"][0]["resources"] = RESOURCES

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
        body["spec"]["brokerSpec"]["brokerPodSpec"]["resources"] = RESOURCES

        api.create_namespaced_custom_object(group=GROUP,
                                            version=VERSION,
                                            namespace=NAMESPACE,
                                            plural="configurations",
                                            body=body)
    except ApiException as e:
        print(
            "As expected, Invalid Akri Configuration generates API Exception")
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

    # Save Webhook logs
    run("{kubectl} logs deployment/{service} --namespace={namespace} >> {file}"
        .format(kubectl=kubectl_cmd,
                service=WEBHOOK_NAME,
                namespace=NAMESPACE,
                file=WEBHOOK_LOG_PATH))

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
    if result.stdout:
        print("stdout:")
        print(result.stdout)
    if result.stderr:
        print("stderr:")
        print(result.stderr)


if __name__ == "__main__":
    main()
