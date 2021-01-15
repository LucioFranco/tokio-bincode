//! Tokio codec for use with bincode
//!
//! This crate provides a `bincode` based codec that can be used with
//! tokio's `Framed`, `FramedRead`, and `FramedWrite`.
//!
//! # Example
//!
//! ```
//! # use futures::{StreamExt};
//! # use tokio::io::{AsyncRead, AsyncWrite};
//! # use tokio_util::codec::Framed;
//! # use serde::{Serialize, Deserialize};
//! # use tokio_bincode::BinCodec;
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
//!
//! # Features
//!
//! This crate provides a single feature, `big_data`, which enables large amounts of data
//! to be encoded by prepending the length of the data to the data itself,
//! using tokio's `LengthDelimitedCodec`.
//!
//! This functionality is optional because it might affect performance.

#![deny(missing_docs, missing_debug_implementations)]

use bincode::{DefaultOptions, Options};
use bytes::BytesMut;
use serde::{Deserialize, Serialize};
use std::{fmt, marker::PhantomData};
use tokio_util::codec::{Decoder, Encoder};

#[cfg(feature = "big_data")]
use tokio_util::codec::length_delimited::{Builder, LengthDelimitedCodec};

/// Bincode based codec for use with `tokio-codec`
///
/// # Note
///
/// Optionally depends on [`LengthDelimitedCodec`](https://docs.rs/tokio/0.1/tokio/codec/length_delimited/struct.LengthDelimitedCodec.html)
/// when `big_data` feature is enabled
pub struct BinCodecWithOptions<T, O> {
    #[cfg(feature = "big_data")]
    lower: LengthDelimitedCodec,
    config: O,
    _pd: PhantomData<T>,
}

/// BinCodec with DefaultOptions
pub type BinCodec<T> = BinCodecWithOptions<T, DefaultOptions>;

impl<T> BinCodec<T> {
    /// Provides a bincode based codec with default options
    pub fn new() -> Self { Self::default() }
}

/// Creates a new BinCodec with the default options
pub fn new<T>() -> BinCodec<T> { Default::default() }

impl<T, O> BinCodecWithOptions<T, O> {
    /// Provides a bincode based codec from the bincode config
    #[cfg(not(feature = "big_data"))]
    pub fn with_config(config: O) -> Self { BinCodec { config, _pd: PhantomData } }

    /// Provides a bincode based codec from the bincode config and a `LengthDelimitedCodec` builder
    #[cfg(feature = "big_data")]
    pub fn with_config(config: O, builder: &mut Builder) -> Self {
        BinCodecWithOptions { lower: builder.new_codec(), config, _pd: PhantomData }
    }
}

impl<T> Default for BinCodec<T> {
    #[inline]
    fn default() -> Self {
        let config = bincode::options();
        BinCodecWithOptions::with_config(
            config,
            #[cfg(feature = "big_data")]
            &mut Builder::new(),
        )
    }
}

impl<T, O> Decoder for BinCodecWithOptions<T, O>
where
    for<'de> T: Deserialize<'de>,
    O: Options + Clone,
{
    type Error = bincode::Error;
    type Item = T;

    #[cfg(feature = "big_data")]
    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        Ok(if let Some(buf) = self.lower.decode(src)? {
            Some(self.config.clone().deserialize(&buf)?)
        } else {
            None
        })
    }

    #[cfg(not(feature = "big_data"))]
    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        use bytes::Buf;
        if !buf.is_empty() {
            let mut reader = reader::Reader::new(&buf[..]);
            let message = self.config.clone().deserialize_from(&mut reader)?;
            let amount = reader.amount();
            buf.advance(amount);
            Ok(Some(message))
        } else {
            Ok(None)
        }
    }
}

impl<T, O> Encoder<T> for BinCodecWithOptions<T, O>
where
    T: Serialize,
    O: Options + Clone,
{
    type Error = bincode::Error;

    #[cfg(feature = "big_data")]
    fn encode(&mut self, item: T, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let bytes = self.config.clone().serialize(&item)?;
        self.lower.encode(bytes.into(), dst)?;
        Ok(())
    }

    #[cfg(not(feature = "big_data"))]
    fn encode(&mut self, item: T, buf: &mut BytesMut) -> Result<(), Self::Error> {
        use bytes::BufMut;
        let size = self.config.clone().serialized_size(&item)?;
        buf.reserve(size as usize);
        let message = self.config.clone().serialize(&item)?;
        buf.put(&message[..]);
        Ok(())
    }
}

impl<T, O> fmt::Debug for BinCodecWithOptions<T, O> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { f.debug_struct("BinCodec").finish() }
}

#[cfg(not(feature = "big_data"))]
mod reader {
    use std::io::{self, Read};

    #[derive(Debug)]
    pub struct Reader<'buf> {
        buf: &'buf [u8],
        amount: usize,
    }

    impl<'buf> Reader<'buf> {
        pub fn new(buf: &'buf [u8]) -> Self { Reader { buf, amount: 0 } }

        pub fn amount(&self) -> usize { self.amount }
    }

    impl<'buf, 'a> Read for &'a mut Reader<'buf> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let bytes_read = self.buf.read(buf)?;
            self.amount += bytes_read;
            Ok(bytes_read)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::{SinkExt, TryStreamExt};
    use serde::{Deserialize, Serialize};
    use tokio::net::UnixStream;
    use tokio_util::codec::{FramedRead, FramedWrite};

    #[tokio::test]
    async fn it_works() {
        #[derive(Deserialize, Serialize, Debug, Clone, Eq, PartialEq)]
        enum Mock {
            One,
            Two,
        }

        let (send, recv) = UnixStream::pair().expect("couldn't get unix stream");
        let recv = FramedRead::new(recv, BinCodec::<Mock>::new());
        let mut send = FramedWrite::new(send, BinCodec::<Mock>::new());
        futures::pin_mut!(recv);

        send.send(Mock::One).await.expect("could not send message 1 to server");

        let got = match recv.try_next().await {
            Ok(x) => x,
            Err(e) => panic!("[Mock::One]> Error during deserialize: {:?}", e),
        };

        assert_eq!(got, Some(Mock::One));

        send.send(Mock::Two).await.expect("could not send message 2 to server");

        let got = match recv.try_next().await {
            Ok(x) => x,
            Err(e) => panic!("[Mock::Two]> Error during deserialize: {:?}", e),
        };

        assert_eq!(got, Some(Mock::Two));
    }

    #[cfg(feature = "big_data")]
    #[tokio::test]
    async fn big_data() {
        #[derive(Deserialize, Serialize, Debug, Clone, Eq, PartialEq)]
        enum Mock {
            One(Vec<u8>),
            Two,
        }

        let (send, recv) = UnixStream::pair().expect("couldn't get unix stream");
        let recv = FramedRead::new(recv, BinCodec::<Mock>::new());
        let mut send = FramedWrite::new(send, BinCodec::<Mock>::new());
        futures::pin_mut!(recv);

        let data = Mock::Two;
        send.send(data.clone()).await.expect("could not send message 2 to server");

        let got = match recv.try_next().await {
            Ok(x) => x,
            Err(e) => panic!("[Mock::Two]> Error during deserialize: {:?}", e),
        };

        assert_eq!(got, Some(data));

        let data = Mock::One(vec![0; 1024 * 1_024 + 1]);
        let task = {
            let data = data.clone();
            async move {
                send.send(data).await.expect("could not send message 1 to server");
            }
        };
        let _ = tokio::spawn(task);

        let got = match recv.try_next().await {
            Ok(x) => x,
            Err(e) => panic!("[Mock::One]> Error during deserialize: {:?}", e),
        };

        assert_eq!(got, Some(data));
    }
}
