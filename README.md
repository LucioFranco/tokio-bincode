# tokio-bincode

[![Build Status](https://dev.azure.com/luciofranco0489/luciofranco/_apis/build/status/LucioFranco.tokio-bincode?branchName=master)](https://dev.azure.com/luciofranco0489/luciofranco/_build/latest?definitionId=1&branchName=master) ![Crates.io](https://img.shields.io/crates/v/tokio-bincode.svg) ![Crates.io](https://img.shields.io/crates/l/tokio-bincode.svg) ![Docs.rs](https://docs.rs/tokio-bincode/badge.svg)

Bincode based `tokio-codec` adapter.

## Usage

First, add this to your `Cargo.toml`:

``` toml
[dependencies]
tokio-bincode = "0.1"
```

Then you can use it like so:

``` rust
#[derive(Serialize, Deserialize)]
struct MyProtocol;

// Create the codec based on your custom protocol
let codec = BinCodec::<MyProtocol>::new();

// Frame the transport with the codec to produce a stream/sink
let (sink, stream) = Framed::new(transport, codec).split();
```

## License

This project is licensed under the [MIT license](LICENSE).

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in tokio-bincode by you, shall be licensed as MIT, without any additional
terms or conditions.



