use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::time::Duration;

use crate::namespaces::value::RuntimeValue;
use crate::namespaces::{DispatchOutcome, arg_to_string, arg_to_u64, arg_to_usize};

use super::common::{lock_net_state, result_err, result_ok, with_net_state_mut};

// TcpListener functions
pub fn tcp_listen(args: &[RuntimeValue]) -> DispatchOutcome {
    let addr_str = arg_to_string(args, 0);

    match TcpListener::bind(&addr_str) {
        Ok(listener) => {
            let handle = with_net_state_mut(|state| state.insert_tcp_listener(listener));
            DispatchOutcome::Value(result_ok(RuntimeValue::Number(handle as f64)))
        }
        Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
    }
}

pub fn tcp_accept(args: &[RuntimeValue]) -> DispatchOutcome {
    let handle = arg_to_u64(args, 0);
    let net_state = lock_net_state();
    let mut state = net_state.lock().unwrap();

    if let Some(listener) = state.tcp_listeners.get(&handle) {
        match listener.accept() {
            Ok((stream, addr)) => {
                let stream_handle = state.insert_tcp_stream(stream);
                let connection = RuntimeValue::Object(
                    [
                        (
                            "stream".to_string(),
                            RuntimeValue::Number(stream_handle as f64),
                        ),
                        (
                            "peer_addr".to_string(),
                            RuntimeValue::String(addr.to_string()),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                );
                DispatchOutcome::Value(result_ok(connection))
            }
            Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
        }
    } else {
        DispatchOutcome::Value(result_err("Invalid listener handle".to_string()))
    }
}

pub fn tcp_local_addr(args: &[RuntimeValue]) -> DispatchOutcome {
    let handle = arg_to_u64(args, 0);
    let net_state = lock_net_state();
    let state = net_state.lock().unwrap();

    if let Some(listener) = state.tcp_listeners.get(&handle) {
        match listener.local_addr() {
            Ok(addr) => DispatchOutcome::Value(result_ok(RuntimeValue::String(addr.to_string()))),
            Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
        }
    } else {
        DispatchOutcome::Value(result_err("Invalid listener handle".to_string()))
    }
}

// TcpStream functions
pub fn tcp_connect(args: &[RuntimeValue]) -> DispatchOutcome {
    let addr_str = arg_to_string(args, 0);

    match TcpStream::connect(&addr_str) {
        Ok(stream) => {
            let handle = with_net_state_mut(|state| state.insert_tcp_stream(stream));
            DispatchOutcome::Value(result_ok(RuntimeValue::Number(handle as f64)))
        }
        Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
    }
}

pub fn tcp_read(args: &[RuntimeValue]) -> DispatchOutcome {
    let handle = arg_to_u64(args, 0);
    let max_bytes = arg_to_usize(args, 1).max(1).min(65536);
    let net_state = lock_net_state();
    let mut state = net_state.lock().unwrap();

    if let Some(stream) = state.tcp_streams.get_mut(&handle) {
        let mut buffer = vec![0; max_bytes];
        match stream.read(&mut buffer) {
            Ok(n) => {
                buffer.truncate(n);
                let data = String::from_utf8_lossy(&buffer).to_string();
                DispatchOutcome::Value(result_ok(RuntimeValue::String(data)))
            }
            Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
        }
    } else {
        DispatchOutcome::Value(result_err("Invalid stream handle".to_string()))
    }
}

pub fn tcp_write(args: &[RuntimeValue]) -> DispatchOutcome {
    let handle = arg_to_u64(args, 0);
    let data = arg_to_string(args, 1);
    let net_state = lock_net_state();
    let mut state = net_state.lock().unwrap();

    if let Some(stream) = state.tcp_streams.get_mut(&handle) {
        match stream.write(data.as_bytes()) {
            Ok(n) => DispatchOutcome::Value(result_ok(RuntimeValue::Number(n as f64))),
            Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
        }
    } else {
        DispatchOutcome::Value(result_err("Invalid stream handle".to_string()))
    }
}

pub fn tcp_flush(args: &[RuntimeValue]) -> DispatchOutcome {
    let handle = arg_to_u64(args, 0);
    let net_state = lock_net_state();
    let mut state = net_state.lock().unwrap();

    if let Some(stream) = state.tcp_streams.get_mut(&handle) {
        match stream.flush() {
            Ok(()) => DispatchOutcome::Value(result_ok(RuntimeValue::Undefined)),
            Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
        }
    } else {
        DispatchOutcome::Value(result_err("Invalid stream handle".to_string()))
    }
}

pub fn tcp_shutdown(args: &[RuntimeValue]) -> DispatchOutcome {
    let handle = arg_to_u64(args, 0);
    let how_str = arg_to_string(args, 1);
    let net_state = lock_net_state();
    let mut state = net_state.lock().unwrap();

    let shutdown_how = match how_str.as_str() {
        "Read" => Shutdown::Read,
        "Write" => Shutdown::Write,
        "Both" => Shutdown::Both,
        _ => return DispatchOutcome::Value(result_err("Invalid shutdown method".to_string())),
    };

    if let Some(stream) = state.tcp_streams.get_mut(&handle) {
        match stream.shutdown(shutdown_how) {
            Ok(()) => DispatchOutcome::Value(result_ok(RuntimeValue::Undefined)),
            Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
        }
    } else {
        DispatchOutcome::Value(result_err("Invalid stream handle".to_string()))
    }
}

pub fn tcp_peer_addr(args: &[RuntimeValue]) -> DispatchOutcome {
    let handle = arg_to_u64(args, 0);
    let net_state = lock_net_state();
    let state = net_state.lock().unwrap();

    if let Some(stream) = state.tcp_streams.get(&handle) {
        match stream.peer_addr() {
            Ok(addr) => DispatchOutcome::Value(result_ok(RuntimeValue::String(addr.to_string()))),
            Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
        }
    } else {
        DispatchOutcome::Value(result_err("Invalid stream handle".to_string()))
    }
}

pub fn tcp_set_read_timeout(args: &[RuntimeValue]) -> DispatchOutcome {
    let handle = arg_to_u64(args, 0);
    let timeout_ms = arg_to_u64(args, 1);
    let net_state = lock_net_state();
    let mut state = net_state.lock().unwrap();

    if let Some(stream) = state.tcp_streams.get_mut(&handle) {
        let timeout = if timeout_ms == 0 {
            None
        } else {
            Some(Duration::from_millis(timeout_ms))
        };

        match stream.set_read_timeout(timeout) {
            Ok(()) => DispatchOutcome::Value(result_ok(RuntimeValue::Undefined)),
            Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
        }
    } else {
        DispatchOutcome::Value(result_err("Invalid stream handle".to_string()))
    }
}

pub fn tcp_set_write_timeout(args: &[RuntimeValue]) -> DispatchOutcome {
    let handle = arg_to_u64(args, 0);
    let timeout_ms = arg_to_u64(args, 1);
    let net_state = lock_net_state();
    let mut state = net_state.lock().unwrap();

    if let Some(stream) = state.tcp_streams.get_mut(&handle) {
        let timeout = if timeout_ms == 0 {
            None
        } else {
            Some(Duration::from_millis(timeout_ms))
        };

        match stream.set_write_timeout(timeout) {
            Ok(()) => DispatchOutcome::Value(result_ok(RuntimeValue::Undefined)),
            Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
        }
    } else {
        DispatchOutcome::Value(result_err("Invalid stream handle".to_string()))
    }
}

pub fn tcp_set_nodelay(args: &[RuntimeValue]) -> DispatchOutcome {
    let handle = arg_to_u64(args, 0);
    let nodelay = matches!(args.get(1), Some(RuntimeValue::Bool(true)));
    let net_state = lock_net_state();
    let mut state = net_state.lock().unwrap();

    if let Some(stream) = state.tcp_streams.get_mut(&handle) {
        match stream.set_nodelay(nodelay) {
            Ok(()) => DispatchOutcome::Value(result_ok(RuntimeValue::Undefined)),
            Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
        }
    } else {
        DispatchOutcome::Value(result_err("Invalid stream handle".to_string()))
    }
}

pub fn tcp_nodelay(args: &[RuntimeValue]) -> DispatchOutcome {
    let handle = arg_to_u64(args, 0);
    let net_state = lock_net_state();
    let state = net_state.lock().unwrap();

    if let Some(stream) = state.tcp_streams.get(&handle) {
        match stream.nodelay() {
            Ok(nodelay) => DispatchOutcome::Value(result_ok(RuntimeValue::Bool(nodelay))),
            Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
        }
    } else {
        DispatchOutcome::Value(result_err("Invalid stream handle".to_string()))
    }
}

pub fn tcp_set_ttl(args: &[RuntimeValue]) -> DispatchOutcome {
    let handle = arg_to_u64(args, 0);
    let ttl = arg_to_usize(args, 1) as u32;
    let net_state = lock_net_state();
    let mut state = net_state.lock().unwrap();

    if let Some(stream) = state.tcp_streams.get_mut(&handle) {
        match stream.set_ttl(ttl) {
            Ok(()) => DispatchOutcome::Value(result_ok(RuntimeValue::Undefined)),
            Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
        }
    } else {
        DispatchOutcome::Value(result_err("Invalid stream handle".to_string()))
    }
}

pub fn tcp_ttl(args: &[RuntimeValue]) -> DispatchOutcome {
    let handle = arg_to_u64(args, 0);
    let net_state = lock_net_state();
    let state = net_state.lock().unwrap();

    if let Some(stream) = state.tcp_streams.get(&handle) {
        match stream.ttl() {
            Ok(ttl) => DispatchOutcome::Value(result_ok(RuntimeValue::Number(ttl as f64))),
            Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
        }
    } else {
        DispatchOutcome::Value(result_err("Invalid stream handle".to_string()))
    }
}
