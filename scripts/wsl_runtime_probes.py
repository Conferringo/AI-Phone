"""Real offline runtime probes; no simulator output or public credential evidence."""
import hashlib
import http.cookiejar
import json
from pathlib import Path
import struct
import subprocess
import time
import urllib.request
import urllib.error
import zlib


def execute(root,args,**kwargs):
    return subprocess.run(list(map(str,args)),cwd=root,check=True,capture_output=True,timeout=kwargs.pop('timeout',120),**kwargs).stdout


def request(opener,url,*,token=None,data=None):
    headers={}
    if url.startswith('http://127.0.0.1:17860/') and data is not None:headers['origin']='http://127.0.0.1:17860'
    if token:headers['authorization']='Bearer '+token
    if data is not None:headers['content-type']='application/x-protobuf'
    return opener.open(urllib.request.Request(url,data=data,headers=headers),timeout=5)


def wait_response(opener,name,url,data,content,token):
    deadline=time.monotonic()+60
    while True:
        try:
            with request(opener,url,token=token if data is not None else None,data=data) as response:
                if response.status!=200 or content not in response.headers.get('content-type',''):raise ValueError(name+' returned an invalid health response')
                body=response.read(1024**2)
                if not body and content!='application/x-protobuf':raise ValueError(name+' returned an empty health response')
            return
        except OSError:
            if time.monotonic()>=deadline:raise ValueError(name+' readiness timed out') from None
            time.sleep(.25)


def ready(root):
    opener=urllib.request.build_opener(urllib.request.ProxyHandler({}))
    token=(root/'data/assistant/token').read_text().strip()
    targets=[('assistant','http://127.0.0.1:17863/v1/setup/status',b'','application/x-protobuf'),('web','http://127.0.0.1:17860/phone',None,'text/html'),('channels','http://127.0.0.1:17864/v1/list',b'','application/x-protobuf')]
    for name,url,data,content in targets:wait_response(opener,name,url,data,content,token)
    return {name:True for name,_,_,_ in targets}


def database_roles(root):
    from prebuilt_services import launch_spec
    spec=launch_spec(root,'assistant')
    value=json.loads(execute(root,[*spec['argv'],'--database-check'],env={**__import__('os').environ,**spec['environment']}))
    if value.get('runtime')!='ai_phone_runtime' or value.get('privileged') is not False or value.get('ddl') is not False:
        raise ValueError('Application database role was not restricted')
    if execute(root,['docker','exec','ai-phone-redis-1','redis-cli','ping']).strip()!=b'PONG':raise ValueError('Redis did not answer')
    return {'database_roles':True,'redis':True}


def png_chunks(data):
    if not data.startswith(b'\x89PNG\r\n\x1a\n') or len(data)>16*1024**2:raise ValueError('Invalid PNG frame')
    offset=8
    while offset<len(data):
        if offset+12>len(data):raise ValueError('Truncated PNG chunk')
        size=struct.unpack('!I',data[offset:offset+4])[0];end=offset+12+size
        if end>len(data):raise ValueError('Truncated PNG body')
        kind=data[offset+4:offset+8];body=data[offset+8:end-4]
        if zlib.crc32(kind+body)!=struct.unpack('!I',data[end-4:end])[0]:raise ValueError('PNG checksum differs')
        yield kind,body
        offset=end
    if offset!=len(data):raise ValueError('Invalid PNG trailing bytes')


def png_dimensions(chunks):
    if not chunks or chunks[0][0]!=b'IHDR' or chunks[-1]!=(b'IEND',b''):raise ValueError('Incomplete PNG frame')
    try:width,height,depth,color,compression,filtering,interlace=struct.unpack('!IIBBBBB',chunks[0][1])
    except struct.error:raise ValueError('Invalid PNG dimensions') from None
    if not 1<=width<=8192 or not 1<=height<=8192:raise ValueError('Invalid PNG dimensions')
    if (depth,color,compression,filtering,interlace) not in ((8,2,0,0,0),(8,6,0,0,0)):raise ValueError('Unsupported PNG frame format')
    return width,height,color


def png_pixels(chunks,width,height,color):
    row_bytes=1+width*(3 if color==2 else 4);expected=row_bytes*height
    if expected>32*1024**2:raise ValueError('PNG pixel budget exceeded')
    decoder=zlib.decompressobj()
    try:pixels=decoder.decompress(b''.join(body for kind,body in chunks if kind==b'IDAT'),expected+1)
    except zlib.error:raise ValueError('Invalid PNG pixels') from None
    if len(pixels)!=expected or not decoder.eof:raise ValueError('Invalid PNG pixels')
    if any(pixels[offset]>4 for offset in range(0,expected,row_bytes)):raise ValueError('Invalid PNG row filter')


def frame_evidence(data):
    chunks=list(png_chunks(data));width,height,color=png_dimensions(chunks)
    png_pixels(chunks,width,height,color)
    return {'width':width,'height':height,'sha256':hashlib.sha256(data).hexdigest()}


def desktop_frame(root):
    data=execute(root,['docker','exec','-u','ai','ai-phone-desktop','/usr/local/bin/desktop-bridge','snapshot'])
    value=frame_evidence(data)
    if (value['width'],value['height'])!=(1280,800):raise ValueError('Desktop dimensions differ')
    return value


def desktop_registration(root,nonce,phase):
    import os
    from prebuilt_js import launch_spec
    spec=launch_spec(root,'backend-qualification')
    environment={**os.environ,**spec['environment'],'AI_PHONE_WSL_QUALIFICATION_NONCE':nonce,'AI_PHONE_WSL_QUALIFICATION_PHASE':str(phase)}
    value=json.loads(execute(root,spec['argv'],env=environment,timeout=300))
    if value.get('desktop_registered') is not True or value.get('desktop_observation') is not True:raise ValueError('Desktop API was not verified')
    return value['device_id']


def first_jpeg(response):
    data=b''
    while len(data)<4*1024**2:
        part=response.read(8192)
        if not part:break
        data+=part;start=data.find(b'\xff\xd8');end=data.find(b'\xff\xd9',max(start,0))
        if start>=0 and end>start:return data[start:end+2]
    raise ValueError('Desktop media did not return a complete frame')


def desktop_media(root,device):
    opener=urllib.request.build_opener(urllib.request.ProxyHandler({}),urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))
    with request(opener,'http://127.0.0.1:17860/api/auth/enter',data=b'') as response:
        if response.status!=200:raise ValueError('Local web entry failed')
    with request(opener,'http://127.0.0.1:17860/api/desktop/media?device_id='+device+'&raw=1') as response:
        if response.status!=200 or response.headers.get('content-type','').split(';',1)[0].strip().lower()!='application/octet-stream':raise ValueError('Web desktop media is unavailable')
        frame=first_jpeg(response)
    script='import io,sys;from PIL import Image;im=Image.open(io.BytesIO(sys.stdin.buffer.read()));im.load();im.convert("RGB").save(sys.stdout.buffer,"PNG")'
    png=execute(root,['docker','exec','-i','-u','ai','ai-phone-desktop','python3','-c',script],input=frame)
    frame_evidence(png)
    return True


def work_file(root,nonce,phase):
    script='import sys,json,hashlib;from pathlib import Path;r=json.load(sys.stdin);p=Path("/home/ai/Files")/("qualification-"+r["nonce"]+".txt");b=("AI Phone offline qualification\\n"+r["nonce"]).encode();p.open("xb").write(b) if r["phase"]==1 else None;assert p.read_bytes()==b;print(hashlib.sha256(b).hexdigest())'
    body=json.dumps({'nonce':nonce,'phase':phase}).encode()
    actual=execute(root,['docker','exec','-i','-u','ai','ai-phone-desktop','python3','-c',script],input=body).strip().decode()
    expected=hashlib.sha256(('AI Phone offline qualification\n'+nonce).encode()).hexdigest()
    if actual!=expected:raise ValueError('Desktop work file changed across restart')
    host=root/'data/desktop/home/Files'/('qualification-'+nonce+'.txt')
    if hashlib.sha256(host.read_bytes()).hexdigest()!=expected:raise ValueError('Desktop work file is not in the owned bind directory')
    return True


def database_marker(root,nonce,phase):
    from database import env_values
    env=env_values(root)
    statement=("CREATE TABLE qualification_receipt(value text PRIMARY KEY); INSERT INTO qualification_receipt VALUES ('"+nonce+"'); ") if phase==1 else ''
    statement+='SELECT value FROM qualification_receipt;'
    output=execute(root,['docker','exec','-i','ai-phone-postgres-1','psql','-X','-qAt','-v','ON_ERROR_STOP=1','-U',env['POSTGRES_USER'],'-d',env['POSTGRES_DB']],input=statement.encode()).strip().decode()
    if output!=nonce:raise ValueError('Database marker changed across restart')
    return True
