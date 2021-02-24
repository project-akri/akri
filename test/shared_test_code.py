#mod shared_test_code

import json, os, time, yaml
from kubernetes import client, config
from kubernetes.client.rest import ApiException

CONFIGURATION_LABEL_NAME = "akri.sh/configuration"
INSTANCE_LABEL_NAME = "akri.sh/instance"
AGENT_POD_NAME = "akri-agent"
CONTROLLER_POD_NAME = "akri-controller"
GROUP = "akri.sh"
AGENT_LOG_PATH = "/tmp/agent_log.txt"
CONTROLLER_LOG_PATH = "/tmp/controller_log.txt"
DEBUG_ECHO_NAME = "akri-debug-echo-foo"
KUBE_CONFIG_PATH_FILE = "/tmp/kubeconfig_path_to_test.txt"
RUNTIME_COMMAND_FILE = "/tmp/runtime_cmd_to_test.txt"
HELM_CRI_ARGS_FILE = "/tmp/cri_args_to_test.txt"
VERSION_FILE = "/tmp/version_to_test.txt"
SLEEP_DURATION_FILE = "/tmp/sleep_duration.txt"
EXTRA_HELM_ARGS_FILE = "/tmp/extra_helm_args.txt"
HELM_CHART_LOCATION = "/tmp/helm_chart_location.txt"
SLEEP_INTERVAL = 20

CONTROLLER_POD_LABEL_SELECTOR = "app=" + CONTROLLER_POD_NAME
AGENT_POD_LABEL_SELECTOR = "name=" + AGENT_POD_NAME
BROKER_POD_LABEL_SELECTOR = CONFIGURATION_LABEL_NAME

CONFIGURATION_SVC_LABEL_SELECTOR = CONFIGURATION_LABEL_NAME
INSTANCE_SVC_LABEL_SELECTOR = INSTANCE_LABEL_NAME

major_version = ""
agent_pod_name = ""
controller_pod_name = ""


def get_helm_chart_location():
    # Get helm chart location passed in helm install command (i.e. `repo/chart --version X.Y.Z` or `./deployment/helm`)
    return open(HELM_CHART_LOCATION, "r").readline().rstrip()


def get_extra_helm_args():
    # Get any extra helm args passed from workflow
    if os.path.exists(EXTRA_HELM_ARGS_FILE):
        return open(EXTRA_HELM_ARGS_FILE, "r").readline().rstrip()
    return ""


def initial_sleep():
    # Sleep for amount of time specified in SLEEP_DURATION_FILE else don't sleep at all
    if os.path.exists(SLEEP_DURATION_FILE):
        initial_sleep_duration = open(SLEEP_DURATION_FILE,
                                      "r").readline().rstrip()
        print("Sleeping for {} seconds".format(initial_sleep_duration))
        time.sleep(int(initial_sleep_duration))
        print("Done sleeping")


def helm_update():
    # Update Helm and install this version's chart
    os.system("helm repo update")


def get_kubeconfig_path():
    # Get kubeconfig path
    return open(KUBE_CONFIG_PATH_FILE, "r").readline().rstrip()


def get_kubectl_command():
    # Get kubectl command
    return open(RUNTIME_COMMAND_FILE, "r").readline().rstrip()


def get_cri_args():
    # Get CRI args for Akri Helm
    return open(HELM_CRI_ARGS_FILE, "r").readline().rstrip()


def get_test_version():
    # Get version of akri to test
    if os.path.exists(VERSION_FILE):
        return open(VERSION_FILE, "r").readline().rstrip()
    return open("version.txt", "r").readline().rstrip()


def save_agent_and_controller_logs(namespace="default"):
    kubectl_cmd = get_kubectl_command()
    os.system("{} logs {} --namespace={} >> {}".format(kubectl_cmd,
                                                       agent_pod_name,
                                                       namespace,
                                                       AGENT_LOG_PATH))
    os.system("{} logs {} --namespace={} >> {}".format(kubectl_cmd,
                                                       controller_pod_name,
                                                       namespace,
                                                       CONTROLLER_LOG_PATH))


def crds_applied():
    print("Checking for CRDs")
    v1_ext = client.ApiextensionsV1beta1Api()
    for x in range(5):
        if x != 0:
            time.sleep(SLEEP_INTERVAL)
        current_crds = [
            x["spec"]["names"]["kind"].lower() for x in
            v1_ext.list_custom_resource_definition().to_dict()['items']
        ]
        if "configuration" in current_crds and "instance" in current_crds:
            return True
    return False


def check_pods_running(v1, pod_label_selector, count):
    print("Checking number of pods [{}] ... expected {}".format(
        pod_label_selector, count))
    for x in range(30):
        if x != 0:
            time.sleep(SLEEP_INTERVAL)
            print(
                "Sleep iteration {} ... been waiting for {} seconds for pod check"
                .format(x + 1, (x + 1) * SLEEP_INTERVAL))
        pods = v1.list_pod_for_all_namespaces(
            label_selector=pod_label_selector).items
        print("Found {} pods".format(len(pods)))
        if count == 0:
            # Expectation is that no pods are running
            if len(pods) == 0:
                return True
            else:
                all_terminating = True
                for pod in pods:
                    # Ensure that none of the pods are still running
                    if pod.status.phase != "Terminating":
                        all_terminating = False
                        break
            if all_terminating: return True
        else:
            # Expectation is that `count` pods are running
            all_running = True
            if len(pods) == count:
                for pod in pods:
                    if pod.status.phase != "Running":
                        all_running = False
                        break
                if all_running: return True
    print("Wrong number of pods [{}] found ... expected {}".format(
        pod_label_selector, count))
    return False


def check_svcs_running(v1, svc_label_selector, count):
    print("Checking number of svcs  [{}] ... expected {}".format(
        svc_label_selector, count))
    for x in range(30):
        if x != 0:
            time.sleep(SLEEP_INTERVAL)
            print(
                "Sleep iteration {} ... been waiting for {} seconds for svc check"
                .format(x + 1, (x + 1) * SLEEP_INTERVAL))
        svcs = v1.list_service_for_all_namespaces(
            label_selector=svc_label_selector).items
        print("Found {} pods".format(len(svcs)))
        if count == 0:
            # Expectation is that no svcs are running
            if len(svcs) == 0:
                return True
        else:
            # Expectation is that `count` svcs are running
            if len(svcs) == count:
                return True
    print("Wrong number of services [{}] found ... expected {}".format(
        svc_label_selector, count))
    return False


def get_pod_name(pod_label_selector, index):
    v1 = client.CoreV1Api()
    print("Getting pod name [{}]".format(pod_label_selector))
    pods = v1.list_pod_for_all_namespaces(
        label_selector=pod_label_selector).items
    if len(pods) >= index:
        if pods[index].status.phase == "Running":
            return pods[index].metadata.name
    return ""


def get_running_pod_names_and_uids(pod_label_selector):
    v1 = client.CoreV1Api()
    map = {}
    print("Getting pod name [{}]".format(pod_label_selector))
    pods = v1.list_pod_for_all_namespaces(
        label_selector=pod_label_selector).items
    for pod in pods:
        if pod.status.phase == "Running":
            map[pod.metadata.name] = pod.metadata.uid
    return map


def check_instance_count(count, namespace="default"):
    print("Checking for instances ... version:{} count:{}".format(
        major_version, count))
    if count == 0:
        return True

    api_instance = client.CustomObjectsApi()
    for x in range(20):
        if x != 0:
            time.sleep(SLEEP_INTERVAL)
        print(
            "Sleep iteration {} ... been waiting for {} seconds for instances".
            format(x + 1, (x + 1) * SLEEP_INTERVAL))
        instances = api_instance.list_namespaced_custom_object(
            group=GROUP,
            version=major_version,
            namespace=namespace,
            plural="instances")['items']
        if len(instances) == count:
            return True
    return False


def check_agent_pods_state(v1, agents):
    global agent_pod_name
    print("Checking for agent pods ... expected {}".format(agents))
    agents_check_failed = check_pods_running(v1, AGENT_POD_LABEL_SELECTOR,
                                             agents)
    if not agents_check_failed:
        print("Wrong number of agents found ... expected {}".format(agents))
    else:
        if agents == 1:
            agent_pod_name = get_pod_name(AGENT_POD_LABEL_SELECTOR, 0)
            if agent_pod_name == "":
                print("Agent pod name not found")
                return False

    return agents_check_failed


def check_controller_pods_state(v1, controllers):
    global controller_pod_name
    print("Checking for controller pods ... expected {}".format(controllers))
    controllers_check_failed = check_pods_running(
        v1, CONTROLLER_POD_LABEL_SELECTOR, controllers)
    if not controllers_check_failed:
        print("Wrong number of controllers found ... expected {}".format(
            controllers))
    else:
        if controllers == 1:
            controller_pod_name = get_pod_name(CONTROLLER_POD_LABEL_SELECTOR,
                                               0)
            if controller_pod_name == "":
                print("Controller pod name not found")
                return False

    return controllers_check_failed


def check_broker_pods_state(v1, brokers):
    print("Checking for broker pods ... expected {}".format(brokers))
    brokers_check_failed = check_pods_running(v1, BROKER_POD_LABEL_SELECTOR,
                                              brokers)
    if not brokers_check_failed:
        print("Wrong number of brokers found ... expected {}".format(brokers))
    return brokers_check_failed


def check_config_svcs_state(v1, count: int):
    print("Checking for configuration services ... expected {}".format(count))
    config_svcs_check_failed = check_svcs_running(
        v1, CONFIGURATION_SVC_LABEL_SELECTOR, count)
    if not config_svcs_check_failed:
        print("Wrong number of configuration services found ... expected {}".
              format(count))
    return config_svcs_check_failed


def check_instance_svcs_state(v1, count: int):
    print("Checking for instance services ... expected {}".format(count))
    instance_svcs_check_failed = check_svcs_running(
        v1, INSTANCE_SVC_LABEL_SELECTOR, count)
    if not instance_svcs_check_failed:
        print("Wrong number of brokers found ... expected {}".format(count))
    return instance_svcs_check_failed


def check_akri_state(agents,
                     controllers,
                     instances,
                     brokers,
                     config_svcs,
                     instance_svcs,
                     namespace="default"):
    print(
        "Checking for Akri state ... expected agent(s):{}, controller(s):{}, instance(s):{}, broker(s):{}, config service(s):{}, and instance service(s):{} to exist"
        .format(agents, controllers, instances, brokers, config_svcs,
                instance_svcs))
    v1 = client.CoreV1Api()
    return check_agent_pods_state(v1, agents) and \
        check_controller_pods_state(v1, controllers) and \
        check_instance_count(instances, namespace) and \
        check_broker_pods_state(v1, brokers) and \
        check_config_svcs_state(v1, config_svcs) and \
        check_instance_svcs_state(v1, instance_svcs)
