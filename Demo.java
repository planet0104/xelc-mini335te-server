import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.net.HttpURLConnection;
import java.net.URL;
import java.util.Base64;

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