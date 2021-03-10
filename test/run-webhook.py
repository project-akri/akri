#!/usr/bin/env python3

import shared_test_code
import os, subprocess

from kubernetes import client, config
from kubernetes.client.rest import ApiException

HELM_CHART_NAME = "akri"
NAMESPACE = "default"
WEBHOOK_NAME = "akri-webhook-configuration"
WEBHOOK_LOG_PATH = "/tmp/webhook_log.txt"

# Required by Webhook
# DNS: `akri-webhook-configuration.default.svc`
# Expires: 10-Mar-2022
CRT = "LS0tLS1CRUdJTiBDRVJUSUZJQ0FURS0tLS0tCk1JSURTekNDQWpPZ0F3SUJBZ0lRREhEOEZwaHhBWXl0MEFYWHQ1Z1I1akFOQmdrcWhraUc5dzBCQVFzRkFEQU4KTVFzd0NRWURWUVFEREFKRFFUQWVGdzB5TVRBek1UQXdNVEE1TlROYUZ3MHlNakF6TVRBd01UQTVOVE5hTUFBdwpnZ0VpTUEwR0NTcUdTSWIzRFFFQkFRVUFBNElCRHdBd2dnRUtBb0lCQVFEUUg0dnJoV1ZEb0hpUTRycEhmYjEyCmtuYmNndjFjUmxqc0x4eTgyem1UQ1MrTVRCTUR1UmdDaWhaaTdXenU0dm5Ba0JMY2lwaVNTb09VNDVHWkdRdHAKYkx4bVZWbzh3dGtvamQwYWx6NEhlaVYwdSs3VGFRSmdueFZab1BWNCtyV0VlS3N6Y1NWSEs0dldGaWdlaWdRWQpKMnZxQ3M3ZVRqUE5idHFpUVUvQlBwc2VCbGZ6a1lVeS9WVmxPUkZMeDdTR1d2bkRRZDFiMHdWZXFGWm42blovCkdwMDJydGl3eTZhZlFnYUtQaW5GU0JMUjVnNm1zcWVRWU0zQS9lK2pmV0paQUpzUENSaVBBa0tRUkwvVTB3RjUKQkhkUE1VRXN0K09GZmFvTThENnY0S1Z4amlDTFA4MHpnUW9BeGZQSEgxZG9sYzVJdUExUStGaVdhSnU1ZUhZLwpBZ01CQUFHamdiTXdnYkF3RXdZRFZSMGxCQXd3Q2dZSUt3WUJCUVVIQXdFd0RBWURWUjBUQVFIL0JBSXdBREFmCkJnTlZIU01FR0RBV2dCUXBSNWZiZC9STER2WkFoSjRmK0RzNWYyNHo0ekJxQmdOVkhSRUJBZjhFWURCZWdpWmgKYTNKcExYZGxZbWh2YjJzdFkyOXVabWxuZFhKaGRHbHZiaTVrWldaaGRXeDBMbk4yWTRJMFlXdHlhUzEzWldKbwpiMjlyTFdOdmJtWnBaM1Z5WVhScGIyNHVaR1ZtWVhWc2RDNXpkbU11WTJ4MWMzUmxjaTVzYjJOaGJEQU5CZ2txCmhraUc5dzBCQVFzRkFBT0NBUUVBVmwvUWUrZHhMZXdJaTdmQUVJL25BTVJzQ3dOemwyU2JWeXFiM2xtaWxYOWsKSnNhTWhIWFlyMzNvckh2Nm5iejFJbG1zWi9LeFBkeGZnZ2EzRUlNcHRaZFVzUnl6QytlMnhqU2lTRmpmZXVRYQpoMm9RWUNHY2hSa3ArVzdVWERFNlkrVlhoTEVyblRlQkhLWnRlZ0xHYXZIcnMzd2YvUHhQTVJhWUg5TFhYQWlmCnJUYXUwUm1MQlBNTGo0ZE5SU01RbHh6RzdmWVhHS1IzMzNCWWVCRXB1eXJBQkNvaDRmdWlyNHVYWXZITDhpT0UKQkxlejZld2tjdjhST0JkSXAzRWxJc0pNeHo4M2dYeHM5amViL3plRlpIUmJ1d0M4Q29XRU5lSEVRaEdjK0tUZgpzaCs3QnEvem9DMkhnL05SeTE5UGRPRUpJVGVmd1U5ZlhPbVJoTzR6cGc9PQotLS0tLUVORCBDRVJUSUZJQ0FURS0tLS0tCg=="
KEY = "LS0tLS1CRUdJTiBSU0EgUFJJVkFURSBLRVktLS0tLQpNSUlFcEFJQkFBS0NBUUVBMEIrTDY0VmxRNkI0a09LNlIzMjlkcEoyM0lMOVhFWlk3Qzhjdk5zNWt3a3ZqRXdUCkE3a1lBb29XWXUxczd1TDV3SkFTM0lxWWtrcURsT09SbVJrTGFXeThabFZhUE1MWktJM2RHcGMrQjNvbGRMdnUKMDJrQ1lKOFZXYUQxZVBxMWhIaXJNM0VsUnl1TDFoWW9Ib29FR0NkcjZnck8zazR6elc3YW9rRlB3VDZiSGdaWAo4NUdGTXYxVlpUa1JTOGUwaGxyNXcwSGRXOU1GWHFoV1orcDJmeHFkTnE3WXNNdW1uMElHaWo0cHhVZ1MwZVlPCnByS25rR0ROd1Azdm8zMWlXUUNiRHdrWWp3SkNrRVMvMU5NQmVRUjNUekZCTExmamhYMnFEUEErcitDbGNZNGcKaXovTk00RUtBTVh6eHg5WGFKWE9TTGdOVVBoWWxtaWJ1WGgyUHdJREFRQUJBb0lCQUJaSHlrcmtkUHJRYXhmWApyZW1KWkljVkZ2UjBjWHMzYkwyY0xZOXFTTGVjL0NJZzRzZzdRSDdGR2JCdGlvUG9lS1JNeURnai9rRnJDTHNmCndhNktKOWFaZFhIZklWSHY2aCtWVUY1UVlxdWFQL2hIUmtJTHM5MTBLbXoxOWxHRlJYbHhFYUxvTWYxMGcvdmYKVTF3eG1rNmJxY25jYmxrT05pMS8rSmYyTmZ1UjdwejR5SVVlMkhxQkZnV2kxRTB2cVh1S3lVdFovbWVkWCtNTgp3RkR2YW1nYXR0dHJFYlBkSk15ZXFtZDRlbjJuR21IYnVsYzl2Y095YWNnWFJxbE9mM0FUNURMUit3S1AvVm5yClI5L1R0c0F0bFZyVU1Db3duWU42bHR5ZFVyTkQzeGNYckxXNkxLUkN1MXo3U1hWY3J4TW43VXo0cXdJVzFOcGQKMVV0TlVJRUNnWUVBKytKTFdtdEJheTVSQ09PRVNlVmRqdWNQRTkxamNhdmowUWZKS2MwcWdHd0toNS9UVGF2VwpLWHJXa1NueURiTVJxNmNSRUNrTUk4TUlHS2F6THFjaTBSWDk5NTAva2RwZlB3VXl5RTBnKzMrQ1BwRXNKbmJqCk9XU3ExNC84NFpsR3Z0dGVxZVJoNmNKdGlCbGV1SG5QVUttdHlDQU9Pdk12UGJYbkFtOUVPL2tDZ1lFQTA0WXcKSXE1SW1ZZjVET3NOY25TU3hTMU1UaXo1TmpYSDBNak05QVhqYzV4YzVKNFZ2SzZZN2xZMWZlak5wTUFoRjd2NgpOR0tBRjc2K3JJdk4zVWQ2RU9BNXB2Nk5MbUtBa29HMHlMWkU1eTdFM2FjeDhzZFJCczNyUWJ1T3JvSVM5SDFyCktrU0pKbGhKaGdCRHJSNzN0Wm9wcjVPVERkQnYwaVlzWUFrRG9mY0NnWUVBOVdaeGw3UXJWaXNYMUJzbHhZRHIKZDlCeGhoOEpSYlA4RHFrUk9lS2paOTdiRzJ1QlNJa0Q3QUc3ams1WmZ6TlpJZTF3MkZmRmRnb0xsMGpDQmMvYwpRZXkxTkV0RnBlb2xKWmNBOU5rQUswYjlNOHZvUWNsT2M1bzZRQzRPYUJVWE1kYzBFVDFxajM1WGpHTjdQeXVkCjZhNkdteFZ3QjhycDJhdWhWMlBrRExFQ2dZQWtnQnBjVWJETGRaQS9iMnd4blBZYXVsZFpnaDg3QUlyTGQyc08Kak5tVUFKNXpBT2lGVjZlaU1SUW45djFOZWEzOE4zN1VmVTdYU2g0RERsam0zMGVzRTlVL0FOd0I3aE43dEpBcQp0bkVyWjRHbk1ndkhkaWVBUWhaZmtHcnRxQnAzUUJFM0NQNlZ0RlJ6b0NZTmdMT0VEZWMxbWdTZE5LT25JdGt6CmRUckFQd0tCZ1FDa1l2SHhZbXp2a0pBaExNemFBSitTRUtxZWp1REtTakxTMTVHUGdZRUlTMTZwRmM5YjE4S24Kd2Q2N2MwOU1LVG9KNUZ6b2ZtY0xlTXpFVWRxYXkyM3RFWmZqODhIRy9pUzVjbGhJUEQvaUhCQUVYS1VHcTNldwpNVDgyUFlKSVNtdTByZGt4SzNham1zb2lUb2ZGdGwxTVRUMHhQS3hOMEExRzhZb29acmpsUWc9PQotLS0tLUVORCBSU0EgUFJJVkFURSBLRVktLS0tLQo="
# Expires: 12-Feb-2026 
CA_BUNDLE = "LS0tLS1CRUdJTiBDRVJUSUZJQ0FURS0tLS0tCk1JSUMrekNDQWVPZ0F3SUJBZ0lVSFl0WnJiSUdna0VSdkw2NlhEb1F1aSs3cUFnd0RRWUpLb1pJaHZjTkFRRUwKQlFBd0RURUxNQWtHQTFVRUF3d0NRMEV3SGhjTk1qRXdNekV3TURBMU9ETXhXaGNOTWpZd01qRXlNREExT0RNeApXakFOTVFzd0NRWURWUVFEREFKRFFUQ0NBU0l3RFFZSktvWklodmNOQVFFQkJRQURnZ0VQQURDQ0FRb0NnZ0VCCkFMdVY2UmNjbUZma0lpbTB2R25VcC85OEFIU3pTbzBGK0lDMnNmUk5RTGtVUnFTSGZSV00ydGF1b1hqRDZtMUYKR2hsL1MrWUZGQXZzT1dITkNlUnRieHJXOXRlWEVuTmJBaDRzVWlwQS92MkRXSU8vSVpETHBWdVpYYlJkRVVsTgptQ1FJSmJ5algwdSt6K2l6Mkx1TVM4MTdDZHhSZWdEZ3B2UDlvUnBQUlJvTi93anZmdVhwRXlZZDBsQWlZbERpCm5neFZPKzdlblpsd0p5UTFKN3hoSC9YQWpVa0hiN09tdU1Bc05TTE5Zdmp3REpTZmU1Wm9XYW1lbHl3S0JTcHcKbDNSZytIcXlXSGlLTHF1NzZ0YzE2R0F4NGtHYkdURkljNmV4NTNHQVI3OXFwVnc2dTUyM1ptSDRXZVFKR2tJTwpFTmQwK2JRUEdLOEZTMHVhYXdXUXFmTUNBd0VBQWFOVE1GRXdIUVlEVlIwT0JCWUVGQ2xIbDl0MzlFc085a0NFCm5oLzRPemwvYmpQak1COEdBMVVkSXdRWU1CYUFGQ2xIbDl0MzlFc085a0NFbmgvNE96bC9ialBqTUE4R0ExVWQKRXdFQi93UUZNQU1CQWY4d0RRWUpLb1pJaHZjTkFRRUxCUUFEZ2dFQkFDRkw5Y1E0TTczTlEwV2w2UjdJQ1g0TAo0UGZDTW05aEZrT2R5SGFOeTEvN00vSTNjSVVwMjY5eGtpS2dMZUd3MmNJTDJXNEZXeU5zWmtONG16WlpadXlkCjBScldldWlkR1duQUFSem9yOFgvWWJqUHIwcEtkSXM1bys0VmpCWTBZS05vZXRkMGd4NXA1eG9CeHRScm56dTkKejY1MzJER1NKQUxjQXNTa2tadXBPaE5DaVFqVDdOZGZRdmtMaHQyZW0yNi8zaGVDV3hlYW9tQWlLb2dQYzJjSApwUmN0SE5JL1hqTmJNKzBpaTAxQ1M0OE1tRk1zOVo0TTMzdTF1QjRCQWROS0l6Q3lnK0lpZmpPZU56RHVkZVdJClN5TlZZR2ZPdDUrNnlyaG1vYVhIb2Qvd2ljSXk3SnVIdlNtazV3ZkNZdmVtYU9UUnJFWWQvZng0ZTlqZzltcz0KLS0tLS1FTkQgQ0VSVElGSUNBVEUtLS0tLQo="
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
