"""Facts about inspected installers and contents, separate from release approval."""
from pathlib import Path
from client_host_entry import COMPONENT_MANIFESTS

ASSET_LIMIT=2*1024**3
INSTALLER_FORMATS={'macos-arm64':{'dmg'},'windows-x64':{'exe'},
                   'linux-x64':{'deb','appimage'},'linux-amd64':{'deb','appimage'},
                   'linux-arm64':{'deb','appimage'},'android-arm64':{'apk'}}


def installer_metadata(path,host):
    path=Path(path);kind=path.suffix.lstrip('.').casefold()
    if kind not in INSTALLER_FORMATS.get(host,set()):
        raise ValueError('Unsupported single-file installer format')
    if path.is_symlink() or not path.is_file():
        raise ValueError('Installer must be one nonempty regular file')
    size=path.stat().st_size
    if size==0:raise ValueError('Installer must be one nonempty regular file')
    return {'file':path.name,'format':kind,'size':size,'single_file':True,
            'user_assembly_required':False,'github_asset_eligible':size<ASSET_LIMIT}

CORE_SIDECARS={'android-runtime','container-images','installer-tools'}
SIDECARS=CORE_SIDECARS|{'wsl-backend'}
SERVICES={'web','native-services','js-services'}
FULL_RUNTIME=set(COMPONENT_MANIFESTS)|CORE_SIDECARS
HOST_RUNTIMES={'macos-arm64':FULL_RUNTIME,'linux-x64':FULL_RUNTIME,'linux-amd64':FULL_RUNTIME,
               'windows-x64':{'android-runtime','wsl-backend'}}


def delivery_metadata(components,host):
    if not isinstance(components,list) or any(not isinstance(name,str) for name in components):
        raise ValueError('Invalid client component list')
    selected=set(components)
    if len(selected)!=len(components) or selected-set(COMPONENT_MANIFESTS)-SIDECARS:
        raise ValueError('Unknown or duplicate client component')
    required=HOST_RUNTIMES.get(host,FULL_RUNTIME|{'host-runtime'})
    missing=sorted(required-selected)
    backend='wsl-backend' in selected
    server=SERVICES<=selected or backend
    complete=not missing
    return {'prebuilt_components':components,'server_included':server,'runtime_payload_complete':complete,
            'complete_device_bundle':complete,'missing_runtime_components':missing,
            'deployment_mode':'embedded-local' if complete else 'component-bundle',
            'server_requirement':'None; the client initializes its local device runtime.' if complete else
                ('Bundled Linux backend; matching native Android resources are still required.' if backend else
                 'Client-managed source setup or a configured local/LAN service; complete device hosting is not included.')}
