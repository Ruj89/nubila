use std::pin::Pin;

use bytes::Bytes;
use futures_util::{AsyncReadExt, AsyncWriteExt, Sink, SinkExt, Stream, TryStreamExt};
use js_sys::{Object, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_streams::{ReadableStream, WritableStream};

use crate::{
	stream_provider::{ProviderAsyncRW, ProviderUnencryptedStream},
	utils::{convert_body, object_set, ReaderStream},
	NubilaError, NubilaIoStream,
};

fn create_iostream(
	stream: Pin<Box<dyn Stream<Item = Result<Bytes, NubilaError>>>>,
	sink: Pin<Box<dyn Sink<Bytes, Error = NubilaError>>>,
) -> NubilaIoStream {
	let read = ReadableStream::from_stream(
		stream
			.map_ok(|x| Uint8Array::from(x.as_ref()).into())
			.map_err(Into::into),
	)
	.into_raw();
	let write = WritableStream::from_sink(
		sink.with(|x| async {
			convert_body(x)
				.await
				.map_err(|_| NubilaError::InvalidPayload)
				.map(|x| Bytes::from(x.0.to_vec()))
		})
		.sink_map_err(Into::into),
	)
	.into_raw();

	let out = Object::new();
	object_set(&out, "read", read.into());
	object_set(&out, "write", write.into());
	JsValue::from(out).into()
}

pub fn iostream_from_asyncrw(asyncrw: ProviderAsyncRW, buffer_size: usize) -> NubilaIoStream {
	let (rx, tx) = asyncrw.split();
	create_iostream(
		Box::pin(ReaderStream::new(Box::pin(rx), buffer_size).map_err(NubilaError::Io)),
		Box::pin(tx.into_sink().sink_map_err(NubilaError::Io)),
	)
}

pub fn iostream_from_stream(stream: ProviderUnencryptedStream) -> NubilaIoStream {
	let (rx, tx) = stream.into_split();
	create_iostream(
		Box::pin(rx.map_ok(Bytes::from).map_err(NubilaError::Wisp)),
		Box::pin(tx.sink_map_err(NubilaError::Wisp)),
	)
}
