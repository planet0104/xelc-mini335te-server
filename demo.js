// 运行: deno run --allow-net demo.js

import { decode, encode } from 'https://deno.land/std@0.118.0/encoding/base64.ts';
import { createClient } from "https://deno.land/x/httpclient@0.0.1/mod.ts";

const client = createClient().baseURL("http://127.0.0.1:8180/");

try{
  // 打开串口
  const res = await client.get('open?port=COM7');

  if(!res.success) throw('串口打开失败');

  //读取卡片ID
  const uid = await client.get('uid');

  console.log('卡片编号:', uid.message);

  //写入字节数组
  console.log('写入...');
  const buffer = new TextEncoder().encode('你好世界！');
  const data_len = buffer.length;
  const base64Str = encode(buffer);
  const wres = await client.get('write?data='+base64Str);
  console.log(wres);

  if(!wres.success) throw('数据写入失败');

  //读取字节数组并解码
  console.log('读取...');
  const rres = await client.get('read?len='+data_len);
  console.log(rres);
  const readBuffer = decode(rres.message);
  const readText = new TextDecoder().decode(readBuffer);
  console.log(readText);

  if(!rres.success) throw('数据读取失败');
}catch(e){
  console.error(e);
}finally{
  //关闭串口
  await client.get('/close');
}