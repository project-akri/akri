---
name: Bug report
about: Create a report to help us improve
title: ''
labels: bug
assignees: ''

---

**Describe the bug**
A clear and concise description of what the bug is.

**Output of `kubectl get pods,akrii,akric -o wide`**

**Kubernetes Version: [e.g. Native Kubernetes 1.19, MicroK8s 1.19, Minikube 1.19, K3s]**

**To Reproduce**
Steps to reproduce the behavior:
1. Create cluster using '...'
2. Install Akri with the Helm command '...'
3. '...'

**Expected behavior**
A clear and concise description of what you expected to happen.

**Logs (please share snips of applicable logs)**
 - To get the logs of any pod, run `kubectl logs <pod name>`
 - To get the logs of a pod that has already terminated, `kubectl get logs <pod name> --previous`
 - If you believe that the problem is with the Kubelet, run `journalctl -u kubelet` or `journalctl -u snap.microk8s.daemon-kubelet` if you are using a MicroK8s cluster.

**Additional context**
Add any other context about the problem here.
