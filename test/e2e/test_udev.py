from pathlib import Path
import time

import yaml
import pytest
import kubernetes
from helpers import assert_akri_instances_present, check_akri_is_healthy


discovery_handlers = ["udev"]


@pytest.fixture
def dev_null_config(akri_version):
    with open(Path(__file__).parent / "yaml/udevDevNullConfiguration.yaml") as f:
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


def test_dev_null_config(akri_version, dev_null_config):
    check_akri_is_healthy(discovery_handlers)
    assert_akri_instances_present(akri_version, dev_null_config, 1)


@pytest.fixture
def grouped_config(akri_version):
    v1_core = kubernetes.client.CoreV1Api()
    pods = v1_core.list_namespaced_pod(
        "default",
        label_selector=f"app.kubernetes.io/name=akri-udev-discovery",
        field_selector="status.phase=Running",
    ).items
    base_command = ["/bin/sh", "-c"]
    # This command will get the ID_PATH to use to get a device with many "subdevices"
    command = "grep -hr ID_PATH= /run/udev/data | sort | uniq -cd | sort -n | tail -1 | cut -d '=' -f 2"
    paths = set()
    # Get the ID_PATH we can use
    for pod in pods:
        resp = kubernetes.stream.stream(
            v1_core.connect_get_namespaced_pod_exec,
            pod.metadata.name,
            "default",
            command=base_command + [command],
            stdout=True,
            stdin=False,
            stderr=True,
            tty=False,
            _preload_content=False,
        )
        try:
            paths.add(resp.readline_stdout(timeout=3).strip())
        except:
            pytest.skip(f"No udev ?")
    if len(paths) == 0:
        pytest.skip("No groupable devices found")
    path = paths.pop()

    with open(Path(__file__).parent / "yaml/udevGroupedConfiguration.yaml") as f:
        body = yaml.safe_load(f)
    body["spec"]["discoveryHandler"]["discoveryDetails"] = body["spec"][
        "discoveryHandler"
    ]["discoveryDetails"].format(path)
    client = kubernetes.client.CustomObjectsApi()
    version = f'v{akri_version.split(".")[0]}'
    client.create_namespaced_custom_object(
        "akri.sh", version, "default", "configurations", body
    )
    yield body["metadata"]["name"]
    client.delete_namespaced_custom_object(
        "akri.sh", version, "default", "configurations", body["metadata"]["name"]
    )


def test_grouped_config(akri_version, grouped_config):
    check_akri_is_healthy(discovery_handlers)
    assert_akri_instances_present(akri_version, grouped_config, 1)
