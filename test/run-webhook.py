#!/usr/bin/env python3

import shared_test_code
import os, subprocess

from kubernetes import client, config
from kubernetes.client.rest import ApiException

HELM_CHART_NAME = "akri"
NAMESPACE = "default"
WEBHOOK_NAME = "akri-webhook-configuration"
WEBHOOK_LOG_PATH = "/tmp/webhook_log.txt"

# Required by Webhook - renew by following the "Generate Certificate|Key" section of run-webhook.md
# DNS: `akri-webhook-configuration.default.svc`
# Expires: Mar 24 18:25:48 2027 GMT
CRT = "LS0tLS1CRUdJTiBDRVJUSUZJQ0FURS0tLS0tCk1JSURUakNDQWphZ0F3SUJBZ0lSQUlQbVJsZ3dCNUppcitDMk5heU95Z1l3RFFZSktvWklodmNOQVFFTEJRQXcKRFRFTE1Ba0dBMVVFQXd3Q1EwRXdIaGNOTWpJd016STFNVGd5TlRRNFdoY05NamN3TXpJME1UZ3lOVFE0V2pBQQpNSUlCSWpBTkJna3Foa2lHOXcwQkFRRUZBQU9DQVE4QU1JSUJDZ0tDQVFFQXlBbEp1VXB6dGtjV0Z0Tlg3b0thCmsyVkZYWGlCejU0dmk0cHM0ZEJLTXQ0aFUrOU1DSUU1NXlZTGNYT1loUDE4bEFVQ3dxTXJOWXcrZnM4VURFVEMKTjVmQ0hzVVZwZFhYWHlCSWhpeXpIU0dHTjQ3MGxCSmNXbDA3cUMwRmM1eUY5SWVrMmlRUVVqcUR5c2FzOG0xZQpSNmVLcEdDNVJGbHA5RTZvdVg4Z1V2Mnc2dHU3bnBFUFpRcnlqM094Q21GRFgvNzZSeDF4eU5wQU1EL2ZPa0ViCkV2Y2hGdVpwM01CRzZ2K1JpaEhibjRxOUhaRWJkbFRYRlFLRjBmWEgreWZENVRQYzNVWmo5dnp4b1RvZEU0aDMKckNlNHVnUkdOdFRkbC80d251a1h1S25XVzh0Rlp0WkhJeG1LWTArRWF0L25neVl2NDdsODd4QTF6WndFNzl0Mgo4d0lEQVFBQm80RzFNSUd5TUJNR0ExVWRKUVFNTUFvR0NDc0dBUVVGQndNQk1Bd0dBMVVkRXdFQi93UUNNQUF3Ckh3WURWUjBqQkJnd0ZvQVUyRzlPYlNPVkFYV3E2WCtZUGFJOVlUVnZEYUl3YkFZRFZSMFJBUUgvQkdJd1lJSW4KWVd0eWFTMTNaV0pvYjI5ckxXTnZibVpwWjNWeVlYUnBiMjR1WkdWc1pYUmxiV1V1YzNaamdqVmhhM0pwTFhkbApZbWh2YjJzdFkyOXVabWxuZFhKaGRHbHZiaTVrWld4bGRHVnRaUzV6ZG1NdVkyeDFjM1JsY2k1c2IyTmhiREFOCkJna3Foa2lHOXcwQkFRc0ZBQU9DQVFFQU9WU2VTcTNWRUdQVHY2TktualozdVRLbGxSN3k3di8yWE93Y0NTTjAKYWJrbmxqSDFWd0RGNHBBazU3eVR4K01EbFdDU1Y4WVpwZVRVbHdsb3krWWJOVnRqNzZWVXpNSmkyNjlRdko5TQo3c2haT0dseldtWXBvcHhtc0NoeVc4KzI3dUIxSkVrSksvTlNHOGV5d09NY25PQnRzTmg4Y1BmOWMyNVhuUkF4Ck9BOHJZd3oyaFFQY2hMeUYxQytoRXdQM0gxcElzWUxlT09jRnVZdTFvYXRsRFNkSktDOW9mUzVpMElRd2lVdTgKaGgrZEo4VUFkRXlXbXVVc1lCN0RWeXRJdS9yS1RxUzM4Nk02dkNCUWp0WWtwdGNSQ1JuSlVOZWFza1ZON0p6bAprcFcwU2VZTlByUGxobnNWQ0lrSGhKZ1RQY0NrMEZqYitkYUFwcEppdnZNS3hBPT0KLS0tLS1FTkQgQ0VSVElGSUNBVEUtLS0tLQo="
KEY = "LS0tLS1CRUdJTiBSU0EgUFJJVkFURSBLRVktLS0tLQpNSUlFb3dJQkFBS0NBUUVBeUFsSnVVcHp0a2NXRnROWDdvS2FrMlZGWFhpQno1NHZpNHBzNGRCS010NGhVKzlNCkNJRTU1eVlMY1hPWWhQMThsQVVDd3FNck5ZdytmczhVREVUQ041ZkNIc1VWcGRYWFh5QkloaXl6SFNHR040NzAKbEJKY1dsMDdxQzBGYzV5RjlJZWsyaVFRVWpxRHlzYXM4bTFlUjZlS3BHQzVSRmxwOUU2b3VYOGdVdjJ3NnR1NwpucEVQWlFyeWozT3hDbUZEWC83NlJ4MXh5TnBBTUQvZk9rRWJFdmNoRnVacDNNQkc2ditSaWhIYm40cTlIWkViCmRsVFhGUUtGMGZYSCt5ZkQ1VFBjM1Vaajl2enhvVG9kRTRoM3JDZTR1Z1JHTnRUZGwvNHdudWtYdUtuV1c4dEYKWnRaSEl4bUtZMCtFYXQvbmd5WXY0N2w4N3hBMXpad0U3OXQyOHdJREFRQUJBb0lCQURrTXhSVHVVZkFEZUI1TQpha0NneVBzT24ralhqSllzOUR4azMwYkx3ODJjSW44d3VVdVhwMjd3SDhWY2hYd3dXMDVQMjRpdFJvNkFEL2JVCmtsQXBjQWF3NW5FbUhsVnNsbjhQMHY5SlVsQVZscFRUMVpkQllVdDRXYUpPTE1iYk5pMFdYb0xFVkU5UFZ2VUgKRXA0VmFSVWdpRjczSXYrR1RMeWJqbTFRLzJRTGEwZnNEaDdNajRhVGRack8ydkFoN3pHOEtrZ3dEWUxxME91bgp4R0wxaXNqTjJLY014WXNxYlRTWUtvODg1dnlvNGlSWm5EbnpIeWdMMllrUUdRY2Q5VHg4dlkvZHJhNjVWQndyCkc3SFpXRmpKWDBMKzlPNUxqSFdWdHo3d2lsQmFlNGM1b2NyZ2MvK3J3M1FqZEc3Z0FDM2I5K0x3YjdEbEhqVDEKRFZmM21VRUNnWUVBM3FSeFFlT1BTT1VoRDlTdk9Wd3QzSVNvcSt2RjF0VVRqRlU5anB2OGNIZmphUVF2d2FQcwo5ZDRRekp3K1NXS1p0VkRSM1RjazVvV1RBNjFyK2NKWmlKaWNIT1VMZUtSQ1JWUWQ2TkF1alBnOGZENDZUOFp0CnFOV2ZCRmREZVg5Q0gwdXVOUzgyLzFLRnNQYnJPMXJRNmpnblFlTGVhM1pNdlZTelQvQk9WVk1DZ1lFQTVnSEgKUzY5aURvNEI2QjlPZ096OHFwY1ZPR3lLVThrM2hiU3RVSFVuQzJoTHRDdkpMaVdEanFTaEdEMG1YUE9FU0pTcAozbXMrY1ZoMVFMNVRyUTlWUHdnL2RrWklaeGVoQWhwQW1QSTJpVWM5MXJwTDQxUkFHeXdwSFkrWTQvN05CM3FMCjJvR3dhcDJkeW96b2g4S3VkTXZXREtFOGpRSk9STkp3WkxramcrRUNnWUE1WHphd08rdVlaVEwzMlY2dDhVc0EKSUU4MnZqTGxBVk5nUGpiMm9NdVVUOUNTSnpvSE5DN0R6TTJYYkV2QXJWL2VrVTBETEVxZC9KMjl2TnF1S1o3WQp6RHF1VjNkMVJ4NnNydGhtUGY5QTVGYnh6VGRKaDJDS3VVR1k1TVBHY3p5ZXcrbklXcnBaWVBLQ2Y1NXVWU0N3CnVuZWpTc3IxOWk0Z084dFpOaHQ1Y1FLQmdETjI0bWtFN1NQa2tuaWx5S01BWStpbnRZL1NlWUVWM042Rjl1R3gKMVBLd2UzL3M3Qzd2SmVpYzNZN1czK2FjZGxUbkxyc2RzL01ZbitQRXNtUmVzZXhRcENLS3gxaUo2UFRYZXV5KwpCWVhoOHV4QTh3b0NwL1ZzaENhaElzeWhEcTlGdEZWSC8zbGJteHJmUEloai96VVRCdW44aWRmalZEQUNCalFECldQY0JBb0dCQUpVazV5UVhpQWg0VDJBREpPeG1IV3ZyZ1ByTmFuODAxVm9idHZNRDExMFg0Y05SaDMxdkZ1SUUKdU5hSzd2T1VIcVlaTFNUYlVsS0tLZlNpVDJCc1ZYWHFLdzQ3M3hwWis2ZnN4R0RzM2xpeGh6aWUrL1ZwRVQxcQpsMFBuUGFmUCtGSnY0RFI4ZmN5amkwUHRRN2VydkFxais4L1o4QnZHTXpsWVRtSHdVVjBZCi0tLS0tRU5EIFJTQSBQUklWQVRFIEtFWS0tLS0tCg=="
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
