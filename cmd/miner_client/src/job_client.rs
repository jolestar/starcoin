use crate::JobClient;
use actix::clock::Duration;
use anyhow::Result;
use futures::stream::BoxStream;
use futures::{stream::StreamExt, Future, TryStreamExt};
use futures_channel::mpsc::{unbounded, UnboundedSender};
use futures_timer::Delay;
use logger::prelude::*;
use serde::export::Option::Some;
use starcoin_config::{RealTimeService, TimeService};
use starcoin_rpc_client::RpcClient;
use starcoin_types::system_events::MintBlockEvent;
use std::sync::Arc;

#[derive(Clone)]
pub struct JobRpcClient {
    rpc_client: Arc<RpcClient>,
    seal_sender: UnboundedSender<(Vec<u8>, u32)>,
    time_service: Arc<dyn TimeService>,
}

impl JobRpcClient {
    pub fn new(rpc_client: RpcClient) -> Self {
        let rpc_client = Arc::new(rpc_client);
        let seal_client = rpc_client.clone();
        let (seal_sender, mut seal_receiver) = unbounded();
        let fut = async move {
            while let Some((minting_blob, nonce)) = seal_receiver.next().await {
                if let Err(e) = seal_client.miner_submit(minting_blob, nonce) {
                    error!("Submit seal error: {}", e);
                    Delay::new(Duration::from_secs(1)).await;
                }
            }
        };
        Self::spawn(fut);
        Self {
            rpc_client,
            seal_sender,
            time_service: Arc::new(RealTimeService::new()),
        }
    }

    fn forward_mint_block_stream(&self) -> BoxStream<'static, MintBlockEvent> {
        let (sender, receiver) = unbounded();
        let client = self.rpc_client.clone();
        let fut = async move {
            // use a loop to retry subscribe event when connection error.
            loop {
                match client.subscribe_new_mint_blocks() {
                    Ok(stream) => {
                        let mut stream = stream.into_stream();
                        while let Some(item) = stream.next().await {
                            match item {
                                Ok(b) => {
                                    let event = MintBlockEvent::new(
                                        b.strategy,
                                        b.minting_blob,
                                        b.difficulty,
                                    );
                                    info!(
                                        "Receive mint event, minting_blob: {}, difficulty: {}",
                                        hex::encode(event.minting_blob.as_slice()),
                                        event.difficulty
                                    );
                                    let _ = sender.unbounded_send(event);
                                }
                                Err(e) => {
                                    error!("Receive error event:{}", e);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("Subscribe new blocks event error: {}, retry later.", e);
                        Delay::new(Duration::from_secs(1)).await
                    }
                }
            }
        };
        Self::spawn(fut);
        receiver.boxed()
    }

    fn spawn<F>(fut: F)
    where
        F: Future + Send + 'static,
    {
        // if we use async spawn, RpcClient will panic when try reconnection.
        // refactor this after RpcClient refactor to async.
        std::thread::spawn(move || {
            futures::executor::block_on(fut);
        });
        //async_std::task::spawn(fut);
    }
}

impl JobClient for JobRpcClient {
    fn subscribe(&self) -> Result<BoxStream<'static, MintBlockEvent>> {
        Ok(self.forward_mint_block_stream())
    }

    fn submit_seal(&self, minting_blob: Vec<u8>, nonce: u32) -> Result<()> {
        self.seal_sender.unbounded_send((minting_blob, nonce))?;
        Ok(())
    }

    fn time_service(&self) -> Arc<dyn TimeService> {
        self.time_service.clone()
    }
}
