use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};

use crate::namespaces::value::JsValue;
use crate::namespaces::{arg_to_string, DispatchOutcome};

use super::common::{result_err, result_ok};

// IP Address utilities
pub fn parse_ip_addr(args: &[JsValue]) -> DispatchOutcome {
    let addr_str = arg_to_string(args, 0);

    match addr_str.parse::<IpAddr>() {
        Ok(ip) => {
            let ip_obj = JsValue::Object([
                ("version".to_string(), JsValue::String(match ip {
                    IpAddr::V4(_) => "v4".to_string(),
                    IpAddr::V6(_) => "v6".to_string(),
                })),
                ("addr".to_string(), JsValue::String(ip.to_string())),
                ("is_loopback".to_string(), JsValue::Bool(ip.is_loopback())),
                ("is_multicast".to_string(), JsValue::Bool(ip.is_multicast())),
                ("is_unspecified".to_string(), JsValue::Bool(ip.is_unspecified())),
            ].into_iter().collect());
            DispatchOutcome::Value(result_ok(ip_obj))
        }
        Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
    }
}

pub fn parse_ipv4_addr(args: &[JsValue]) -> DispatchOutcome {
    let addr_str = arg_to_string(args, 0);

    match addr_str.parse::<Ipv4Addr>() {
        Ok(ip) => {
            let octets = ip.octets();
            let octets_str = octets.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",");
            let octets_array = JsValue::String(octets_str);

            let ip_obj = JsValue::Object([
                ("octets".to_string(), octets_array),
                ("addr".to_string(), JsValue::String(ip.to_string())),
                ("is_loopback".to_string(), JsValue::Bool(ip.is_loopback())),
                ("is_multicast".to_string(), JsValue::Bool(ip.is_multicast())),
                ("is_broadcast".to_string(), JsValue::Bool(ip.is_broadcast())),
                ("is_private".to_string(), JsValue::Bool(ip.is_private())),
                ("is_link_local".to_string(), JsValue::Bool(ip.is_link_local())),
            ].into_iter().collect());
            DispatchOutcome::Value(result_ok(ip_obj))
        }
        Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
    }
}

pub fn parse_ipv6_addr(args: &[JsValue]) -> DispatchOutcome {
    let addr_str = arg_to_string(args, 0);

    match addr_str.parse::<Ipv6Addr>() {
        Ok(ip) => {
            let segments = ip.segments();
            let segments_str = segments.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",");
            let segments_array = JsValue::String(segments_str);

            let ip_obj = JsValue::Object([
                ("segments".to_string(), segments_array),
                ("addr".to_string(), JsValue::String(ip.to_string())),
                ("is_loopback".to_string(), JsValue::Bool(ip.is_loopback())),
                ("is_multicast".to_string(), JsValue::Bool(ip.is_multicast())),
                ("is_unspecified".to_string(), JsValue::Bool(ip.is_unspecified())),
            ].into_iter().collect());
            DispatchOutcome::Value(result_ok(ip_obj))
        }
        Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
    }
}

pub fn parse_socket_addr(args: &[JsValue]) -> DispatchOutcome {
    let addr_str = arg_to_string(args, 0);

    match addr_str.parse::<SocketAddr>() {
        Ok(socket_addr) => {
            let socket_obj = JsValue::Object([
                ("ip".to_string(), JsValue::String(socket_addr.ip().to_string())),
                ("port".to_string(), JsValue::Number(socket_addr.port() as f64)),
                ("addr".to_string(), JsValue::String(socket_addr.to_string())),
            ].into_iter().collect());
            DispatchOutcome::Value(result_ok(socket_obj))
        }
        Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
    }
}

pub fn to_socket_addrs(args: &[JsValue]) -> DispatchOutcome {
    let addr_str = arg_to_string(args, 0);

    match addr_str.to_socket_addrs() {
        Ok(addrs) => {
            let addrs_str = addrs.map(|addr| addr.to_string()).collect::<Vec<_>>().join(",");
            let addrs_array = JsValue::String(addrs_str);
            DispatchOutcome::Value(result_ok(addrs_array))
        }
        Err(e) => DispatchOutcome::Value(result_err(e.to_string())),
    }
}