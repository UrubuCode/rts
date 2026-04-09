use super::{alloc_handle, lock_net, NetHandle};

pub fn bind(host: &str, port: u16) -> Result<u64, String> {
    let addr = format!("{host}:{port}");
    let socket =
        std::net::UdpSocket::bind(&addr).map_err(|e| format!("net.udp_bind('{addr}'): {e}"))?;
    Ok(alloc_handle(NetHandle::Udp(socket)))
}

pub fn send_to(socket_id: u64, data: &str, host: &str, port: u16) -> Result<usize, String> {
    let state = lock_net();
    match state.handles.get(&socket_id) {
        Some(NetHandle::Udp(socket)) => {
            let addr = format!("{host}:{port}");
            socket
                .send_to(data.as_bytes(), &addr)
                .map_err(|e| format!("net.udp_send_to: {e}"))
        }
        _ => Err("net.udp_send_to: handle is not a UDP socket".to_string()),
    }
}

pub fn recv_from(socket_id: u64, max_bytes: usize) -> Result<(String, String), String> {
    let state = lock_net();
    match state.handles.get(&socket_id) {
        Some(NetHandle::Udp(socket)) => {
            let mut buf = vec![0u8; max_bytes];
            let (n, addr) = socket
                .recv_from(&mut buf)
                .map_err(|e| format!("net.udp_recv_from: {e}"))?;
            let data = String::from_utf8_lossy(&buf[..n]).to_string();
            Ok((data, addr.to_string()))
        }
        _ => Err("net.udp_recv_from: handle is not a UDP socket".to_string()),
    }
}

pub fn send(socket_id: u64, data: &str) -> Result<usize, String> {
    let state = lock_net();
    match state.handles.get(&socket_id) {
        Some(NetHandle::Udp(socket)) => socket
            .send(data.as_bytes())
            .map_err(|e| format!("net.udp_send: {e}")),
        _ => Err("net.udp_send: handle is not a UDP socket".to_string()),
    }
}

pub fn connect(socket_id: u64, host: &str, port: u16) -> Result<(), String> {
    let state = lock_net();
    match state.handles.get(&socket_id) {
        Some(NetHandle::Udp(socket)) => {
            let addr = format!("{host}:{port}");
            socket
                .connect(&addr)
                .map_err(|e| format!("net.udp_connect: {e}"))
        }
        _ => Err("net.udp_connect: handle is not a UDP socket".to_string()),
    }
}
