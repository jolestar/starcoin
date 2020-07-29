// Copyright (c) The Starcoin Core Contributors
// SPDX-License-Identifier: Apache-2.0

use futures::executor;
use logger::{self, prelude::*};
use starcoin_config::{ConsensusStrategy, MinerConfig};
use starcoin_miner_client::miner::Miner;
use structopt::StructOpt;

#[derive(Debug, Clone, StructOpt, Default)]
#[structopt(name = "starcoin-miner", about = "Starcoin Miner")]
pub struct StarcoinOpt {
    #[structopt(long, short = "a", default_value = "0.0.0.0:9940")]
    pub stratum_server: String,
    #[structopt(long, short = "n", default_value = "1")]
    pub thread_num: u16,
    #[structopt(long, short = "c", default_value = "argon")]
    pub consensus: ConsensusStrategy,
}

fn main() {
    let _logger_handle = logger::init();
    let opts: StarcoinOpt = StarcoinOpt::from_args();
    let config = {
        let mut cfg = MinerConfig::default();
        cfg.enable_stderr = true;
        cfg.thread_num = opts.thread_num;
        cfg.stratum_server = opts
            .stratum_server
            .parse()
            .expect("Invalid stratum server address");
        cfg
    };
    executor::block_on(async move {
        match Miner::new(config, opts.consensus).await {
            Err(e) => error!("Start miner client failed:{:?}", e),
            Ok(mut miner_client) => miner_client.start().await,
        }
    });
}
