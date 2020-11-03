# Using the Udev Discovery Protocol in a Configuration
## Background
Udev is the device manager for Linux. It manages device nodes in the `/dev` directory, such as microphones, security
chips, usb cameras, and so on. Udev can be used to find devices that are attached to or embedded in nodes. 

## Udev discovery in Akri
Akri's udev discovery handler parses udev rules listed in a Configuration, searches for them using udev, and returns a
list of discovered device nodes (ie: /dev/video0). You tell Akri which device(s) to find by passing [udev
rules](https://wiki.archlinux.org/index.php/Udev) into a Configuration. Akri has created a
[grammar](../agent/src/protocols/udev/udev_rule_grammar.pest) for parsing the rules, expecting them to be formatted
according to the [Linux Man pages](https://linux.die.net/man/7/udev). While udev rules are normally used to both find
devices and perform actions on devices, the Akri udev discovery handler is only interested in finding devices.
Consequently, the discovery handler will ignore all action operations ("=" , "+=" , "-=" , ":=") and action fields such
as `TEST` in the udev rules. You should only use match operations ("==",  "!=") and fields such as `KERNEL` and
`DEVPATH` in your udev rules. Additionally, there are some match fields that look up the device hierarchy, such as
`SUBSYSTEMS`, that are yet to be supported and are also ignored. See the
[grammar](../agent/src/protocols/udev/udev_rule_grammar.pest) for the full list of supported fields.

## Choosing a udev rule
To see what devices will be discovered on a specific node by a udev rule, you can use `udevadm`. For example, to find
all devices in the sound subsystem, you could run:
```sh
udevadm trigger --verbose --dry-run --type=devices --subsystem-match=sound
```
To see all the properties of a specific device discovered, you can use `udevadm info`:
```sh
udevadm info --attribute-walk --path=$(udevadm info --query=path /sys/devices/pci0000:00/0000:00:1f.3/sound/card0)
```
Now, you can see a bunch of attributes you could use to narrow your udev rule. Maybe you decide you want to find all
sound devices made by the vendor `Great Vendor`. You set the following udev rule under the udev protocol in your
Configuration:
```yaml
spec:
  protocol:
    udev:
      udevRules:
      -  'SUBSYSTEM=="sound", ATTR{vendor}=="Great Vendor"'
```

## Using the udev Configuration template
Instead of having to assemble your own udev Configuration yaml, we have provided a [Helm
template](../deployment/helm/templates/udev.yaml). Helm allows us to parametrize the commonly modified fields in our configuration files, and we have provided many for udev (to see
them, run `helm inspect values akri-helm-charts/akri`). 
To add the udev Configuration to your cluster, simply set
`udev.enabled=true`. Be sure to also **specify one or more udev rules** for the Configuration. If you want Akri to only
discover and advertize the resources, omit a broker pod image. Helm will automatically apply the udev Configuration yaml
for you, and the Akri Agent will advertize discovered leaf devices as resources. By default, the udev Configuration does
not specify a broker pod or services, so upon discovery, broker pods will not be deployed nor will services be created.
Later, we will discuss [how to add a custom broker to the
Configuration](./#adding-a-custom-broker-to-the-configuration).
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri-dev \
    --set useLatestContainers=true \
    --set udev.enabled=true \
    --set udev.udevRules[0]='SUBSYSTEM=="sound"\, ATTR{vendor}=="Great Vendor"'
```

The udev Configuration can be tailored to your cluster by modifying the [Akri helm chart values](../deployment/helm/values.yaml) in the following ways:

* Modifying the udev rule
* Specifying a broker pod image
* Disabling automatic Instance/Configuration Service creation
* Modifying the broker PodSpec (See [Modifying a Akri
  Installation](./modifying-akri-installation#modifying-the-brokerpodspec))
* Modifying instanceServiceSpec or configurationServiceSpec (See [Modifying a Akri
  Installation](./modifying-akri-installation#modifying-instanceservicespec-or-configurationservicespec))

For more advanced Configuration changes that are not aided by
our Helm chart, we suggest creating a Configuration file using Helm and then manually modifying it. To do this, see our documentation on [Modifying an Akri Installation](./modifying-akri-installation.md#generating-modifying-and-applying-a-configuration-yaml)

## Modifying the udev rule
The udev protocol will find all devices that are described by ANY of the udev rules. For example, to discover devices made by either Great Vendor or Awesome Vendor, you could add a second udev rule.
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri-dev \
    --set useLatestContainers=true \
    --set udev.enabled=true \
    --set udev.udevRules[0]='SUBSYSTEM=="sound"\, ATTR{vendor}=="Great Vendor"' \
    --set udev.udevRules[1]='SUBSYSTEM=="sound"\, ATTR{vendor}=="Awesome Vendor"'
```
Akri will now discover these devices and advertize them to the cluster as resources. Each discovered device is
represented as an Akri Instance. To list them, run `kubectl get akrii`. Note `akrii` is a short name for Akri Instance.
All the instances will be named in the format `<configuration-name>-<hash>`. You could change the name of the
Configuration and resultant Instances to be `sound-device` by adding `--set udev.name=sound-devices` to your
installation command. Now, you can schedule pods that request these Instances as resources, as explained in the
[requesting akri resources document](./requesting-akri-resources.md). 

## Specifying a broker pod image
Instead of manually deploying Pods to resources advertized by Akri, you can add a broker image to the udev
Configuration. Then, a broker will automatically be deployed to each discovered device. The controller will inject the
information the broker needs to find its device as an environment variable. Namely, it injects an environment variable
named `UDEV_DEVNODE` which contains the devnode path for that device (ie: `/dev/snd/pcmC0D0c`). The broker can grab this
environment variable and proceed to interact with the device. To add a broker to the udev configuration, set the
`udev.brokerPod.image.repository` value to point to your image. As an example, the installation below will deploy an
empty nginx pod for each instance. Instead, you can point to your image, say `ghcr.io/<USERNAME>/sound-broker`.
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri-dev \
    --set useLatestContainers=true \
    --set udev.enabled=true \
    --set udev.udevRules[0]='SUBSYSTEM=="sound"\, ATTR{vendor}=="Great Vendor"' \
    --set udev.brokerPod.image.repository=nginx
```
The Configuration will automatically create a broker for each discovered device. It will also create a service for each
broker and one for all brokers of the Configuration that applications can point to. See the [Modifying an Akri
Installation](./modifying-akri-installation.md) to learn how to [modify the broker pod
spec](./modifying-akri-installation.md#modifying-the-brokerpodspec) and [service
specs](./modifying-akri-installation.md#modifying-instanceservicespec-or-configurationservicespec) in the Configuration.

## Disabling automatic service creation
By default, the generic udev Configuration will create services for all the brokers of a specific Akri Instance and all the brokers of an Akri Configuration. Disable the create of Instance level services and Configuration level services by setting `--set udev.createInstanceServices=false` and `--set udev.createConfigurationService=false`, respectively.

## Modifying a Configuration
More information about how to modify an installed Configuration, add additional protocol Configurations to a cluster, or delete a Configuration can be found in the [Modifying a Akri Installation document](./modifying-akri-installation.md).

## Implementation details
The udev implementation can be understood by looking at several things:
1. [UdevDiscoveryHandlerConfig](../shared/src/akri/configuration.rs) defines the required properties
1. [The udev property in akri-configuration-crd.yaml](../deployment/helm/crds/akri-configuration-crd.yaml) validates the
   CRD input
1. [UdevDiscoveryHandler](../agent/src/protocols/udev/discovery_handler.rs) defines udev camera discovery
1. [samples/brokers/udev-video-broker](../samples/brokers/udev-video-broker) defines the udev protocol broker
1. [udev_rule_grammar.pest](../agent/src/protocols/udev/udev_rule_grammar.pest) defines the grammar for parsing udev
   rules and enumerate which fields are supported (such as `ATTR` and `TAG`), which are yet to be supported (`ATTRS` and
   `TAGS`), and which fields will never be supported, mainly due to be assignment rather than matching fields (such as
   `ACTION` and `GOTO`).