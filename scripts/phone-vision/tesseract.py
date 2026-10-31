#!/usr/bin/python3
"""Linux OCR compatibility boundary: PNG stdin to the runner's bounded capture."""
import csv
import io
import json
import os
import subprocess
import sys


def recognize(png):
    from PIL import Image
    if not png or len(png)>2097152:raise ValueError('Invalid screenshot size')
    with Image.open(io.BytesIO(png)) as image:
        width,height=image.size
        if image.format!='PNG' or not 0<width<=4096 or not 0<height<=4096:raise ValueError('Invalid screenshot dimensions')
        grid=list(image.convert('L').resize((32,64),Image.Resampling.BILINEAR).getdata())
    result=subprocess.run(['tesseract','stdin','stdout','-l','chi_sim+eng','--psm','11','tsv'],input=png,capture_output=True,timeout=4,env={**os.environ,'OMP_THREAD_LIMIT':'1'})
    if result.returncode or len(result.stdout)>2097152:raise ValueError('OCR did not complete')
    return {'width':width,'height':height,'regions':parse_regions(result.stdout,width,height),'grid':grid}


def parse_regions(data,width,height):
    reader=csv.DictReader(io.StringIO(data.decode('utf-8')),delimiter='\t')
    if reader.fieldnames!=['level','page_num','block_num','par_num','line_num','word_num','left','top','width','height','conf','text']:
        raise ValueError('OCR returned an invalid TSV header')
    regions=[]
    for row in reader:
        if row['level']!='5' or not row['text'].strip():continue
        confidence=float(row['conf'])/100
        if confidence<.7:continue
        left,top,w,h=(int(row[key]) for key in ['left','top','width','height'])
        if not (0<=left<left+w<=width and 0<=top<top+h<=height):continue
        regions.append({'text':row['text'][:400],'confidence':confidence,'bounds':{'left':left,'top':top,'right':left+w,'bottom':top+h}})
        if len(regions)>=200:break
    return regions


if __name__=='__main__':
    from PIL import Image
    try:print(json.dumps(recognize(sys.stdin.buffer.read(2097153)),ensure_ascii=False))
    except (ValueError,OSError,subprocess.TimeoutExpired,Image.DecompressionBombError):
        print('Phone OCR unavailable',file=sys.stderr);raise SystemExit(1)
