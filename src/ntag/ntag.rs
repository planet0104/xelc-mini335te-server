use anyhow::{anyhow, Result};
use log::{error, info, warn};
use serde::Deserialize;
use serialport::SerialPort;
use std::{io::Read, time::{Duration, Instant}};
use crc16::*;
use std::thread;
use std::sync::Mutex;
use std::mem;
use byteorder::{ByteOrder, LittleEndian, WriteBytesExt};
use once_cell::sync::Lazy;
use std::sync::mpsc::{channel, Sender, Receiver};

// 是否关闭串口
pub static OPENED: Lazy<Mutex<bool>> = Lazy::new(|| { Mutex::new(false) });
// 存储当前读取到的UID
static UID: Lazy<Mutex<Option<Vec<u8>>>> = Lazy::new(|| { Mutex::new( None ) });
static LOOPING: Lazy<Mutex<bool>> = Lazy::new(|| { Mutex::new( true ) });

#[derive(Debug, Deserialize)]
pub enum CardType{
    Mifare,
    UltraLight,
    CPU,
    ISO14443B,
    ISO15693,
    Other
}

impl CardType{
    pub fn from_str(tp: &str) -> CardType{
        match tp{
            "Mifare" => CardType::Mifare,
            "UltraLight" => CardType::UltraLight,
            "CPU" => CardType::CPU,
            "ISO14443B" => CardType::ISO14443B,
            "ISO15693" => CardType::ISO15693,
            _ => CardType::Other,
        }
    }
    pub fn from_i32(tp: i32) -> CardType{
        match tp{
            2 => CardType::Mifare,
            4 => CardType::UltraLight,
            8 => CardType::CPU,
            9 => CardType::ISO14443B,
            6 => CardType::ISO15693,
            _ => CardType::Other,
        }
    }

    // pub fn to_i32(&self) -> i32{
    //     match self{
    //         CardType::Mifare => 2,
    //         CardType::UltraLight => 4,
    //         CardType::CPU => 8,
    //         CardType::ISO14443B => 9,
    //         CardType::ISO15693 => 6,
    //         CardType::Other => 0,
    //     }
    // }

    /// 读取UID的功能码
    pub fn fn_code_read_uid(&self) -> u8{
        match self{
            CardType::Mifare => 0x20,
            CardType::UltraLight => 0x40,
            CardType::CPU => 0x80,
            CardType::ISO14443B => 0x90,
            CardType::ISO15693 => 0x60,
            CardType::Other => 0x00,
        }
    }
    /// 读取块数据的功能码
    pub fn fn_code_read_data(&self) -> u8{
        match self{
            CardType::Mifare => 0x21,
            CardType::UltraLight => 0x41,
            CardType::CPU => 0x81,
            CardType::ISO14443B => 0x91,
            CardType::ISO15693 => 0x61,
            CardType::Other => 0x01,
        }
    }

    /// 写入块数据
    pub fn fn_code_write_data(&self) -> u8{
        match self{
            CardType::Mifare => 0x22,
            CardType::UltraLight => 0x42,
            CardType::CPU => 0x81,
            CardType::ISO14443B => 0x92,
            CardType::ISO15693 => 0x62,
            CardType::Other => 0x02,
        }
    }
}

// 设置蜂鸣器功能码
pub const FN_CODE_SET_BUZZER:u8 = 0x05;
/// UID上报设置
pub const FN_CODE_UID_REPORT_SET:u8 = 0x07;

// 状态码

/// 操作成功
pub const ST_CODE_SUCCESS:u8 = 0x00;
/// 数据长度错误
// pub const ST_CODE_DATA_ERROR:u8 = 0x01;
/// CRC校验失败
// pub const ST_CODE_CRC_ERROR:u8 = 0x02;
/// 命令参数错误
// pub const ST_CODE_PARAM_ERROR:u8 = 0x03;
/// 寻卡失败
// pub const ST_CODE_CARD_ERROR:u8 = 0x0B;
/// UID获取失败
// pub const ST_CODE_UID_ERROR:u8 = 0x0C;
/// 读写数据错误
// pub const ST_CODE_READ_WRITE_ERROR:u8 = 0x0F;

//读取UID
// pub const CMD_READ_UID:u8 = 0x00;
//写入数据
pub const CMD_WRITE_DATA: u8 = 0x01;
//读取数据
pub const CMD_READ_DATA: u8 = 0x02;
//设置蜂鸣器
pub const CMD_SET_BUZZER: u8 = 0x03;
//关闭UID主动上报
pub const CMD_CLOSE_UID_REPORT: u8 = 0x04;
//打开UID主动上报
pub const CMD_OPEN_UID_REPORT: u8 = 0x05;

pub const READ_TIMEOUT:u16 = 500;

/// 给数据添加校验码
fn wrap_data(mut data: Vec<u8>) -> Result<Vec<u8>>{
    let cr = State::<XMODEM>::calculate(&data);
    let mut bs = [0u8; mem::size_of::<u16>()];
    bs.as_mut().write_u16::<LittleEndian>(cr)?;
    data.push(bs[0]);
    data.push(bs[1]);
    Ok(data)
}

struct PackageInfo{
    /// 帧头
    pub header: u8,
    /// 包长度 为（功能码 + 状态码 + 数据长度 + 数据 + CRC）的所有字节长度之和，低字节在前，高字节在后；
    pub length: u16,
    /// 功能码
    fn_code: u8,
    /// 状态码
    st_code: u8,
    /// 数据长度(数据字节长度)
    pub data_length: u16,
    /// 数据
    data: Vec<u8>,
    /// 校验码
    pub crc: u16,
}

/// 发送数据包
fn send_package(port:&mut Box<dyn SerialPort>, fn_code: u8, data:&[u8], debug: bool) -> Result<()>{
    let mut pkg = vec![];
    //帧头
    pkg.push(0x24);
    //包长度为（  功能码  +  状态码  +  数据长度  +  数据  + CRC  ）的所有字节长度之和
    let pkg_len:u16 = 1 + 2 + data.len() as u16 + 2;
    let mut pkg_len_bytes = vec![];
    pkg_len_bytes.write_u16::<LittleEndian>(pkg_len)?;
    pkg.extend_from_slice(&pkg_len_bytes);
    //功能码
    pkg.push(fn_code);
    //数据长度
    let data_len = data.len() as u16;
    let mut data_len_bytes = vec![];
    data_len_bytes.write_u16::<LittleEndian>(data_len)?;
    pkg.extend_from_slice(&data_len_bytes);
    //数据
    pkg.extend_from_slice(data);
    //crc
    let send_data = wrap_data(pkg.to_vec())?;
    if debug{
        warn!("发送:{:X?}", send_data);
    }
    port.write_all(&send_data)?;
    Ok(())
}

///u16转字节
fn u16_to_slice<'a>(v: u16) -> Result<Vec<u8>>{
    let mut u16_bytes = Vec::with_capacity(2);
    u16_bytes.write_u16::<LittleEndian>(v)?;
    Ok(u16_bytes)
}

///从串口中读取一个字节
fn read_u8(port:&mut Box<dyn SerialPort>, mut try_times: u8) -> Result<u8>{
    let buf = &mut[0u8];
    let mut success = false;
    loop{
        match port.read(buf){
            Err(_err) => {
                //可能会读取超时，但是不报错，继续尝试读取
                // error!("{:?}", err);
            }
            Ok(len) => {
                if len == 1{
                    success = true;
                    break;
                }
            }
        }
        //延迟一会儿
        thread::sleep(Duration::from_millis(1));
        if try_times == 0{
            break;
        }
        try_times -= 1;
    }
    if success{
        Ok(buf[0])
    }else{
        Err(anyhow!("read_u8 超时"))
    }
}
///从串口中读取两个字节
fn read_u16(port:&mut Box<dyn SerialPort>, mut try_times: u8) -> Result<u16>{
    let buf = &mut[0u8; 2];
    let mut success = false;
    loop{
        if port.read(buf)? == 2{
            success = true;
            break;
        }
        if try_times == 0{
            break;
        }
        try_times -= 1;
    }
    if success{
        let v = LittleEndian::read_u16(buf);
        Ok(v)
    }else{
        Err(anyhow!("read_u16 超时"))
    }
}
///从串口中读取一组字节
fn read_bytes(port:&mut Box<dyn SerialPort>, len: usize, mut try_times: u8) -> Result<Vec<u8>>{
    if len == 0{
        return Ok(vec![]);
    }
    let mut buf = vec![0u8; len];
    let mut idx = 0;
    let mut success = false;
    loop{
        let read_len = port.read(&mut buf[idx..])?;
        idx += read_len;
        if idx == read_len{
            success = true;
            break;
        }
        if read_len == 0{
            if try_times == 0{
                break;
            }
            try_times -= 1;
        }
    }
    if success{
        Ok(buf)
    }else{
        Err(anyhow!("read_bytes 超时"))
    }
}

///从串口读取数据包
fn read_package(port:&mut Box<dyn SerialPort>, debug: bool) -> Result<PackageInfo>{
    //每次读取字节尝试3次
    let try_times = 3;
    let mut pkg_buf = vec![];
    //等待帧头
    let start = Instant::now();
    while read_u8(port, try_times)? != 36{
        thread::sleep(Duration::from_millis(1));
        if start.elapsed().as_millis() as u16 > READ_TIMEOUT{
            return Err( anyhow!("帧头读取超时") );
        }
    }
    pkg_buf.push(0x24);
    //读取包长度
    let length = read_u16(port, try_times)?;
    pkg_buf.extend_from_slice(&u16_to_slice(length)?);
    //读取功能码
    let fn_code = read_u8(port, try_times)?;
    pkg_buf.push(fn_code);
    //读取状态码
    let st_code = read_u8(port, try_times)?;
    pkg_buf.push(st_code);
    //读取数据长度
    let data_length = read_u16(port, try_times)?;
    pkg_buf.extend_from_slice(&u16_to_slice(data_length)?);
    //读取数据
    let data:Vec<u8> = read_bytes(port, data_length as usize, try_times)?;
    pkg_buf.extend_from_slice(&data);
    //读取校验码
    let crc = read_u16(port, try_times)?;
    let my_crc = State::<XMODEM>::calculate(&pkg_buf);
    if crc != my_crc{
        return Err(anyhow!("数据校验失败"));
    }
    if debug{
        warn!("接收 => 包长度:{} 功能码:{:X} 状态码:{:X} 数据长度:{} 数据:{} 校验码:{:#02X} 本地校验码:{:#02X}",
        length, fn_code, st_code, data_length, hex::encode(&data), crc, my_crc);
    }

    let pkg_info = PackageInfo{
        header: 0x24,
        length,
        fn_code,
        st_code,
        data_length,
        data,
        crc,
    };
    Ok(pkg_info)
}

/// 写入4个字节并等待
fn write_page(port:&mut Box<dyn SerialPort>, card_type:&CardType, page: u8, data:&[u8; 4], debug: bool) -> Result<PackageInfo>{
    let mut snd:Vec<u8> = Vec::with_capacity(5);
    snd.push(page);
    snd.extend(data);
    Ok(send_package_and_wait(port, card_type.fn_code_write_data(), &snd, debug)?)
}

/// 关闭串口
pub fn close_port() -> bool{
    match OPENED.lock(){
        Ok(mut opened) => {
            *opened = false;
            true
        }
        Err(err) => {
            error!("{:?}", err);
            false
        }
    }
}

/// 获取当前读取到的UID，读取失败时为空
pub fn get_current_uid() -> Result<Option<Vec<u8>>>{
    match UID.lock(){
        Ok(uid) => Ok(uid.clone()),
        Err(err) => {
            let err = format!("UID lock失败:{:?}", err);
            Err(anyhow!(err))
        }
    }
}

// 开始、停止循环读取UID(在做其他命令的时候要先停止，停止后，至少要过一定时间才生效)
pub fn set_loop(lp: bool) -> Result<()>{
    match LOOPING.lock(){
        Ok(mut l) => {
            *l = lp;
            Ok(())
        }
        Err(err) => {
            let err = format!("LOOPING lock失败:{:?}", err);
            Err(anyhow!(err))
        }
    }
}

/*

loop{
    -> 同步获取UID 发送指令，等待返回
    -> 同步写入
    -> 同步读取
}

 */

 /// 同步发送消息，并等待应答
 fn send_package_and_wait(port:&mut Box<dyn SerialPort>, fn_code: u8, data:&[u8], debug: bool) -> Result<PackageInfo>{
    //发送
    send_package(port, fn_code, &data, debug)?;
    //尝试3次读取对应的回应
    let mut count = 0;
    loop{
        let pkg = read_package(port, debug)?;
        if pkg.fn_code == fn_code{
            return Ok(pkg);
        }
        count += 1;
        if count > 3{
            break;
        }
    }
    Err(anyhow!("功能码:{} 应答超时", fn_code))
 }

/// 启动检测线程
// 返回: Sender
// 返回: Receiver
pub fn open_port(dev:String, card_type:CardType, query_delay: u16, debug: bool) -> Result<(Sender<(u8, Vec<u8>)>, Receiver<(u8, bool, Vec<u8>)>)>{

    info!("打开串口 {} UID检测频率:{}ms card_type={:?}", dev, query_delay, card_type);

    let mut port = serialport::new(dev.clone(), 115_200)
    .timeout(Duration::from_millis(100))
    .open()?;

    match OPENED.lock(){
        Ok(mut opened) => *opened = true,
        Err(err) => error!("{:?}", err)
    };

    info!("串口打开成功 {:?}", port.name());

    //注意，两条指令不能一起发

    //关闭蜂鸣器: 24 06 00 05 01 00 04
    // send_hex(&mut port,"24060005010004")?;
    //关闭主动上报
    // send_package(&mut port, FN_CODE_UID_REPORT_SET, &[0xAA])?;
    //主动上报UID
    // send_package(&mut port, FN_CODE_UID_REPORT_SET, &[0x55]).unwrap();
    
    let mut delay_time = Instant::now();
    
    let (port_tx, port_rx) = channel();
    let (user_tx, user_rx) = channel();
    thread::spawn(move || {
        loop{
            if let Ok(opened) = OPENED.try_lock(){
                if !*opened{
                    break;
                }
            }

            //每隔一定时间发送一次获取UID指令
            if delay_time.elapsed().as_millis() as u16 >= query_delay{
                delay_time = Instant::now();
                match send_package_and_wait(&mut port, card_type.fn_code_read_uid(), &[], debug){
                    Ok(pkg) => {
                        if pkg.st_code != ST_CODE_SUCCESS{
                            if debug{
                                error!("FN_CODE_READ_UID st_code={}", pkg.st_code);
                            }
                            match UID.lock(){
                                Ok(mut uid) => *uid = None,
                                Err(err) => error!("UID lock失败:{:?}", err)
                            };
                            // if let Err(err) = crate::notify_uid(CMD_READ_UID, false, vec![]){
                            //     error!("UID通知失败:{}", err);
                            // }
                        }else{
                            if debug{
                                warn!("UID读取成功:{}", hex::encode(&pkg.data));
                            }
                            match UID.lock(){
                                Ok(mut uid) => *uid = Some(pkg.data.clone()),
                                Err(err) => error!("UID lock失败:{:?}", err)
                            };
                            // if let Err(err) = crate::notify_uid(CMD_READ_UID, true, pkg.data.clone()){
                            //     error!("UID通知失败:{}", err);
                            // }
                        }
                    }
                    Err(err) => {
                        match UID.lock(){
                            Ok(mut uid) => *uid = None,
                            Err(err) => error!("UID lock失败:{:?}", err)
                        };
                        error!("UID读取失败 {:?}", err);
                        // if let Err(err) = crate::notify_uid(CMD_READ_UID, false, vec![]){
                        //     error!("UID通知失败:{}", err);
                        // }
                    }
                }
            }

            //接收要发送的命令
            if let Ok((cmd, data)) = port_rx.try_recv(){
                let mut data:Vec<u8> = data;
                //写入块数据
                if cmd == CMD_WRITE_DATA{
                    //同步写入每一个数据块
                    let mut page = 4;//4~39
                    let mut write_success = true;
                    while data.len()>0{
                        let byte4 = &mut [0u8; 4];
                        if let Some(b1) = data.pop(){
                            byte4[0] = b1;
                        }
                        if let Some(b2) = data.pop(){
                            byte4[1] = b2;
                        }
                        if let Some(b3) = data.pop(){
                            byte4[2] = b3;
                        }
                        if let Some(b4) = data.pop(){
                            byte4[3] = b4;
                        }
                        match write_page(&mut port, &card_type, page, byte4, debug){
                            Ok(pkg) => {
                                if pkg.st_code != ST_CODE_SUCCESS{
                                    error!("CMD_WRITE_DATA st_code={}", pkg.st_code);
                                    write_success = false;
                                    break;
                                }
                            }
                            Err(err) => {
                                info!("CMD_WRITE_DATA {:?}", err);
                                write_success = false;
                                break;
                            }
                        }
                        page += 1;
                        if page>=39{
                            break;
                        }
                    }
                    //返回写入结果
                    if let Err(err) = user_tx.send((CMD_WRITE_DATA, write_success, vec![])){
                        error!("消息 发送失败: CMD_WRITE_DATA {:?}", err);
                    }
                }else if cmd == CMD_READ_DATA{
                    let total_read_len = data[0] as usize;
                    let mut data_read = vec![];
                    //从第4页开始读取
                    let mut page_index = 4;
                    let mut read_success = true;
                    while data_read.len() < total_read_len{
                        let fn_code_read_data = card_type.fn_code_read_data();
                        match send_package_and_wait(&mut port, fn_code_read_data, &[page_index], debug){
                            Ok(pkg) => {
                                if pkg.st_code != ST_CODE_SUCCESS{
                                    error!("CMD_READ_DATA st_code={} fn_code_read_data={:X}", pkg.st_code, fn_code_read_data);
                                    read_success = false;
                                    break;
                                }else{
                                    let last_count = total_read_len - data_read.len();
                                    if last_count>=4{
                                        data_read.extend(pkg.data);
                                    }else{
                                        data_read.extend(&pkg.data[0..last_count]);
                                    }
                                }
                            }
                            Err(err) => {
                                error!("FN_CODE_READ_DATA {:?}", err);
                                read_success = false;
                                break;
                            }
                        }
                        page_index += 1;
                        //最多读39页
                        if page_index>39{
                            if data_read.len() != total_read_len{
                                read_success = false;
                            }
                            break;
                        }
                    }
                    //通知读取结果
                    if let Err(err) = user_tx.send((CMD_READ_DATA, read_success, data_read)){
                        error!("end_read_data 发送失败: CMD_READ_DATA {:?}", err);
                    }
                }else if cmd == CMD_SET_BUZZER {
                    let mut success = false;
                    if data.len() == 0{
                        error!("蜂鸣器设置失败 数据为空 data={:?}", data);
                    }else{
                        match send_package_and_wait(&mut port, FN_CODE_SET_BUZZER, &[data[0]], debug){
                            Ok(pkg) => {
                                if pkg.st_code == ST_CODE_SUCCESS{
                                    success = true;
                                }else{
                                    error!("蜂鸣器设置失败 st_code={}", pkg.st_code);
                                }
                            }
                            Err(err) => {
                                error!("蜂鸣器设置失败{:?}", err);
                            }
                        }
                    }
                    if let Err(err) = user_tx.send((FN_CODE_SET_BUZZER, success, vec![])){
                        error!("发送失败: FN_CODE_SET_BUZZER {:?}", err);
                    }
                }else if cmd == CMD_CLOSE_UID_REPORT {
                    match send_package(&mut port, FN_CODE_UID_REPORT_SET, &[0xAA], debug){
                        Ok(_) => error!("FN_CODE_UID_REPORT_SET 设置成功"),
                        Err(err) => error!("FN_CODE_UID_REPORT_SET 设置失败 {:?}", err),
                    };
                }else if cmd == CMD_OPEN_UID_REPORT {
                    match send_package(&mut port, FN_CODE_UID_REPORT_SET, &[0x55], debug){
                        Ok(_) => error!("FN_CODE_UID_REPORT_SET 设置成功"),
                        Err(err) => error!("FN_CODE_UID_REPORT_SET 设置失败 {:?}", err),
                    };
                }
            }
            thread::sleep(Duration::from_millis(1));
        }
        
        info!("串口关闭 {}", dev);
    });

    Ok((port_tx, user_rx))
}