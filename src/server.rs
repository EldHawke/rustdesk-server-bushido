use hbb_common::ResultType;
use std::net::IpAddr;

use crate::RendezvousServer;

pub async fn start(
    bind_addr: Option<IpAddr>,
    port: i32,
    serial: i32,
    key: &str,
    rmem: usize,
) -> ResultType<()> {

    RendezvousServer::start_with_bind(
        bind_addr,
        port,
        serial,
        key,
        rmem,
    ).await
}