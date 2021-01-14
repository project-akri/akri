# Naming Guidelines

One of the [two hard things](https://martinfowler.com/bliki/TwoHardThings.html) in Compute Science is naming things. It is proposed that Akri adopt naming guidelines to make developers' lives easier by providing consistency and reduce naming complexity.

Akri existed before naming guidelines were documented and may not employ the guidelines summarized here. However, it is hoped that developers will, at least, consider these guidelines when extending Akri.

## General Principles

+ Akri uses English
+ Akri is written principally in Rust and Rust [Naming](https://rust-lang.github.io/api-guidelines/naming.html) conventions are used
+ Types need not be included in names unless ambiguity would result
+ Shorter, simpler names are preferred

## Akri Discovery Handlers

Various Discovery Handlers have been developed: `debug_echo`, `onvif`, `opcua`, `udev`

Guidance:

+ `snake_case` names
+ (widely understood) initializations|acronyms are preferred

## Akri Brokers

Various Brokers have been developed: `onvif-video-broker`, `opcua-monitoring-broker`, `udev-broker`

Guidance:

+ Broker names should reflect Discovery Handler (Protocol) names and be suffixed `-broker`
+ Use Programming language-specific naming conventions when developing Brokers in non-Rust languages

> **NOTE** Even though the initialization of [ONVIF](https://en.wikipedia.org/wiki/ONVIF) includes "Video", the specification is broader than video and the broker name adds specificity by including the word (`onvif-video-broker`) in order to effectively describe its functionality.

## Kubernetes Resources

Various Kubernetes Resources have been developed:

+ CRDS: `Configurations`, `Instances`
+ Instances: `akri-agent-daemonset`, `akri-controller-deployment`, `akri-onvif`, `akri-opcua`, `akri-udev`

Guidance:

+ Kubernetes Convention is that Resources (e.g. `DaemonSet`) and CRDs use (upper) CamelCase
+ Akri Convention is that Akri Kubernetes Resources be prefixed `akri-`, e.g. `akri-agent-daemonset`
+ Names combining words should use hypens (`-`) to separate the words e.g. `akri-debug-echo`

> **NOTE** `akri-agent-daemonset` contradicts the general principle of not including types, if it had been named after these guidelines were drafted, it would be named `akri-agent`.
>
> Kubernetes' resources are strongly typed and the typing is evident through the CLI e.g. `kubectl get daemonsets/akri-agent-daemonset` and through a resource's `Kind` (e.g. `DaemonSet`). Including such types in the name is redundant.


