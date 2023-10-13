from dataclasses import dataclass
import subprocess
import pytest
import kubernetes
from pathlib import Path

from helpers import save_akri_logs


def pytest_addoption(parser):
    parser.addoption(
        "--distribution", action="store", help="Specify distribution to use"
    )
    parser.addoption("--test-version", action="store", help="version to test")
    parser.addoption(
        "--local-tag", action="store", default="pr", help="tag for local images"
    )
    parser.addoption("--use-local", action="store_true", help="use local images if set")
    parser.addoption("--release", action="store_true", help="use released helm chart")


@dataclass
class Distribution:
    name: str
    kubeconfig: Path
    kubectl: str


@pytest.fixture(scope="session")
def distribution_config(pytestconfig):
    distribution = pytestconfig.getoption("--distribution", None)
    if distribution == "k3s":
        yield Distribution("k3s", Path.home() / ".kube/config", "kubectl")
    elif distribution == "k8s":
        yield Distribution("k8s", Path.home() / ".kube/config", "kubectl")
    elif distribution == "microk8s":
        yield Distribution("microk8s", Path.home() / ".kube/config", "kubectl")
    elif distribution is None:
        pytest.exit(
            "Please provide a kubernetes distribution via '--distribution' flag"
        )
    else:
        pytest.exit(
            "Wrong distribution provided, valid values are 'k3s', 'k8s' or 'microk8s'"
        )


@pytest.fixture(scope="session", autouse=True)
def kube_client(distribution_config):
    kubernetes.config.load_kube_config(str(distribution_config.kubeconfig))
    return kubernetes.client.ApiClient()


@pytest.fixture(scope="session")
def akri_version(pytestconfig):
    local_version = (Path(__file__).parent / "../../version.txt").read_text().strip()
    version = pytestconfig.getoption("--test-version")
    if version is None:
        version = local_version
    return version


@pytest.fixture(scope="module", autouse=True)
def install_akri(request, distribution_config, pytestconfig, akri_version):
    discovery_handlers = getattr(request.module, "discovery_handlers", [])

    release = pytestconfig.getoption("--release", False)
    subprocess.run(["helm", "repo", "update"], check=True)
    helm_install_command = ["helm", "install", "akri"]

    if pytestconfig.getoption("--use-local"):
        local_tag = pytestconfig.getoption("--local-tag", "pr")
        helm_install_command.extend(
            [
                Path(__file__).parent / "../../deployment/helm",
                "--set",
                "agent.image.pullPolicy=Never,"
                f"agent.image.tag={local_tag},"
                "controller.image.pullPolicy=Never,"
                f"controller.image.tag={local_tag},"
                "webhookConfiguration.image.pullPolicy=Never,"
                f"webhookConfiguration.image.tag={local_tag}",
            ]
        )
    else:
        chart_name = "akri" if release else "akri-dev"
        helm_install_command.extend(
            [
                f"akri-helm-charts/{chart_name}",
                "--version",
                akri_version,
            ]
        )

    for discovery_handler in discovery_handlers:
        if discovery_handler == "debugEcho":
            helm_install_command.extend(
                [
                    "--set",
                    "agent.allowDebugEcho=true,debugEcho.configuration.shared=false",
                ]
            )
        if pytestconfig.getoption("--use-local"):
            local_tag = pytestconfig.getoption("--local-tag", "pr")
            helm_install_command.extend(
                [
                    "--set",
                    f"{discovery_handler}.discovery.image.pullPolicy=Never,"
                    f"{discovery_handler}.discovery.image.tag={local_tag}"
                ])
        helm_install_command.extend(
            [
                "--set",
                f"{discovery_handler}.discovery.enabled=true",
            ]
        )
    helm_install_command.extend(
        [
            "--set",
            f"kubernetesDistro={distribution_config.name}",
            "--debug",
            "--atomic",
        ]
    )
    subprocess.run(helm_install_command, check=True)
    yield
    save_akri_logs(getattr(request.module, "__name__"))
    subprocess.run(["helm", "delete", "akri", "--wait"])
