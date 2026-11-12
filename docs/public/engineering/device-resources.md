# Change device resources after setup

The workbench can edit a device's name and Agent assignment today. CPU and memory are selected during installation. The planned resource editor belongs in the same device details, with the current values and one **Apply and restart** action.

| Device | Read the applied settings | Apply and restart |
| --- | --- | --- |
| Android | Read the owned AVD configuration. | Use the native session owner to stop the emulator, update the CPU and memory keys atomically, then start it with the existing AVD. Return the values after root ADB and the first frame are ready. |
| Linux desktop | Read the owned desktop container's resource limits. macOS Lima, Windows QEMU and Windows WSL all run the same desktop container. | Update the container limits and restart only the desktop container. Retain the mounted home. Check the available host memory before applying, then verify the limits and first frame. |

Use one device-scoped protobuf read and one apply operation. Fetch settings when the user opens a device. An unchanged apply returns immediately. The device owner serializes an apply with its lifecycle and advances the device's control epoch so old actions cannot reach the restarted device. The response reports the actual applied state. No schema migration or registry copy of these values is needed.

Disk expansion is a separate operation: stop the device, grow the backing image and guest filesystem, then verify capacity. The first editor covers CPU and memory.
