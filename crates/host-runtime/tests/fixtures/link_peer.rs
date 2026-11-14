//! Standalone native-process fixture. It implements only the small test wire
//! envelope, has no networking/device access and is never a packaged executable.
use std::{fs,io::{self,Read,Write},path::Path};

fn varint(bytes:&mut &[u8])->u64 {
    let mut value=0;let mut shift=0;
    loop {let byte=bytes[0];*bytes=&bytes[1..];value|=u64::from(byte&127)<<shift;if byte<128{return value;}shift+=7;assert!(shift<64);}
}
fn fields(mut bytes:&[u8])->std::collections::BTreeMap<u64,Vec<u8>> {
    let mut result=std::collections::BTreeMap::new();
    while !bytes.is_empty(){let key=varint(&mut bytes);let value=match key&7 {
        0=>{let value=varint(&mut bytes);encode(value)},
        2=>{let length=varint(&mut bytes) as usize;let value=bytes[..length].to_vec();bytes=&bytes[length..];value},
        _=>panic!("unexpected test wire type"),
    };result.insert(key>>3,value);}
    result
}
fn encode(mut value:u64)->Vec<u8>{let mut bytes=Vec::new();while value>=128{bytes.push((value as u8)|128);value>>=7;}bytes.push(value as u8);bytes}
fn string(key:u64,value:&[u8])->Vec<u8>{let mut bytes=encode(key*8+2);bytes.extend(encode(value.len() as u64));bytes.extend(value);bytes}
fn number(key:u64,value:&[u8])->Vec<u8>{let mut bytes=encode(key*8);bytes.extend(value);bytes}
fn main(){
    let args:Vec<_>=std::env::args().collect();let root=Path::new(&args[1]);let mode=&args[2];
    let mut input=io::stdin();let mut length=[0;4];input.read_exact(&mut length).unwrap();
    let length=u32::from_be_bytes(length) as usize;assert!(length<=16384);
    let mut bytes=vec![0;length];input.read_exact(&mut bytes).unwrap();
    let outer=fields(&bytes);assert_eq!(outer[&5],[1]);
    let config=fields(&outer[&1]);
    fs::write(root.join("endpoint"),&config[&5]).unwrap();
    fs::write(root.join("input-consumed"),std::process::id().to_string()).unwrap();
    if mode!="wait" {
        let mut reply=Vec::new();reply.extend(number(1,&[1]));reply.extend(number(2,&[2]));
        reply.extend(string(3,if mode=="wrong"{b"wrong-pair"}else{&config[&3]}));reply.extend(string(4,&config[&5]));
        reply.extend(number(5,config.get(&6).map(Vec::as_slice).unwrap_or(&[0xbd,0x75])));
        reply.extend(number(6,config.get(&7).map(Vec::as_slice).unwrap_or(&[0xee,0x42])));
        let mut output=io::stdout();output.write_all(&(reply.len() as u32).to_be_bytes()).unwrap();output.write_all(&reply).unwrap();output.flush().unwrap();
    }
    let mut rest=Vec::new();input.read_to_end(&mut rest).unwrap();assert!(rest.is_empty());
    fs::write(root.join("owner-eof"),b"drained").unwrap();
    std::process::exit(if mode=="exit7"{7}else{0});
}
