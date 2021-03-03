#!/usr/bin/env python3
import shared_test_code
import json, os, subprocess, time, yaml
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

    helm_chart_location = shared_test_code.get_helm_chart_location()
    print("Get Akri Helm chart: {}".format(helm_chart_location))
    cri_args = shared_test_code.get_cri_args()
    print("Providing Akri Helm chart with CRI args: {}".format(cri_args))
    extra_helm_args = shared_test_code.get_extra_helm_args()
    print("Providing Akri Helm chart with extra helm args: {}".format(extra_helm_args))
    helm_install_command = "helm install akri {} --set debugEcho.enabled=true --set debugEcho.name={} --set agent.allowDebugEcho=true {} {} --debug ".format(helm_chart_location, shared_test_code.DEBUG_ECHO_NAME, cri_args, extra_helm_args)
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
    
    #
    # Check agent responds to dynamic offline/online resource
    # 
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

    #
    # Check that slot reconciliation is working on agent
    # 
    print("Check logs for Agent slot-reconciliation for pod {}".format(shared_test_code.agent_pod_name))
    temporary_agent_log_path = "/tmp/agent_log.txt"
    for x in range(3):
        log_result = subprocess.run('sudo {} logs {} > {}'.format(kubectl_cmd, shared_test_code.agent_pod_name, temporary_agent_log_path), shell=True)
        if log_result.returncode == 0:
            print("Successfully stored Agent logs in {}".format(temporary_agent_log_path))
            break
        print("Failed to get logs from {} pod with result {} on attempt {} of 3".format(shared_test_code.agent_pod_name, log_result, x))
        if x == 2:
            return False
    grep_result = subprocess.run(['grep', "get_node_slots - crictl called successfully", temporary_agent_log_path])
    if grep_result.returncode != 0:
        print("Akri failed to successfully connect to crictl via the CRI socket with return value of {}", grep_result)
        # Log information to understand why error occurred
        os.system('sudo {} get pods,services,akric,akrii --show-labels'.format(kubectl_cmd))
        os.system('grep get_node_slots {}'.format(temporary_agent_log_path))
        return False

    #
    # Check that broker is recreated if it is deleted
    # 
    broker_pod_selector = "{}={}".format(shared_test_code.CONFIGURATION_LABEL_NAME, shared_test_code.DEBUG_ECHO_NAME)
    brokers_info = shared_test_code.get_running_pod_names_and_uids(broker_pod_selector)
    if len(brokers_info) != 2:
        print("Expected to find 2 broker pods but found: {}", len(brokers_info))
        os.system('sudo {} get pods,services,akric,akrii --show-labels'.format(kubectl_cmd))
        return False

    # There is a possible race condition here between when the `kubectl delete pod` returns,
    # when check_broker_pod_state validates that the pod is gone, and when the check_akri_state
    # validates that the broker pod has been restarted

    broker_pod_name = sorted(brokers_info.keys())[0]
    delete_pod_command = 'sudo {} delete pod {}'.format(kubectl_cmd, broker_pod_name)
    print("Deleting broker pod: {}".format(delete_pod_command))
    os.system(delete_pod_command)

    # Create kube client
    v1 = client.CoreV1Api()

    # Wait for there to be 2 brokers pods again
    if not shared_test_code.check_broker_pods_state(v1, 2):
        print("Akri not running in expected state after broker pod restoration should have happened")
        os.system('sudo {} get pods,services,akric,akrii --show-labels'.format(kubectl_cmd))
        return False

    restored_brokers_info = shared_test_code.get_running_pod_names_and_uids(broker_pod_selector)
    if len(restored_brokers_info) != 2:
        print("Expected to find 2 broker pods but found: {}", len(restored_brokers_info))
        os.system('sudo {} get pods,services,akric,akrii --show-labels'.format(kubectl_cmd))
        return False

    # Make sure that the deleted broker uid is different from the restored broker pod uid ... signifying
    # that the Pod was restarted
    print("Restored broker pod uid should differ from original broker pod uid")
    if brokers_info[broker_pod_name] == restored_brokers_info[broker_pod_name]:
        print("Restored broker pod uid [{}] should differ from original broker pod uid [{}]".format(brokers_info[broker_pod_name], restored_brokers_info[broker_pod_name]))
        os.system('sudo {} get pods,services,akric,akrii --show-labels'.format(kubectl_cmd))
        return False

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
