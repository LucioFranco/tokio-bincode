//! Tokio codec for use with bincode
//!
//! This crate provides a `bincode` based codec that can be used with
//! tokio's `Framed`, `FramedRead`, and `FramedWrite`.
//!
//! # Example
//!
//! ```
//! # use futures::{Stream, Sink};
//! # use tokio::io::{AsyncRead, AsyncWrite};
//! # use tokio_codec::Framed;
//! # use serde::{Serialize, Deserialize};
//! # use tokio_bincode::BinCodec;
//! # use serde_derive::{Serialize, Deserialize};
//! # fn sd<'a>(transport: impl AsyncRead + AsyncWrite) {
//! #[derive(Serialize, Deserialize)]
//! struct MyProtocol;
//!
//! // Create the codec based on your custom protocol
//! let codec = BinCodec::<MyProtocol>::new();
//!
//! // Frame the transport with the codec to produce a stream/sink
//! let (sink, stream) = Framed::new(transport, codec).split();
//! # }
//! ```

#![deny(missing_docs, missing_debug_implementations)]

use bincode::Config;
use bytes::{BufMut, BytesMut};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::io::{self, Read};
use std::marker::PhantomData;
use tokio_codec::{Decoder, Encoder};

/// Bincode based codec for use with `tokio-codec`
pub struct BinCodec<T> {
    config: Config,
    _pd: PhantomData<T>,
}

impl<T> BinCodec<T> {
    /// Provides a bincode based codec
    pub fn new() -> Self {
        let config = bincode::config();
        BinCodec::with_config(config)
    }

    /// Provides a bincode based codec from the bincode config
    pub fn with_config(config: Config) -> Self {
        BinCodec {
            config,
            _pd: PhantomData,
        }
    }
}

impl<T> Decoder for BinCodec<T>
where
    for<'de> T: Deserialize<'de>,
{
    type Item = T;
    type Error = bincode::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if !buf.is_empty() {
            let mut reader = Reader::new(&buf[..]);
            let message = self.config.deserialize_from(&mut reader)?;
            buf.split_to(reader.amount());
            Ok(Some(message))
        } else {
            Ok(None)
        }
    }
}

impl<T> Encoder for BinCodec<T>
where
    T: Serialize,
{
    type Item = T;
    type Error = bincode::Error;

    fn encode(&mut self, item: T, buf: &mut BytesMut) -> Result<(), Self::Error> {
        let size = self.config.serialized_size(&item)?;
        buf.reserve(size as usize);
        let message = self.config.serialize(&item)?;
        buf.put(&message[..]);
        Ok(())
    }
}

impl<T> fmt::Debug for BinCodec<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("BinCodec").finish()
    }
}

#[derive(Debug)]
struct Reader<'buf> {
    buf: &'buf [u8],
    amount: usize,
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Reader { buf, amount: 0 }
    }

    pub fn amount(&self) -> usize {
        self.amount
    }
}

impl<'buf, 'a> Read for &'a mut Reader<'buf> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let bytes_read = self.buf.read(buf)?;
        self.amount += bytes_read;
        Ok(bytes_read)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::{Future, Sink, Stream};
    use serde_derive::{Deserialize, Serialize};
    use std::net::SocketAddr;
    use tokio::{
        codec::Framed,
        net::{TcpListener, TcpStream},
        runtime::current_thread,
    };

    #[derive(Deserialize, Serialize, Debug, Clone, Eq, PartialEq)]
    enum Mock {
        One,
        Two,
    }

    #[test]
    fn it_works() {
        let addr = SocketAddr::new("127.0.0.1".parse().unwrap(), 15151);
        let echo = TcpListener::bind(&addr).unwrap();

        let jh = std::thread::spawn(move || {
            current_thread::run(
                echo.incoming()
                    .map_err(bincode::Error::from)
                    .take(1)
                    .for_each(|stream| {
                        let (w, r) = Framed::new(stream, BinCodec::<Mock>::new()).split();
                        r.forward(w).map(|_| ())
                    })
                    .map_err(|_| ()),
            )
        });

        let client = TcpStream::connect(&addr).wait().unwrap();
        let client = Framed::new(client, BinCodec::<Mock>::new());

        let client = client.send(Mock::One).wait().unwrap();

        let (got, client) = match client.into_future().wait() {
            Ok(x) => x,
            Err((e, _)) => panic!(e),
        };

        assert_eq!(got, Some(Mock::One));

        let client = client.send(Mock::Two).wait().unwrap();

        let (got2, client) = match client.into_future().wait() {
            Ok(x) => x,
            Err((e, _)) => panic!(e),
        };

        assert_eq!(got2, Some(Mock::Two));

        drop(client);
        jh.join().unwrap();
    }
}
