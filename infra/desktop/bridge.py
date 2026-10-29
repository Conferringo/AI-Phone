#!/usr/local/bin/python3
import sys, json, subprocess, time, os, io, base64, re
from pathlib import Path
from PIL import ImageGrab,Image
os.environ['DISPLAY']=':0'
try: os.environ['DBUS_SESSION_BUS_ADDRESS']=Path('/tmp/ai-runtime/dbus-address').read_text()
except FileNotFoundError: pass
def cmd(*args,timeout=8):
    return subprocess.run(args,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=timeout,check=True).stdout
def snapshot():
    image=ImageGrab.grab(xdisplay=':0'); b=io.BytesIO();image.save(b,'PNG');return b.getvalue()
def meta():
    try: title=cmd('xdotool','getactivewindow','getwindowname').decode(errors='replace').strip()
    except Exception:title='Linux desktop'
    return {'width':1280,'height':800,'title':title,'png':base64.b64encode(snapshot()).decode()}
def point(r,x='x',y='y'):
    a,b=int(r.get(x,0)),int(r.get(y,0))
    if not 0<=a<1280 or not 0<=b<800:raise ValueError('坐标超出屏幕')
    return str(a),str(b)
cursor_points=[]
cursor_started=0
def cursor_ack(r):
    if not re.fullmatch(r'[0-9a-f-]{36}',r.get('action_id','')):return
    actual=dict(line.split('=',1) for line in cmd('xdotool','getmouselocation','--shell').decode().splitlines() if '=' in line)
    cursor_points.append({'x':int(actual['X']),'y':int(actual['Y']),'at_ms':round((time.monotonic()-cursor_started)*1000)})
    folder=Path('/home/ai/.aiphone');folder.mkdir(exist_ok=True,mode=0o700)
    temp=folder/(r['action_id']+'.tmp');temp.write_text(json.dumps({'action_id':r['action_id'],'points':cursor_points[-128:]}));temp.chmod(0o600);os.replace(temp,folder/'cursor.json')
def path(value):
    p=(Path('/home/ai/Files')/value).resolve()
    if not p.is_relative_to('/home/ai/Files'):raise ValueError('只允许工作文件目录')
    return p
try:
    op=sys.argv[1]
    if op=='snapshot':sys.stdout.buffer.write(snapshot());sys.exit(0)
    r=json.load(sys.stdin) if op!='observe' else {}
    if op=='observe':out=meta()
    elif op=='zoom':
        image=Image.open(io.BytesIO(base64.b64decode(r['png'],validate=True)))
        x,y,w,h=(int(r[k]) for k in ['x','y','width','height'])
        if x<0 or y<0 or w<32 or h<32 or x+w>image.width or y+h>image.height:raise ValueError('放大区域超出原始观察')
        crop=image.crop((x,y,x+w,y+h));scale=min(2,1280/w,800/h);crop=crop.resize((round(w*scale),round(h*scale)),Image.Resampling.LANCZOS);b=io.BytesIO();crop.save(b,'PNG')
        out={'success':True,'png':base64.b64encode(b.getvalue()).decode(),'scale':scale,'width':crop.width,'height':crop.height}
    elif op=='action':
        kind=r['kind'];cursor_started=time.monotonic()
        if kind=='click':
            cmd('xdotool','mousemove',*point(r));cursor_ack(r);cmd('xdotool','click','--repeat',str(min(2,max(1,int(r.get('clicks',1))))),'--delay','110',str(r.get('button',1)));cursor_ack(r)
        elif kind=='drag':
            x,y=point(r);u,v=point(r,'to_x','to_y');cmd('xdotool','mousemove',x,y,'mousedown','1');cursor_ack(r)
            try:
                for i in range(1,13):cmd('xdotool','mousemove',str(round(int(x)+(int(u)-int(x))*i/12)),str(round(int(y)+(int(v)-int(y))*i/12)));cursor_ack(r);time.sleep(.02)
            finally:cmd('xdotool','mouseup','1')
        elif kind=='scroll':
            cmd('xdotool','mousemove',*point(r));cursor_ack(r);cmd('xdotool','click','--repeat',str(min(8,max(1,abs(int(r.get('amount',3)))))),'--delay','50','5' if int(r.get('amount',3))>0 else '4');cursor_ack(r)
        elif kind=='key':
            key=r['key']
            if not re.fullmatch(r'[A-Za-z0-9_+ -]{1,80}',key):raise ValueError('无效按键')
            cmd('xdotool','key','--clearmodifiers',key)
        elif kind=='text':
            text=r['text']
            if len(text)>16000:raise ValueError('文本过长')
            p=subprocess.Popen(['xclip','-selection','clipboard'],stdin=subprocess.PIPE,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL);p.communicate(text.encode(),timeout=5);cmd('xdotool','key','--clearmodifiers','ctrl+v')
        else:raise ValueError('未知操作')
        time.sleep(.12);out={'success':True,**meta()}
    elif op=='files':
        p=path(r.get('path',''));out={'files':[{'name':v.name,'path':str(v.relative_to('/home/ai/Files')),'directory':v.is_dir(),'bytes':v.stat().st_size} for v in sorted(p.iterdir())][:100]}
    elif op=='run':
        script=r.get('script','')
        if not script or len(script)>24000:raise ValueError('命令为空或过长')
        p=subprocess.run(['timeout','85','bash','-lc',script],cwd='/home/ai/Files',stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=90)
        out={'success':p.returncode==0,'exit_code':p.returncode,'output':p.stdout[-65536:].decode(errors='replace')}
    else:raise ValueError('未知操作')
    print(json.dumps(out,ensure_ascii=False))
except Exception as e:
    print(json.dumps({'success':False,'error':str(e)},ensure_ascii=False));sys.exit(1)
