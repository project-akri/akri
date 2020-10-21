#!/usr/bin/env python3
import shared_test_code
import json, os, time, yaml
from kubernetes import client, config
from kubernetes.client.rest import ApiException

def main():
    print("End-to-end test main start")

    # If this is a PUSH, the test needs to wait for the new containers to be
    # built/pushed.  In this case, the workflow will set /tmp/sleep_duration.txt to
    # the number of seconds to sleep.
    # If this is a MANUALLY triggerd or a PULL-REQUEST, no new containers will 
    # be built/pushed, the workflows will not set /tmp/sleep_duration.txt and
    # this test will execute immediately.
    shared_test_code.initial_sleep()

    # Update Helm and install this version's chart
    os.system("helm repo update")

    # Get version of akri to test
    test_version = shared_test_code.get_test_version()
    print("Testing version: {}".format(test_version))
    
    shared_test_code.major_version = "v" + test_version.split(".")[0]
    print("Testing major version: {}".format(shared_test_code.major_version))

    print("Installing Akri Helm chart: {}".format(test_version))
    cri_args = shared_test_code.get_cri_args()
    print("Providing Akri Helm chart with CRI args: {}".format(cri_args))
    helm_install_command = "helm install akri akri-helm-charts/akri --version {} --set debugEcho.enabled=true --set debugEcho.name={} --set debugEcho.shared=false --set agent.allowDebugEcho=true {}".format(test_version, shared_test_code.DEBUG_ECHO_NAME, cri_args)
    print("Helm command: {}".format(helm_install_command))
    os.system(helm_install_command)
    
    try:
        res = do_test()
    except Exception as e:
        print(e)
        res = False
    finally:
        # Best effort cleanup work
        try:
            # Save Agent and controller logs
            shared_test_code.save_agent_and_controller_logs() 
        finally:
            # Delete akri and check that controller and Agent pods deleted
            os.system("helm delete akri")
            if res:
                # Only test cleanup if the test has succeeded up to now
                if not shared_test_code.check_akri_state(0, 0, 0, 0, 0, 0):
                    print("Akri not running in expected state after helm delete")
                    raise RuntimeError("Scenario Failed")
    
    if not res:
        raise RuntimeError("Scenario Failed")
    


def do_test():
    kubeconfig_path = shared_test_code.get_kubeconfig_path()
    print("Loading k8s config: {}".format(kubeconfig_path))
    config.load_kube_config(config_file=kubeconfig_path)

    # Get kubectl command
    kubectl_cmd = shared_test_code.get_kubectl_command()

    # Ensure Helm Akri installation applied CRDs and set up agent and controller
    print("Checking for CRDs")
    if not shared_test_code.crds_applied():
        print("CRDs not applied by helm chart")
        return False
    
    print("Checking for initial Akri state")
    if not shared_test_code.check_akri_state(1, 1, 2, 2, 1, 2):
        print("Akri not running in expected state")
        os.system('sudo {} get pods,services,akric,akrii --show-labels'.format(kubectl_cmd))
        return False
    
    # Do offline scenario
    print("Writing to Agent pod {} that device offline".format(shared_test_code.agent_pod_name))
    os.system('sudo {} exec -i {} -- /bin/bash -c "echo "OFFLINE" > /tmp/debug-echo-availability.txt"'.format(kubectl_cmd, shared_test_code.agent_pod_name))

    print("Checking Akri state after taking device offline")
    if not shared_test_code.check_akri_state(1, 1, 0, 0, 0, 0):
        print("Akri not running in expected state after taking device offline")
        os.system('sudo {} get pods,services,akric,akrii --show-labels'.format(kubectl_cmd))
        return False

    # Do back online scenario
    print("Writing to Agent pod {} that device online".format(shared_test_code.agent_pod_name))
    os.system('sudo {} exec -i {} -- /bin/bash -c "echo "ONLINE" > /tmp/debug-echo-availability.txt"'.format(kubectl_cmd, shared_test_code.agent_pod_name))
    
    print("Checking Akri state after bringing device back online")
    if not shared_test_code.check_akri_state(1, 1, 2, 2, 1, 2):
        print("Akri not running in expected state after bringing device back online")
        os.system('sudo {} get pods,services,akric,akrii --show-labels'.format(kubectl_cmd))
        return False

    # Check Akri slot reconiliation logs for success
    print("Check logs for Agent slot-reconciliation for pod {}".format(shared_test_code.agent_pod_name))
    os.system('sudo {} logs $(sudo {} get pods | grep agent | awk \'{{print $1}}\') | grep "get_node_slots - crictl called successfully" | wc -l | grep -v 0'.format(kubectl_cmd, kubectl_cmd))

    # Do cleanup scenario
    print("Deleting Akri configuration: {}".format(shared_test_code.DEBUG_ECHO_NAME))
    os.system("sudo {} delete akric {}".format(kubectl_cmd, shared_test_code.DEBUG_ECHO_NAME))

    print("Checking Akri state after deleting configuration")
    if not shared_test_code.check_akri_state(1, 1, 0, 0, 0, 0):
        print("Akri not running in expected state after deleting configuration")
        os.system('sudo {} get pods,services,akric,akrii --show-labels'.format(kubectl_cmd))
        return False

    return True    

main()
