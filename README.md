# XELC-MINI335TE-SERVER

XELC-MINI335TE 读卡器串口HTTP服务

## 读卡器与USB-TTL链接方式

XELC-MINI335TE -> USB-TTL

```
V -> 5V
R -> TXD
T -> RXD
G -> GND
```

## 启动服务器

```
可选参数：
port: 监听端口 默认 8180
ip: 监听IP地址 默认 ::

示例:
xelc-mini335te-server 8180 127.0.0.1
```

## HTTP接口

```
/open?port=COM4 打开串口

可选参数：
    card_type： Mifare, UltraLight, CPU, ISO14443B, ISO15693, Other
    delay: 读取频率 默认 300 (毫秒)
    debug: 调试输出 默认 false

/close 关闭串口

/isopen 检查串口是否打开

/uid 读取卡片UID

/write?data= 写入数据 data是字节数组转base64的字符串

/read?len= 读取数据 len是要读取的字节长度，读取后转换成base64字符串返回
```

## 客户端链接

```javascript
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
```

```Java
//Java

public class Demo{
    public static void main(String[] args) throws IOException {
        String base = "http://127.0.0.1:8180";

        System.out.println("打开串口..");
        System.out.println(get(base+"/open?port=COM7"));

        System.out.println("读取卡片ID");
        System.out.println(get(base+"/uid"));

        byte[] data = new byte[]{ 64, 32 };
        String dataStr = Base64.getEncoder().encodeToString(data);
        System.out.println("写入数据 base64="+dataStr);
        System.out.println(get(base+"/write?data="+dataStr));

        System.out.println("读取数据");
        String res = get(base+"/read?len="+data.length);
        if(res.contains(dataStr)){
            System.out.println("读取成功!");
        }

        System.out.println("关闭串口..");
        System.out.println(get(base+"/close"));
    }

    static String get(String u) throws IOException {
        URL url = new URL(u);
        HttpURLConnection con = (HttpURLConnection) url.openConnection();
        con.setRequestMethod("GET");
        con.connect();
        BufferedReader in = new BufferedReader(new InputStreamReader(con.getInputStream()));
        String inputLine;
        StringBuilder content = new StringBuilder();
        while ((inputLine = in.readLine()) != null) {
            content.append(inputLine);
        }
        in.close();
        con.disconnect();
        return content.toString();
    }
}
```



