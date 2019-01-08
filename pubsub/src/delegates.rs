use std::marker::PhantomData;
use std::sync::Arc;
use std::collections::HashMap;

use subscription::{Subscriber, new_subscription};
use handler::{SubscribeRpcMethod, UnsubscribeRpcMethod};
use types::{SubscriptionId, PubSubMetadata};
use core::{self, Params, Value, Error, Metadata, RemoteProcedure, RpcMethod};
use core::futures::IntoFuture;

struct DelegateSubscription<T, F> {
	delegate: Arc<T>,
	closure: F,
}

impl<T, M, F> SubscribeRpcMethod<M> for DelegateSubscription<T, F> where
	M: PubSubMetadata,
	F: Fn(&T, Params, M, Subscriber),
	T: Send + Sync + 'static,
	F: Send + Sync + 'static,
{
	fn call(&self, params: Params, meta: M, subscriber: Subscriber) {
		let closure = &self.closure;
		closure(&self.delegate, params, meta, subscriber)
	}
}

impl<M, T, F, I> UnsubscribeRpcMethod<M> for DelegateSubscription<T, F> where
	M: PubSubMetadata,
	F: Fn(&T, SubscriptionId, M) -> I,
	I: IntoFuture<Item = Value, Error = Error>,
	T: Send + Sync + 'static,
	F: Send + Sync + 'static,
	I::Future: Send + 'static,
{
	type Out = I::Future;
	fn call(&self, id: SubscriptionId, meta: M) -> Self::Out {
		let closure = &self.closure;
		closure(&self.delegate, id, meta).into_future()
	}
}

/// Wire up rpc subscriptions to `delegate` struct
pub struct IoDelegate<T, M = ()> where
	T: Send + Sync + 'static,
	M: Metadata,
{
	inner: core::IoDelegate<T, M>,
	delegate: Arc<T>,
	_data: PhantomData<M>,
}

impl<T, M> IoDelegate<T, M> where
	T: Send + Sync + 'static,
	M: PubSubMetadata,
{
	/// Creates new `PubSubIoDelegate`, wrapping the core IoDelegate
	pub fn new(delegate: Arc<T>) -> Self {
		IoDelegate {
			inner: core::IoDelegate::new(delegate.clone()),
			delegate,
			_data: PhantomData,
		}
	}

	/// Adds subscription to the delegate.
	pub fn add_subscription<Sub, Unsub, I>(
		&mut self,
		name: &str,
		subscribe: (&str, Sub),
		unsubscribe: (&str, Unsub),
	) where
		Sub: Fn(&T, Params, M, Subscriber),
		Sub: Send + Sync + 'static,
		Unsub: Fn(&T, SubscriptionId, M) -> I,
		I: IntoFuture<Item = Value, Error = Error>,
		Unsub: Send + Sync + 'static,
		I::Future: Send + 'static,
	{
		let (sub, unsub) = new_subscription(
			name,
			DelegateSubscription {
				delegate: self.delegate.clone(),
				closure: subscribe.1,
			},
			DelegateSubscription {
				delegate: self.delegate.clone(),
				closure: unsubscribe.1,
			}
		);
		self.inner.add_method_with_meta(subscribe.0, move |_, params, meta| sub.call(params, meta));
		self.inner.add_method_with_meta(unsubscribe.0, move |_, params, meta| unsub.call(params, meta));
	}

	/// Adds an alias to existing method.
	pub fn add_alias(&mut self, from: &str, to: &str) {
		self.inner.add_alias(from, to)
	}
}

impl<T, M> Into<HashMap<String, RemoteProcedure<M>>> for IoDelegate<T, M> where
	T: Send + Sync + 'static,
	M: Metadata,
{
	fn into(self) -> HashMap<String, RemoteProcedure<M>> {
		self.inner.into()
	}
}
