import time

import kubernetes
from pathlib import Path


def get_pods_logs(label_selector, since=None):
    v1_core = kubernetes.client.CoreV1Api()
    pods = v1_core.list_namespaced_pod("default", label_selector=label_selector).items
    return {
        pod.metadata.name: v1_core.read_namespaced_pod_log(
            pod.metadata.name, "default", since_seconds=since
        )
        for pod in pods
    }


def get_agent_logs(since=None):
    return get_pods_logs("app.kubernetes.io/name=akri-agent", since=since)


def save_akri_logs(prefix):
    directory = Path("/tmp/logs")
    directory.mkdir(parents=True, exist_ok=True)
    logs = get_pods_logs("app.kubernetes.io/part-of=akri")
    for pod, content in logs.items():
        with open(directory / f"{prefix}-{pod}.log", "a") as f:
            f.write(content)


def check_akri_is_healthy(handlers):
    v1_core = kubernetes.client.CoreV1Api()
    for component in [f"{h}-discovery" for h in handlers] + ["agent", "controller"]:
        if component == "debugEcho-discovery":
            component = "debug-echo-discovery"
        pods = v1_core.list_namespaced_pod(
            "default",
            label_selector=f"app.kubernetes.io/name=akri-{component}",
            field_selector="status.phase=Running",
        )
        assert len(pods.items) > 0, f"{component} is not running"


def assert_broker_pods_running(config_name, count, timeout_seconds=400):
    v1_core = kubernetes.client.CoreV1Api()
    field_selector = (
        "status.phase=Running" if count > 0 else "status.phase!=Terminating"
    )
    pods = v1_core.list_namespaced_pod(
        "default",
        label_selector=f"akri.sh/configuration={config_name}",
        field_selector=field_selector,
    )
    version = pods.metadata.resource_version
    pods_set = {pod.metadata.name for pod in pods.items}
    if len(pods_set) == count:
        return
    w = kubernetes.watch.Watch()
    for e in w.stream(
        v1_core.list_namespaced_pod,
        "default",
        label_selector=f"akri.sh/configuration={config_name}",
        field_selector=field_selector,
        resource_version=version,
        timeout_seconds=timeout_seconds,
    ):
        if e["type"] == "DELETED":
            pods_set.discard(e["object"].metadata.name)
        else:
            pods_set.add(e["object"].metadata.name)
        if len(pods_set) == count:
            w.stop()
            return
    raise AssertionError(f"{count} != {len(pods_set)}")


def assert_svc_present(config_name, instance_level, count, timeout_seconds=400):
    v1_core = kubernetes.client.CoreV1Api()
    label_selector = (
        "akri.sh/instance" if instance_level else f"akri.sh/configuration={config_name}"
    )
    svcs = v1_core.list_namespaced_service("default", label_selector=label_selector)
    version = svcs.metadata.resource_version
    if instance_level:
        svcs_set = {
            svc.metadata.name
            for svc in svcs.items
            if svc.metadata.labels["akri.sh/instance"].startswith(f"{config_name}-")
        }
    else:
        svcs_set = {svc.metadata.name for svc in svcs.items}

    if count == len(svcs_set):
        return
    w = kubernetes.watch.Watch()
    for e in w.stream(
        v1_core.list_namespaced_service,
        "default",
        label_selector=label_selector,
        resource_version=version,
        timeout_seconds=timeout_seconds,
    ):
        if instance_level and not e["object"].metadata.labels[
            "akri.sh/instance"
        ].startswith(f"{config_name}-"):
            continue
        if e["type"] == "DELETED":
            svcs_set.discard(e["object"].metadata.name)
        else:
            svcs_set.add(e["object"].metadata.name)
        if len(svcs_set) == count:
            w.stop()
            return
    raise AssertionError(f"{len(svcs_set)} != {count}")


def assert_akri_instances_present(
    akri_version, config_name, count, timeout_seconds=400
):
    version = f'v{akri_version.split(".")[0]}'
    v1_custom = kubernetes.client.CustomObjectsApi()

    def get_instances():
        instances = v1_custom.list_namespaced_custom_object(
            "akri.sh", version, "default", "instances"
        )
        resource_version = instances["metadata"]["resourceVersion"]
        instances = {
            instance["metadata"]["name"]
            for instance in instances["items"]
            if instance["spec"]["configurationName"] == config_name
        }
        return instances, resource_version

    instances, resource_version = get_instances()
    if len(instances) == count:
        # Check it is not a transient state
        time.sleep(1)
        instances, resource_version = get_instances()
        if len(instances) == count:
            return
    w = kubernetes.watch.Watch()
    for e in w.stream(
        v1_custom.list_namespaced_custom_object,
        "akri.sh",
        version,
        "default",
        "instances",
        timeout_seconds=timeout_seconds,
        resource_version=resource_version,
    ):
        if e["raw_object"]["spec"]["configurationName"] != config_name:
            continue
        if e["type"] == "DELETED":
            instances.discard(e["raw_object"]["metadata"]["name"])
        else:
            instances.add(e["raw_object"]["metadata"]["name"])
        if len(instances) == count:
            # Check it is not a transient state
            time.sleep(1)
            instances, _ = get_instances()
            if len(instances) == count:
                w.stop()
                return
    raise AssertionError(f"{count} != {len(instances)}")
