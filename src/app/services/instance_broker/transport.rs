use super::protocol::{BrokerResponse, LaunchRequest, MAX_FRAME_BYTES};
use interprocess::local_socket::{GenericNamespaced, ListenerOptions, Name, Stream, prelude::*};
use std::io::{self, Read, Write};
#[cfg(not(target_os = "windows"))]
use std::time::Duration;

#[cfg(not(target_os = "windows"))]
const CONNECTION_TIMEOUT: Duration = Duration::from_millis(500);

pub(super) fn local_socket_name(endpoint: &str) -> io::Result<Name<'static>> {
    endpoint
        .to_ns_name::<GenericNamespaced>()
        .map(Name::into_owned)
}

pub(super) fn bind_listener(endpoint: &str) -> io::Result<interprocess::local_socket::Listener> {
    ListenerOptions::new()
        .name(local_socket_name(endpoint)?)
        .reclaim_name(true)
        .create_sync()
}

pub(super) fn connect(endpoint: &str) -> io::Result<Stream> {
    let stream = Stream::connect(local_socket_name(endpoint)?)?;
    #[cfg(not(target_os = "windows"))]
    configure_timeouts(&stream)?;
    Ok(stream)
}

pub(super) fn send_request(
    stream: &mut Stream,
    request: &LaunchRequest,
) -> io::Result<BrokerResponse> {
    write_frame(stream, &request.encode()?)?;
    BrokerResponse::decode(&read_frame(stream)?)
}

pub(super) fn receive_request(stream: &mut Stream) -> io::Result<LaunchRequest> {
    #[cfg(not(target_os = "windows"))]
    configure_timeouts(stream)?;
    LaunchRequest::decode(&read_frame(stream)?)
}

#[cfg(not(target_os = "windows"))]
fn configure_timeouts(stream: &Stream) -> io::Result<()> {
    stream.set_recv_timeout(Some(CONNECTION_TIMEOUT))?;
    stream.set_send_timeout(Some(CONNECTION_TIMEOUT))
}

pub(super) fn send_response(stream: &mut Stream, response: &BrokerResponse) -> io::Result<()> {
    write_frame(stream, &response.encode()?)
}

fn write_frame(stream: &mut Stream, frame: &[u8]) -> io::Result<()> {
    stream.write_all(frame)?;
    stream.flush()
}

fn read_frame(stream: &mut Stream) -> io::Result<Vec<u8>> {
    let mut prefix = [0_u8; 4];
    stream.read_exact(&mut prefix)?;
    let length = u32::from_le_bytes(prefix) as usize;
    if length > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "broker frame exceeds the size limit",
        ));
    }
    let mut frame = vec![0_u8; length + 4];
    frame[..4].copy_from_slice(&prefix);
    stream.read_exact(&mut frame[4..])?;
    Ok(frame)
}
