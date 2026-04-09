use std::io::{Read, Write as IoWrite};
use std::time::Duration;

use super::{alloc_handle, lock_net, NetHandle};

pub fn listen(host: &str, port: u16) -> Result<u64, String> {
    let addr = format!("{host}:{port}");
    let listener =
        std::net::TcpListener::bind(&addr).map_err(|e| format!("net.listen('{addr}'): {e}"))?;
    Ok(alloc_handle(NetHandle::Listener(listener)))
}

pub fn accept(listener_id: u64) -> Result<u64, String> {
    let listener = {
        let mut state = lock_net();
        match state.handles.remove(&listener_id) {
            Some(NetHandle::Listener(l)) => l,
            Some(other) => {
                state.handles.insert(listener_id, other);
                return Err("net.accept: handle is not a listener".to_string());
            }
            None => return Err("net.accept: invalid listener handle".to_string()),
        }
    };

    let result = listener.accept();

    {
        let mut state = lock_net();
        state.handles.insert(listener_id, NetHandle::Listener(listener));
    }

    match result {
        Ok((stream, _addr)) => Ok(alloc_handle(NetHandle::Stream(stream))),
        Err(e) => Err(format!("net.accept: {e}")),
    }
}

pub fn connect(host: &str, port: u16) -> Result<u64, String> {
    let addr = format!("{host}:{port}");
    let stream =
        std::net::TcpStream::connect(&addr).map_err(|e| format!("net.connect('{addr}'): {e}"))?;
    Ok(alloc_handle(NetHandle::Stream(stream)))
}

pub fn read(stream_id: u64, max_bytes: usize) -> Result<String, String> {
    let mut state = lock_net();
    let handle = state
        .handles
        .get_mut(&stream_id)
        .ok_or_else(|| "net.read: invalid stream handle".to_string())?;

    match handle {
        NetHandle::Stream(stream) => {
            let mut buf = vec![0u8; max_bytes];
            let n = stream.read(&mut buf).map_err(|e| format!("net.read: {e}"))?;
            Ok(String::from_utf8_lossy(&buf[..n]).to_string())
        }
        _ => Err("net.read: handle is not a stream".to_string()),
    }
}

pub fn write(stream_id: u64, data: &str) -> Result<usize, String> {
    let mut state = lock_net();
    let handle = state
        .handles
        .get_mut(&stream_id)
        .ok_or_else(|| "net.write: invalid stream handle".to_string())?;

    match handle {
        NetHandle::Stream(stream) => {
            let n = stream
                .write(data.as_bytes())
                .map_err(|e| format!("net.write: {e}"))?;
            stream
                .flush()
                .map_err(|e| format!("net.write flush: {e}"))?;
            Ok(n)
        }
        _ => Err("net.write: handle is not a stream".to_string()),
    }
}

pub fn set_timeout(stream_id: u64, millis: u64) {
    let state = lock_net();
    if let Some(NetHandle::Stream(stream)) = state.handles.get(&stream_id) {
        let timeout = if millis == 0 {
            None
        } else {
            Some(Duration::from_millis(millis))
        };
        let _ = stream.set_read_timeout(timeout);
        let _ = stream.set_write_timeout(timeout);
    }
}

pub fn peer_addr(stream_id: u64) -> Result<String, String> {
    let state = lock_net();
    match state.handles.get(&stream_id) {
        Some(NetHandle::Stream(s)) => s
            .peer_addr()
            .map(|a| a.to_string())
            .map_err(|e| format!("net.peer_addr: {e}")),
        _ => Err("net.peer_addr: handle is not a stream".to_string()),
    }
}
