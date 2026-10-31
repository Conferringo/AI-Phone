"""Facts about inspected runtime contents, separate from installation acceptance."""
from client_host_entry import COMPONENT_MANIFESTS

SIDECARS={'android-runtime','container-images','installer-tools'}
SERVICES={'web','native-services','js-services'}
MACOS_RUNTIME=set(COMPONENT_MANIFESTS)|SIDECARS
HOST_RUNTIMES={'macos-arm64':MACOS_RUNTIME}


def delivery_metadata(components,host):
    if not isinstance(components,list) or any(not isinstance(name,str) for name in components):
        raise ValueError('Invalid client component list')
    selected=set(components)
    if len(selected)!=len(components) or selected-set(COMPONENT_MANIFESTS)-SIDECARS:
        raise ValueError('Unknown or duplicate client component')
    required=HOST_RUNTIMES.get(host,MACOS_RUNTIME|{'host-runtime'})
    missing=sorted(required-selected)
    server=SERVICES<=selected
    complete=not missing
    return {'prebuilt_components':components,'server_included':server,'runtime_payload_complete':complete,
            'complete_device_bundle':complete,'missing_runtime_components':missing,
            'deployment_mode':'embedded-local' if complete else 'component-bundle',
            'server_requirement':'None; the client initializes its local device runtime.' if complete else
                'Client-managed source setup or a configured local/LAN service; complete device hosting is not included.'}
