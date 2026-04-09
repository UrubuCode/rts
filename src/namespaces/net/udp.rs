use std::net::{Ipv4Addr, UdpSocket};
use std::time::Duration;

use crate::namespaces::lang::JsValue;
use crate::namespaces::{arg_to_string, arg_to_u64, arg_to_usize, DispatchOutcome};

use super::common::{lock_net_state, result_err, result_ok};

// UdpSocket functions
pub fn udp_bind(args: &[JsValue]) -> DispatchOutcome {
    let addr_str = arg_to_string(args, 0);

    match UdpSocket::bind(&addr_str) {
        Ok(socket) => {
            let handle = lock_net_state().insert_udp_socket(socket);
            DispatchOutcome::Value(result_ok(JsValue::Number(handle as f64)))
        }
        Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
    }
}

pub fn udp_connect(args: &[JsValue]) -> DispatchOutcome {
    let handle = arg_to_u64(args, 0);
    let addr_str = arg_to_string(args, 1);
    let mut state = lock_net_state();

    if let Some(socket) = state.udp_sockets.get_mut(&handle) {
        match socket.connect(&addr_str) {
            Ok(()) => DispatchOutcome::Value(result_ok(JsValue::Undefined)),
            Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
        }
    } else {
        DispatchOutcome::Value(result_err("Invalid socket handle".to_string()))
    }
}

pub fn udp_send(args: &[JsValue]) -> DispatchOutcome {
    let handle = arg_to_u64(args, 0);
    let data = arg_to_string(args, 1);
    let mut state = lock_net_state();

    if let Some(socket) = state.udp_sockets.get_mut(&handle) {
        match socket.send(data.as_bytes()) {
            Ok(n) => DispatchOutcome::Value(result_ok(JsValue::Number(n as f64))),
            Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
        }
    } else {
        DispatchOutcome::Value(result_err("Invalid socket handle".to_string()))
    }
}

pub fn udp_recv(args: &[JsValue]) -> DispatchOutcome {
    let handle = arg_to_u64(args, 0);
    let max_bytes = arg_to_usize(args, 1).max(1).min(65536);
    let mut state = lock_net_state();

    if let Some(socket) = state.udp_sockets.get_mut(&handle) {
        let mut buffer = vec![0; max_bytes];
        match socket.recv(&mut buffer) {
            Ok(n) => {
                buffer.truncate(n);
                let data = String::from_utf8_lossy(&buffer).to_string();
                DispatchOutcome::Value(result_ok(JsValue::String(data)))
            }
            Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
        }
    } else {
        DispatchOutcome::Value(result_err("Invalid socket handle".to_string()))
    }
}

pub fn udp_send_to(args: &[JsValue]) -> DispatchOutcome {
    let handle = arg_to_u64(args, 0);
    let data = arg_to_string(args, 1);
    let addr_str = arg_to_string(args, 2);
    let mut state = lock_net_state();

    if let Some(socket) = state.udp_sockets.get_mut(&handle) {
        match socket.send_to(data.as_bytes(), &addr_str) {
            Ok(n) => DispatchOutcome::Value(result_ok(JsValue::Number(n as f64))),
            Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
        }
    } else {
        DispatchOutcome::Value(result_err("Invalid socket handle".to_string()))
    }
}

pub fn udp_recv_from(args: &[JsValue]) -> DispatchOutcome {
    let handle = arg_to_u64(args, 0);
    let max_bytes = arg_to_usize(args, 1).max(1).min(65536);
    let mut state = lock_net_state();

    if let Some(socket) = state.udp_sockets.get_mut(&handle) {
        let mut buffer = vec![0; max_bytes];
        match socket.recv_from(&mut buffer) {
            Ok((n, addr)) => {
                buffer.truncate(n);
                let data = String::from_utf8_lossy(&buffer).to_string();
                let message = JsValue::Object([
                    ("data".to_string(), JsValue::String(data)),
                    ("addr".to_string(), JsValue::String(addr.to_string())),
                ].into_iter().collect());
                DispatchOutcome::Value(result_ok(message))
            }
            Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
        }
    } else {
        DispatchOutcome::Value(result_err("Invalid socket handle".to_string()))
    }
}

pub fn udp_local_addr(args: &[JsValue]) -> DispatchOutcome {
    let handle = arg_to_u64(args, 0);
    let state = lock_net_state();

    if let Some(socket) = state.udp_sockets.get(&handle) {
        match socket.local_addr() {
            Ok(addr) => DispatchOutcome::Value(result_ok(JsValue::String(addr.to_string()))),
            Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
        }
    } else {
        DispatchOutcome::Value(result_err("Invalid socket handle".to_string()))
    }
}

pub fn udp_peer_addr(args: &[JsValue]) -> DispatchOutcome {
    let handle = arg_to_u64(args, 0);
    let state = lock_net_state();

    if let Some(socket) = state.udp_sockets.get(&handle) {
        match socket.peer_addr() {
            Ok(addr) => DispatchOutcome::Value(result_ok(JsValue::String(addr.to_string()))),
            Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
        }
    } else {
        DispatchOutcome::Value(result_err("Invalid socket handle".to_string()))
    }
}

pub fn udp_set_read_timeout(args: &[JsValue]) -> DispatchOutcome {
    let handle = arg_to_u64(args, 0);
    let timeout_ms = arg_to_u64(args, 1);
    let mut state = lock_net_state();

    if let Some(socket) = state.udp_sockets.get_mut(&handle) {
        let timeout = if timeout_ms == 0 {
            None
        } else {
            Some(Duration::from_millis(timeout_ms))
        };

        match socket.set_read_timeout(timeout) {
            Ok(()) => DispatchOutcome::Value(result_ok(JsValue::Undefined)),
            Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
        }
    } else {
        DispatchOutcome::Value(result_err("Invalid socket handle".to_string()))
    }
}

pub fn udp_set_write_timeout(args: &[JsValue]) -> DispatchOutcome {
    let handle = arg_to_u64(args, 0);
    let timeout_ms = arg_to_u64(args, 1);
    let mut state = lock_net_state();

    if let Some(socket) = state.udp_sockets.get_mut(&handle) {
        let timeout = if timeout_ms == 0 {
            None
        } else {
            Some(Duration::from_millis(timeout_ms))
        };

        match socket.set_write_timeout(timeout) {
            Ok(()) => DispatchOutcome::Value(result_ok(JsValue::Undefined)),
            Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
        }
    } else {
        DispatchOutcome::Value(result_err("Invalid socket handle".to_string()))
    }
}

pub fn udp_set_broadcast(args: &[JsValue]) -> DispatchOutcome {
    let handle = arg_to_u64(args, 0);
    let broadcast = matches!(args.get(1), Some(JsValue::Bool(true)));
    let mut state = lock_net_state();

    if let Some(socket) = state.udp_sockets.get_mut(&handle) {
        match socket.set_broadcast(broadcast) {
            Ok(()) => DispatchOutcome::Value(result_ok(JsValue::Undefined)),
            Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
        }
    } else {
        DispatchOutcome::Value(result_err("Invalid socket handle".to_string()))
    }
}

pub fn udp_broadcast(args: &[JsValue]) -> DispatchOutcome {
    let handle = arg_to_u64(args, 0);
    let state = lock_net_state();

    if let Some(socket) = state.udp_sockets.get(&handle) {
        match socket.broadcast() {
            Ok(broadcast) => DispatchOutcome::Value(result_ok(JsValue::Bool(broadcast))),
            Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
        }
    } else {
        DispatchOutcome::Value(result_err("Invalid socket handle".to_string()))
    }
}

pub fn udp_set_multicast_loop_v4(args: &[JsValue]) -> DispatchOutcome {
    let handle = arg_to_u64(args, 0);
    let multicast_loop_v4 = matches!(args.get(1), Some(JsValue::Bool(true)));
    let mut state = lock_net_state();

    if let Some(socket) = state.udp_sockets.get_mut(&handle) {
        match socket.set_multicast_loop_v4(multicast_loop_v4) {
            Ok(()) => DispatchOutcome::Value(result_ok(JsValue::Undefined)),
            Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
        }
    } else {
        DispatchOutcome::Value(result_err("Invalid socket handle".to_string()))
    }
}

pub fn udp_multicast_loop_v4(args: &[JsValue]) -> DispatchOutcome {
    let handle = arg_to_u64(args, 0);
    let state = lock_net_state();

    if let Some(socket) = state.udp_sockets.get(&handle) {
        match socket.multicast_loop_v4() {
            Ok(multicast_loop_v4) => DispatchOutcome::Value(result_ok(JsValue::Bool(multicast_loop_v4))),
            Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
        }
    } else {
        DispatchOutcome::Value(result_err("Invalid socket handle".to_string()))
    }
}

pub fn udp_set_multicast_ttl_v4(args: &[JsValue]) -> DispatchOutcome {
    let handle = arg_to_u64(args, 0);
    let multicast_ttl_v4 = arg_to_usize(args, 1) as u32;
    let mut state = lock_net_state();

    if let Some(socket) = state.udp_sockets.get_mut(&handle) {
        match socket.set_multicast_ttl_v4(multicast_ttl_v4) {
            Ok(()) => DispatchOutcome::Value(result_ok(JsValue::Undefined)),
            Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
        }
    } else {
        DispatchOutcome::Value(result_err("Invalid socket handle".to_string()))
    }
}

pub fn udp_multicast_ttl_v4(args: &[JsValue]) -> DispatchOutcome {
    let handle = arg_to_u64(args, 0);
    let state = lock_net_state();

    if let Some(socket) = state.udp_sockets.get(&handle) {
        match socket.multicast_ttl_v4() {
            Ok(multicast_ttl_v4) => DispatchOutcome::Value(result_ok(JsValue::Number(multicast_ttl_v4 as f64))),
            Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
        }
    } else {
        DispatchOutcome::Value(result_err("Invalid socket handle".to_string()))
    }
}

pub fn udp_set_ttl(args: &[JsValue]) -> DispatchOutcome {
    let handle = arg_to_u64(args, 0);
    let ttl = arg_to_usize(args, 1) as u32;
    let mut state = lock_net_state();

    if let Some(socket) = state.udp_sockets.get_mut(&handle) {
        match socket.set_ttl(ttl) {
            Ok(()) => DispatchOutcome::Value(result_ok(JsValue::Undefined)),
            Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
        }
    } else {
        DispatchOutcome::Value(result_err("Invalid socket handle".to_string()))
    }
}

pub fn udp_ttl(args: &[JsValue]) -> DispatchOutcome {
    let handle = arg_to_u64(args, 0);
    let state = lock_net_state();

    if let Some(socket) = state.udp_sockets.get(&handle) {
        match socket.ttl() {
            Ok(ttl) => DispatchOutcome::Value(result_ok(JsValue::Number(ttl as f64))),
            Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
        }
    } else {
        DispatchOutcome::Value(result_err("Invalid socket handle".to_string()))
    }
}

pub fn udp_join_multicast_v4(args: &[JsValue]) -> DispatchOutcome {
    let handle = arg_to_u64(args, 0);
    let multiaddr_str = arg_to_string(args, 1);
    let interface_str = arg_to_string(args, 2);
    let mut state = lock_net_state();

    if let Some(socket) = state.udp_sockets.get_mut(&handle) {
        let multiaddr: Result<Ipv4Addr, _> = multiaddr_str.parse();
        let interface: Result<Ipv4Addr, _> = interface_str.parse();

        match (multiaddr, interface) {
            (Ok(multiaddr), Ok(interface)) => {
                match socket.join_multicast_v4(&multiaddr, &interface) {
                    Ok(()) => DispatchOutcome::Value(result_ok(JsValue::Undefined)),
                    Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
                }
            }
            _ => DispatchOutcome::Value(result_err("Invalid IPv4 address".to_string())),
        }
    } else {
        DispatchOutcome::Value(result_err("Invalid socket handle".to_string()))
    }
}

pub fn udp_leave_multicast_v4(args: &[JsValue]) -> DispatchOutcome {
    let handle = arg_to_u64(args, 0);
    let multiaddr_str = arg_to_string(args, 1);
    let interface_str = arg_to_string(args, 2);
    let mut state = lock_net_state();

    if let Some(socket) = state.udp_sockets.get_mut(&handle) {
        let multiaddr: Result<Ipv4Addr, _> = multiaddr_str.parse();
        let interface: Result<Ipv4Addr, _> = interface_str.parse();

        match (multiaddr, interface) {
            (Ok(multiaddr), Ok(interface)) => {
                match socket.leave_multicast_v4(&multiaddr, &interface) {
                    Ok(()) => DispatchOutcome::Value(result_ok(JsValue::Undefined)),
                    Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
                }
            }
            _ => DispatchOutcome::Value(result_err("Invalid IPv4 address".to_string())),
        }
    } else {
        DispatchOutcome::Value(result_err("Invalid socket handle".to_string()))
    }
}