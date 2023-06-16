from pathlib import Path

import yaml
import pytest
import kubernetes

from helpers import (
    check_akri_is_healthy,
    assert_akri_instances_present,
    assert_broker_pods_running,
    assert_svc_present,
    get_agent_logs,
)

discovery_handlers = ["debugEcho"]


@pytest.fixture(scope="module")
def basic_config(akri_version):
    with open(Path(__file__).parent / "yaml/debugEchoConfiguration.yaml") as f:
        body = yaml.safe_load(f)
    client = kubernetes.client.CustomObjectsApi()
    version = f'v{akri_version.split(".")[0]}'
    client.create_namespaced_custom_object(
        "akri.sh", version, "default", "configurations", body
    )
    yield body["metadata"]["name"]
    client.delete_namespaced_custom_object(
        "akri.sh", version, "default", "configurations", body["metadata"]["name"]
    )


def test_crd_applied():
    v1_ext = kubernetes.client.ApiextensionsV1Api()
    current_crds = [
        x["spec"]["names"]["kind"].lower()
        for x in v1_ext.list_custom_resource_definition().to_dict()["items"]
        if x["spec"]["group"] == "akri.sh"
    ]
    assert "configuration" in current_crds
    assert "instance" in current_crds


def test_akri_healthy():
    check_akri_is_healthy(discovery_handlers)


def test_all_scheduled(akri_version, basic_config):
    check_akri_is_healthy(discovery_handlers)
    assert_akri_instances_present(akri_version, basic_config, 2)
    assert_broker_pods_running(basic_config, 2)
    assert_svc_present(basic_config, True, 2)
    assert_svc_present(basic_config, False, 1)


def test_device_offline(akri_version, basic_config):
    # Check we are in sane setup
    assert_akri_instances_present(akri_version, basic_config, 2)
    assert_broker_pods_running(basic_config, 2)
    assert_svc_present(basic_config, True, 2)
    assert_svc_present(basic_config, False, 1)

    v1_core = kubernetes.client.CoreV1Api()
    pods = v1_core.list_namespaced_pod(
        "default",
        label_selector=f"app.kubernetes.io/name=akri-debug-echo-discovery",
        field_selector="status.phase=Running",
    ).items
    base_command = ["/bin/sh", "-c"]
    command = "echo {} > /tmp/debug-echo-availability.txt"
    # Unplug the devices
    for pod in pods:
        kubernetes.stream.stream(
            v1_core.connect_get_namespaced_pod_exec,
            pod.metadata.name,
            "default",
            command=base_command + [command.format("OFFLINE")],
            stdout=True,
            stdin=False,
            stderr=False,
            tty=False,
        )
    assert_akri_instances_present(akri_version, basic_config, 0)
    assert_broker_pods_running(basic_config, 0)
    assert_svc_present(basic_config, True, 0)
    assert_svc_present(basic_config, False, 0)
    # Plug them back
    for pod in pods:
        kubernetes.stream.stream(
            v1_core.connect_get_namespaced_pod_exec,
            pod.metadata.name,
            "default",
            command=base_command + [command.format("ONLINE")],
            stdout=True,
            stdin=False,
            stderr=False,
            tty=False,
        )
    assert_akri_instances_present(akri_version, basic_config, 2)
    assert_broker_pods_running(basic_config, 2)
    assert_svc_present(basic_config, True, 2)
    assert_svc_present(basic_config, False, 1)


def test_cleanup(akri_version, faker):
    with open(Path(__file__).parent / "yaml/debugEchoConfiguration.yaml") as f:
        body = yaml.safe_load(f)
    # Change configuration name to avoid conflicting with basic_config fixture
    body["metadata"]["name"] = faker.domain_word()
    client = kubernetes.client.CustomObjectsApi()
    version = f'v{akri_version.split(".")[0]}'
    client.create_namespaced_custom_object(
        "akri.sh", version, "default", "configurations", body
    )
    # Wait for broker pods
    config_name = body["metadata"]["name"]
    assert_broker_pods_running(config_name, 2)
    client.delete_namespaced_custom_object(
        "akri.sh", version, "default", "configurations", config_name
    )
    check_akri_is_healthy(discovery_handlers)
    assert_akri_instances_present(akri_version, config_name, 0)
    assert_broker_pods_running(config_name, 0)
    assert_svc_present(config_name, True, 0)
    assert_svc_present(config_name, False, 0)


def test_slot_reconciliation():
    agent_logs = get_agent_logs(since=20)
    for logs in agent_logs.values():
        assert "get_node_slots - crictl called successfully" in logs


def test_broker_recreated_if_deleted(basic_config):
    # Ensure we are in sane state
    assert_broker_pods_running(basic_config, 2)
    v1_core = kubernetes.client.CoreV1Api()
    pods = v1_core.list_namespaced_pod(
        "default",
        label_selector=f"akri.sh/configuration={basic_config}",
        field_selector="status.phase=Running",
    ).items

    deleted_pod = v1_core.delete_namespaced_pod(pods[0].metadata.name, "default")
    w = kubernetes.watch.Watch()
    for e in w.stream(
        v1_core.list_namespaced_pod,
        "default",
        field_selector=f"metadata.name={deleted_pod.metadata.name}",
        resource_version=deleted_pod.metadata.resource_version,
    ):
        if e["type"] == "DELETED":
            w.stop()
    assert_broker_pods_running(basic_config, 2)
    new_pods = v1_core.list_namespaced_pod(
        "default",
        label_selector=f"akri.sh/configuration={basic_config}",
        field_selector="status.phase=Running",
    ).items
    assert pods[0].metadata.uid not in [pod.metadata.uid for pod in new_pods]
