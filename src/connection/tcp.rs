use crate::connection::MavConnection;
use crate::{read_versioned_msg, write_versioned_msg, MavHeader, MavlinkVersion, Message};
use std::io::{self};
use std::net::ToSocketAddrs;
use std::net::{TcpListener, TcpStream};
use std::sync::Mutex;
use std::time::Duration;

/// TCP MAVLink connection

pub fn select_protocol<M: Message>(
    address: &str,
) -> io::Result<Box<dyn MavConnection<M> + Sync + Send>> {
    if address.starts_with("tcpout:") {
        Ok(Box::new(tcpout(&address["tcpout:".len()..])?))
    } else if address.starts_with("tcpin:") {
        Ok(Box::new(tcpin(&address["tcpin:".len()..])?))
    } else {
        Err(io::Error::new(
            io::ErrorKind::AddrNotAvailable,
            "Protocol unsupported",
        ))
    }
}

pub fn tcpout<T: ToSocketAddrs>(address: T) -> io::Result<TcpConnection> {
    let addr = address
        .to_socket_addrs()
        .unwrap()
        .next()
        .expect("Host address lookup failed.");
    let socket = TcpStream::connect(&addr)?;

    /* Commenting out line below to prevent "Resource temporarily unavailable" errors */
    //socket.set_read_timeout(Some(Duration::from_millis(100)))?;

    Ok(TcpConnection {
        reader: Mutex::new(socket.try_clone()?),
        writer: Mutex::new(TcpWrite {
            socket: socket,
            sequence: 0,
        }),
        protocol_version: MavlinkVersion::V2,
    })
}

pub fn tcpin<T: ToSocketAddrs>(address: T) -> io::Result<TcpConnection> {
    let addr = address
        .to_socket_addrs()
        .unwrap()
        .next()
        .expect("Invalid address");
    let listener = TcpListener::bind(&addr)?;

    //For now we only accept one incoming stream: this blocks until we get one
    for incoming in listener.incoming() {
        match incoming {
            Ok(socket) => {
                return Ok(TcpConnection {
                    reader: Mutex::new(socket.try_clone()?),
                    writer: Mutex::new(TcpWrite {
                        socket: socket,
                        sequence: 0,
                    }),
                    protocol_version: MavlinkVersion::V2,
                })
            }
            Err(e) => {
                //TODO don't println in lib
                println!("listener err: {}", e);
            }
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotConnected,
        "No incoming connections!",
    ))
}

pub struct TcpConnection {
    reader: Mutex<TcpStream>,
    writer: Mutex<TcpWrite>,
    protocol_version: MavlinkVersion,
}

struct TcpWrite {
    socket: TcpStream,
    sequence: u8,
}

impl<M: Message> MavConnection<M> for TcpConnection {
    fn recv(&self) -> Result<(MavHeader, M), crate::error::MessageReadError> {
        let mut lock = self.reader.lock().expect("tcp read failure");
        read_versioned_msg(&mut *lock, self.protocol_version)
    }

    fn send(&self, header: &MavHeader, data: &M) -> Result<usize, crate::error::MessageWriteError> {
        let mut lock = self.writer.lock().unwrap();

        let header = MavHeader {
            sequence: lock.sequence,
            system_id: header.system_id,
            component_id: header.component_id,
        };

        lock.sequence = lock.sequence.wrapping_add(1);
        write_versioned_msg(&mut lock.socket, self.protocol_version, header, data)
    }

    fn set_protocol_version(&mut self, version: MavlinkVersion) {
        self.protocol_version = version;
    }

    fn get_protocol_version(&self) -> MavlinkVersion {
        self.protocol_version
    }
}
