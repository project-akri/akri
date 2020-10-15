#!/usr/bin/env python3
import shared_test_code
import json, os, time, yaml
from kubernetes import client, config
from kubernetes.client.rest import ApiException

def main():
    print("Helm-install-delete test main start")

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
    helm_install_command = "helm install akri akri-helm-charts/akri --version {} --set debugEcho.enabled=true --set debugEcho.name={} --set debugEcho.shared=false --set agent.allowDebugEcho=true".format(test_version, shared_test_code.DEBUG_ECHO_NAME)
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
    print("Loading k8s config")
    config.load_kube_config(config_file="~/.kube/config")

    # Ensure Helm Akri installation applied CRDs and set up agent and controller
    print("Checking for CRDs")
    if not shared_test_code.crds_applied():
        print("CRDs not applied by helm chart")
        return False
    
    print("Checking for initial Akri state")
    if not shared_test_code.check_akri_state(1, 1, 2, 2, 1, 2):
        print("Akri not running in expected state")
        os.system('sudo microk8s  kubectl get pods,services,akric,akrii --show-labels')
        return False
    
    return True    

main()
