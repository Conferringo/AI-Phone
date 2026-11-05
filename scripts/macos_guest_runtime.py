"""Maintainer-only native Mac compilation of the static Linux ARM desktop bridge."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
from artifact_io import digest
from release_package import select_files

TARGET='aarch64-unknown-linux-musl'
BINARY='build/target/release/desktop-bridge'


def source_identity(root):
    rows=[[name,hashlib.sha256(data).hexdigest()] for name,data,_ in select_files(root)
          if name in ('Cargo.toml','Cargo.lock','scripts/macos_guest_runtime.py') or name.startswith(('crates/guest-runtime/','crates/api/','packages/protocol/proto/'))]
    return hashlib.sha256(json.dumps(rows,separators=(',',':')).encode()).hexdigest()


def static_arm_elf(path):
    with Path(path).open('rb') as source:
        header=source.read(64)
        if len(header)!=64 or header[:6]!=b'\x7fELF\x02\x01' or struct.unpack_from('<H',header,18)[0]!=183:
            raise ValueError('Guest bridge is not Linux ARM64 ELF')
        offset=struct.unpack_from('<Q',header,32)[0]
        width,count=struct.unpack_from('<HH',header,54)
        if width!=56 or not 1<=count<=128 or offset<64 or offset+width*count>Path(path).stat().st_size:
            raise ValueError('Invalid guest ELF program headers')
        source.seek(offset);table=source.read(width*count)
        if any(struct.unpack_from('<I',table,index*width)[0]==3 for index in range(count)):
            raise ValueError('Guest bridge requires an external ELF interpreter')


def verify(root,folder,architecture):
    folder=Path(folder)
    if architecture!='arm64' or folder.is_symlink():raise ValueError('Guest component architecture mismatch')
    manifest=json.loads((folder/'manifest.json').read_text())
    binary=folder/BINARY
    if manifest.get('target')!=TARGET or manifest.get('source_sha256')!=source_identity(root):
        raise ValueError('Guest component source mismatch')
    if binary.is_symlink() or digest(binary)!=manifest.get('sha256'):
        raise ValueError('Guest component binary changed')
    static_arm_elf(binary)
    return manifest


def build(root,work):
    root,work=Path(root).resolve(),Path(work)
    if work.exists():raise ValueError('Use a fresh guest compiler directory')
    version=subprocess.check_output(['rustc','--version'],text=True).split()
    if len(version)<2 or version[1]!='1.96.0':raise ValueError('Guest factory requires Rust 1.96.0')
    sysroot=Path(subprocess.check_output(['rustc','--print','sysroot'],text=True).strip())
    linker=sysroot/'lib/rustlib/aarch64-apple-darwin/bin/rust-lld'
    if not linker.is_file() or not (sysroot/'lib/rustlib'/TARGET/'lib').is_dir():
        raise ValueError('Guest factory requires rust-lld and the Linux ARM musl standard library')
    work.mkdir(parents=True);target=work/'target'
    environment={**os.environ,'CARGO_TARGET_DIR':str(target),'CARGO_BUILD_JOBS':'2','CARGO_INCREMENTAL':'0',
                 'CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER':str(linker),
                 'CARGO_ENCODED_RUSTFLAGS':'\x1f'.join(['-Clinker-flavor=ld.lld','--remap-path-prefix='+str(Path.home())+'=/build-user'])}
    subprocess.run(['cargo','build','--release','--locked','--package','ai-phone-guest-runtime','--bin','desktop-bridge','--target',TARGET],
                   cwd=root,env=environment,check=True)
    binary=target/TARGET/'release/desktop-bridge';static_arm_elf(binary)
    context=work/'context';destination=context/BINARY;destination.parent.mkdir(parents=True)
    shutil.copyfile(binary,destination);destination.chmod(0o755)
    manifest={'format':1,'target':TARGET,'source_sha256':source_identity(root),'sha256':digest(destination)}
    (context/'manifest.json').write_text(json.dumps(manifest,sort_keys=True)+'\n')
    verify(root,context,'arm64')
    return context


if __name__=='__main__':
    import argparse
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--work',type=Path,required=True)
    args=parser.parse_args();root=Path(__file__).resolve().parent.parent
    context=build(root,args.work)
    print(json.dumps(verify(root,context,'arm64')))
