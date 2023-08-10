from pathlib import Path
from kubernetes.client.rest import ApiException

import kubernetes
import yaml
import pytest

discovery_handlers = ["debugEcho"]


def test_valid_configuration_accepted(akri_version):
    with open(Path(__file__).parent / "yaml/webhookValidConfiguration.yaml") as f:
        body = yaml.safe_load(f)
    client = kubernetes.client.CustomObjectsApi()
    version = f'v{akri_version.split(".")[0]}'
    client.create_namespaced_custom_object(
        "akri.sh", version, "default", "configurations", body
    )
    client.delete_namespaced_custom_object(
        "akri.sh", version, "default", "configurations", body["metadata"]["name"]
    )


def test_invalid_configuration_rejected(akri_version):
    with open(Path(__file__).parent / "yaml/webhookInvalidConfiguration.yaml") as f:
        body = yaml.safe_load(f)
    client = kubernetes.client.CustomObjectsApi()
    version = f'v{akri_version.split(".")[0]}'
    with pytest.raises(ApiException):
        client.create_namespaced_custom_object(
            "akri.sh", version, "default", "configurations", body
        )
