mod ntag;
use log::LevelFilter;
use ntag::CardType;
use structopt::StructOpt;
use tide::Request;
use tide::Response;
use tide::StatusCode;
use tide::prelude::*;
use anyhow::{anyhow, Result};

#[derive(Debug, StructOpt)]
struct Cli {
    port: Option<u32>,
    ip: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenParams {
    port: String,
    card_type:Option<CardType>,
    delay: Option<u32>,
    debug: Option<bool>
}

#[derive(Debug, Deserialize)]
struct WriteParam {
    data: String,
}

#[derive(Debug, Deserialize)]
struct ReadParam {
    len: u8,
}

#[derive(Debug, Deserialize, Serialize)]
struct ServerResponse {
    success: bool,
    message: String,
}

impl ServerResponse{
    fn to_tide_resp(r:ServerResponse) -> Response{
        let mut resp = Response::new(StatusCode::Ok);
        resp.set_content_type("application/json");
        resp.set_body(json!( r ));
        resp
    }
    fn success(message: &str) -> Response{
        ServerResponse::to_tide_resp(ServerResponse{ success:true, message:message.to_string() })
    }
    fn error(message: &str) -> Response{
        ServerResponse::to_tide_resp(ServerResponse{ success:false, message:message.to_string() })
    }
}

macro_rules! resp{
    ($a:expr)=>{
        match $a{
            Ok(s) => Ok(ServerResponse::success(&s)),
            Err(err) => {
                Ok(ServerResponse::error(&format!("{:?}", err)))
            }
        }
    }
}

fn main() -> Result<()>{
    env_logger::Builder::new().filter_level(LevelFilter::Warn).init();
    
    let args = Cli::from_args();
    let port = args.port.clone().unwrap_or(8180);
    let ip = args.ip.unwrap_or(String::from("::"));
    
    async_std::task::block_on(async {
        let mut app = tide::new();
        app.at("/").get(help);
        app.at("/open").get(open);
        app.at("/isopen").get(is_opened);
        app.at("/close").get(close);
        app.at("/uid").get(get_current_uid);
        app.at("/read").get(read_data);
        app.at("/write").get(write_data);
        println!("服务器启动: {}:{}", ip, port);
        app.listen(&format!("{}:{}", ip, port)).await?;
        Ok(())
    })
}

async fn help(_req: Request<()>) -> tide::Result {
    Ok(r#"

    服务器启动:

    可选参数：
    port: 监听端口 默认 8180
    ip: 监听IP地址 默认 ::

    示例:
    xelc-mini335te-server 8180 127.0.0.1


    HTTP API:

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
    
    "#.into())
}

/// HTTP 打开串口
async fn open(req: Request<()>) -> tide::Result {
    resp!(|| -> Result<String>{
        let OpenParams { port, card_type, delay, debug } = req.query().map_err(|err| anyhow!("{:?}", err) )?;
        ntag::open(&port, card_type.unwrap_or(CardType::UltraLight), delay.unwrap_or(300), debug.unwrap_or(false))?;
        Ok(String::from("OK"))
    }())
}

/// HTTP 关闭串口
async fn close(_req: Request<()>) -> tide::Result {
    resp!(|| -> Result<String>{
        ntag::close();
        Ok(String::from("OK"))
    }())
}

/// HTTP 串口是否已打开
async fn is_opened(_req: Request<()>) -> tide::Result {
    resp!(|| -> Result<String>{
        Ok(format!("{}", ntag::is_opened()))
    }())
}

/// HTTP 读取当前卡片UID
async fn get_current_uid(_req: Request<()>) -> tide::Result {
    resp!(|| -> Result<String>{
        match ntag::get_current_uid()?{
            Some(uid) => {
                Ok(hex::encode(&uid).into())
            }
            None => {
                Err(anyhow!("无卡片"))
            }
        }
    }())
}

/// HTTP 读取数据
async fn read_data(req: Request<()>) -> tide::Result {
    resp!(|| -> Result<String>{
        let ReadParam { len } = req.query().map_err(|err| anyhow!("{:?}", err) )?;
        let (cmd, success, data) = ntag::read_data(len)?;
        // warn!("读取:{:?}", data);
        if success{
            Ok(base64::encode(data))
        }else{
            Err(anyhow!("读取失败 cmd={}", cmd))
        }
    }())
}

/// HTTP 写入数据
async fn write_data(req: Request<()>) -> tide::Result {
    resp!(|| -> Result<String>{
        let WriteParam { data } = req.query().map_err(|err| anyhow!("{:?}", err) )?;
        let w = base64::decode(data)?;
        let len = w.len();
        // warn!("写入:{:?}", w);
        let (cmd, success, _data) = ntag::write_data(w)?;
        if success{
            Ok(format!("写入成功 数据长度:{}", len))
        }else{
            Err(anyhow!("写入失败 cmd={}", cmd))
        }
    }())
}