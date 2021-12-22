
mod ntag;

use log::error;
pub use ntag::{CMD_READ_DATA, CMD_SET_BUZZER, CMD_CLOSE_UID_REPORT, CMD_OPEN_UID_REPORT, CMD_WRITE_DATA, CardType};
use std::thread;
use std::time::Duration;
use std::{sync::{Mutex, mpsc::{Receiver, Sender}}};
use once_cell::sync::Lazy;
use anyhow::{anyhow, Result};

/// 给串口线程发送命令
static SENDER: Lazy<Mutex<Option<Sender<(u8, Vec<u8>)>>>> = Lazy::new(|| { Mutex::new( None ) });
/// 从串口线程接收命令
static RECEIVER: Lazy<Mutex<Option<Receiver<(u8, bool, Vec<u8>)>>>> = Lazy::new(|| { Mutex::new( None ) });

/// 检查串口是否已打开
pub fn is_opened() -> bool{
    match ntag::OPENED.lock(){
        Ok(opend) => {
            *opend
        }
        Err(err) => {
            error!("{:?}", err);
            false
        }
    }
}

/// 获取当前读取到的UID，读取失败时为空
pub use ntag::get_current_uid;

/// 设置是否循环读取UID
pub use ntag::set_loop;

/// 设置是否循环读取UID
pub fn set_loop_sleep(lp: bool, time_ms: u64) -> Result<()>{
    ntag::set_loop(lp)?;
    thread::sleep(Duration::from_millis(time_ms as u64));
    Ok(())
}

/// 打开串口
pub fn open(dev: &str, card_type:CardType, delay: u32, debug: bool) -> Result<()>{
    let (tx, rx) = ntag::open_port(dev.to_string(), card_type, delay as u16, debug)?;
    if let Ok(mut sender) = SENDER.lock(){
        *sender = Some(tx);
    }else{
        return Err(anyhow!("sender锁定失败!"));
    }
    if let Ok(mut recv) = RECEIVER.lock(){
        *recv = Some(rx);
    }else{
        return Err(anyhow!("receiver锁定失败!"));
    }
    Ok(())
}

/// 关闭串口
pub fn close() -> bool{
    ntag::close_port()
}

/// 读取数据
pub fn read_data(len: u8) -> Result<NTAGResult>{
    Ok(send_cmd(CMD_READ_DATA, vec![len as u8])?)
}

/// 设置蜂鸣器
pub fn set_buzzer(data: u8) -> Result<NTAGResult>{
    Ok(send_cmd(CMD_SET_BUZZER, vec![data])?)
}

/// 关闭UID主动上报
fn close_uid_report() -> Result<NTAGResult>{
    Ok(send_cmd_no_resp(CMD_CLOSE_UID_REPORT, vec![])?)
}

/// 打开UID主动上报
pub fn open_uid_report() -> Result<NTAGResult>{
    Ok(send_cmd_no_resp( CMD_OPEN_UID_REPORT, vec![])?)
}

/// 写入数据
pub fn write_data(data: Vec<u8>) -> Result<NTAGResult>{
    Ok(send_cmd( CMD_WRITE_DATA, data)?)
}

/// 发送操作到线程
fn send_cmd_no_resp(cmd: u8, data: Vec<u8>) -> Result<NTAGResult>{
    let sender_lock = SENDER.lock();
    if let Err(err) = sender_lock{
        error!("{:?}", err);
        return Err(anyhow!(format!("{:?}", err)));
    }
    let sender = sender_lock.unwrap();    
    let tx = sender.as_ref().unwrap();
    tx.send((cmd, data))?;
    Ok(( cmd, true, vec![]))
}

/// 发送操作到线程
fn send_cmd(cmd: u8, data: Vec<u8>) -> Result<NTAGResult>{
    let sender_lock = SENDER.lock();
    if let Err(err) = sender_lock{
        error!("{:?}", err);
        return Err(anyhow!(format!("{:?}", err)));
    }
    let sender = sender_lock.unwrap();

    let receiver_lock = RECEIVER.lock();
    if let Err(err) = receiver_lock{
        error!("{:?}", err);
        return Err(anyhow!(format!("{:?}", err)));
    }
    let receiver = receiver_lock.unwrap();

    if receiver.is_none(){
        return Err(anyhow!("receiver为空!"));    
    }
    
    let rx = receiver.as_ref().unwrap();
    let tx = sender.as_ref().unwrap();

    tx.send((cmd, data))?;
    let (cmd, success, data) = rx.recv()?;
    //数据都出来是倒置的
    let data = data.into_iter().rev().collect();
    Ok((cmd, success, data))
}

pub type NTAGResult = (u8, bool, Vec<u8>);