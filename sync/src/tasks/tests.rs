// Copyright (c) The Starcoin Core Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::tasks::mock::{SyncNodeMocker, SyncNodeMockerFactory};
use crate::tasks::{full_sync_task, BlockCollector};
use anyhow::{format_err, Result};
use futures::channel::mpsc::unbounded;
use futures_timer::Delay;
use logger::prelude::*;
use pin_utils::core_reexport::time::Duration;
use starcoin_chain_api::ChainReader;
use starcoin_chain_mock::BlockChain;
use starcoin_genesis::Genesis;
use starcoin_storage::block_info::BlockInfoStore;
use starcoin_storage::BlockStore;
use starcoin_types::block::{Block, BlockBody, BlockHeader};
use starcoin_types::peer_info::PeerInfo;
use starcoin_vm_types::genesis_config::{BuiltinNetworkID, ChainNetwork};
use std::sync::Arc;
use test_helper::DummyNetworkService;

#[stest::test]
pub async fn test_full_sync_new_node() -> Result<()> {
    let net1 = ChainNetwork::new_builtin(BuiltinNetworkID::Test);
    let mut node1 = SyncNodeMocker::new(net1, 1, 50)?;
    node1.produce_block(10)?;

    let mut arc_node1 = Arc::new(node1);

    let net2 = ChainNetwork::new_builtin(BuiltinNetworkID::Test);

    let node2 = SyncNodeMocker::new(net2.clone(), 1, 50)?;

    let target = arc_node1.chain().get_block_info(None)?.unwrap();

    let current_block_header = node2.chain().current_header();

    let storage = node2.chain().get_storage();
    let (sender_1, receiver_1) = unbounded();
    let (sender_2, _receiver_2) = unbounded();
    let peers: Vec<PeerInfo> = Vec::new();
    let fetcher_factory_1 = Arc::new(SyncNodeMockerFactory::new(
        DummyNetworkService,
        arc_node1.clone(),
    ));
    let (sync_task, _task_handle, task_event_counter) = full_sync_task(
        current_block_header.id(),
        target.clone(),
        false,
        net2.time_service(),
        storage.clone(),
        sender_1,
        peers.clone(),
        sender_2,
        fetcher_factory_1,
    )?;
    let join_handle = node2.process_block_connect_event(receiver_1).await;
    let branch = sync_task.await?;
    let node2 = join_handle.await;
    let current_block_header = node2.chain().current_header();
    assert_eq!(branch.current_header().id(), target.block_id);
    assert_eq!(target.block_id, current_block_header.id());
    let reports = task_event_counter.get_reports();
    reports
        .iter()
        .for_each(|report| debug!("reports: {}", report));

    Arc::get_mut(&mut arc_node1).unwrap().produce_block(20)?;

    let (sender_1, receiver_1) = unbounded();
    let (sender_2, _receiver_2) = unbounded();
    let fetcher_factory_2 = Arc::new(SyncNodeMockerFactory::new(
        DummyNetworkService,
        arc_node1.clone(),
    ));
    //sync again
    let target = arc_node1.chain().get_block_info(None)?.unwrap();
    let (sync_task, _task_handle, task_event_counter) = full_sync_task(
        current_block_header.id(),
        target.clone(),
        false,
        net2.time_service(),
        storage.clone(),
        sender_1,
        peers.clone(),
        sender_2,
        fetcher_factory_2,
    )?;
    let join_handle = node2.process_block_connect_event(receiver_1).await;
    let branch = sync_task.await?;
    let node2 = join_handle.await;
    let current_block_header = node2.chain().current_header();
    assert_eq!(branch.current_header().id(), target.block_id);
    assert_eq!(target.block_id, current_block_header.id());

    let reports = task_event_counter.get_reports();
    reports
        .iter()
        .for_each(|report| debug!("reports: {}", report));

    Ok(())
}

#[stest::test]
pub async fn test_failed_block() -> Result<()> {
    let net = ChainNetwork::new_builtin(BuiltinNetworkID::Halley);
    let (storage, startup_info, _) = Genesis::init_storage_for_test(&net)?;

    let block_info = storage.get_block_info(startup_info.main)?.unwrap();
    let chain = BlockChain::new(net.time_service(), startup_info.main, storage.clone())?;
    let (sender, _) = unbounded();
    let mut block_collector =
        BlockCollector::new_with_handle(block_info, chain, sender, DummyNetworkService, true);
    let mut header = BlockHeader::random();
    header.number = 1;
    let body = BlockBody::new(Vec::new(), None);
    let failed_block = Block::new(header, body);
    let failed_block_id = failed_block.id();
    if block_collector.apply_block_for_test(failed_block).is_err() {
        assert!(storage.get_failed_block_by_id(failed_block_id)?.is_some());
        Ok(())
    } else {
        Err(format_err!("test FailedBlock fail."))
    }
}

#[stest::test]
pub async fn test_full_sync_fork() -> Result<()> {
    let net1 = ChainNetwork::new_builtin(BuiltinNetworkID::Test);
    let mut node1 = SyncNodeMocker::new(net1, 1, 50)?;
    node1.produce_block(10)?;

    let mut arc_node1 = Arc::new(node1);

    let net2 = ChainNetwork::new_builtin(BuiltinNetworkID::Test);

    let node2 = SyncNodeMocker::new(net2.clone(), 1, 50)?;

    let target = arc_node1.chain().get_block_info(None)?.unwrap();

    let current_block_header = node2.chain().current_header();

    let storage = node2.chain().get_storage();
    let (sender, receiver) = unbounded();
    let (sender_2, _receiver_2) = unbounded();
    let peers: Vec<PeerInfo> = Vec::new();
    let fetcher_factory_1 = Arc::new(SyncNodeMockerFactory::new(
        DummyNetworkService,
        arc_node1.clone(),
    ));
    let (sync_task, _task_handle, task_event_counter) = full_sync_task(
        current_block_header.id(),
        target.clone(),
        false,
        net2.time_service(),
        storage.clone(),
        sender,
        peers.clone(),
        sender_2,
        fetcher_factory_1,
    )?;
    let join_handle = node2.process_block_connect_event(receiver).await;
    let branch = sync_task.await?;
    let mut node2 = join_handle.await;
    let current_block_header = node2.chain().current_header();
    assert_eq!(branch.current_header().id(), target.block_id);
    assert_eq!(target.block_id, current_block_header.id());
    let reports = task_event_counter.get_reports();
    reports
        .iter()
        .for_each(|report| debug!("reports: {}", report));

    //test fork

    Arc::get_mut(&mut arc_node1).unwrap().produce_block(10)?;
    node2.produce_block(5)?;

    let (sender, receiver) = unbounded();
    let target = arc_node1.chain().get_block_info(None)?.unwrap();
    let (sender_2, _receiver_2) = unbounded();
    let peers: Vec<PeerInfo> = Vec::new();
    let fetcher_factory_1 = Arc::new(SyncNodeMockerFactory::new(
        DummyNetworkService,
        arc_node1.clone(),
    ));
    let (sync_task, _task_handle, task_event_counter) = full_sync_task(
        current_block_header.id(),
        target.clone(),
        false,
        net2.time_service(),
        storage,
        sender,
        peers.clone(),
        sender_2,
        fetcher_factory_1,
    )?;
    let join_handle = node2.process_block_connect_event(receiver).await;
    let branch = sync_task.await?;
    let node2 = join_handle.await;
    let current_block_header = node2.chain().current_header();
    assert_eq!(branch.current_header().id(), target.block_id);
    assert_eq!(target.block_id, current_block_header.id());

    let reports = task_event_counter.get_reports();
    reports
        .iter()
        .for_each(|report| debug!("reports: {}", report));
    Ok(())
}

#[stest::test]
pub async fn test_full_sync_fork_from_genesis() -> Result<()> {
    let net1 = ChainNetwork::new_builtin(BuiltinNetworkID::Test);
    let mut node1 = SyncNodeMocker::new(net1, 1, 50)?;
    node1.produce_block(10)?;

    let arc_node1 = Arc::new(node1);

    let net2 = ChainNetwork::new_builtin(BuiltinNetworkID::Test);

    //fork from genesis
    let mut node2 = SyncNodeMocker::new(net2.clone(), 1, 50)?;
    node2.produce_block(5)?;

    let target = arc_node1.chain().get_block_info(None)?.unwrap();

    let current_block_header = node2.chain().current_header();

    let storage = node2.chain().get_storage();
    let (sender, receiver) = unbounded();
    let (sender_2, _receiver_2) = unbounded();
    let peers: Vec<PeerInfo> = Vec::new();
    let fetcher_factory_1 = Arc::new(SyncNodeMockerFactory::new(
        DummyNetworkService,
        arc_node1.clone(),
    ));
    let (sync_task, _task_handle, task_event_counter) = full_sync_task(
        current_block_header.id(),
        target.clone(),
        false,
        net2.time_service(),
        storage.clone(),
        sender,
        peers.clone(),
        sender_2,
        fetcher_factory_1,
    )?;
    let join_handle = node2.process_block_connect_event(receiver).await;
    let branch = sync_task.await?;
    let node2 = join_handle.await;
    let current_block_header = node2.chain().current_header();
    assert_eq!(branch.current_header().id(), target.block_id);
    assert_eq!(target.block_id, current_block_header.id());
    assert_eq!(
        arc_node1.chain().current_header().id(),
        current_block_header.id()
    );
    let reports = task_event_counter.get_reports();
    reports
        .iter()
        .for_each(|report| debug!("reports: {}", report));

    Ok(())
}

#[stest::test]
pub async fn test_full_sync_continue() -> Result<()> {
    let net1 = ChainNetwork::new_builtin(BuiltinNetworkID::Test);
    let mut node1 = SyncNodeMocker::new(net1, 10, 50)?;
    node1.produce_block(10)?;

    let arc_node1 = Arc::new(node1);

    let net2 = ChainNetwork::new_builtin(BuiltinNetworkID::Test);

    //fork from genesis
    let mut node2 = SyncNodeMocker::new(net2.clone(), 1, 50)?;
    node2.produce_block(7)?;

    // first set target to 5.
    let target_block = arc_node1.chain().get_block_by_number(5)?.unwrap();
    let target = arc_node1
        .chain()
        .get_block_info(Some(target_block.id()))?
        .unwrap();

    let current_block_header = node2.chain().current_header();

    let storage = node2.chain().get_storage();
    let (sender, receiver) = unbounded();
    let (sender_2, _receiver_2) = unbounded();
    let peers: Vec<PeerInfo> = Vec::new();
    let fetcher_factory_1 = Arc::new(SyncNodeMockerFactory::new(
        DummyNetworkService,
        arc_node1.clone(),
    ));
    let (sync_task, _task_handle, task_event_counter) = full_sync_task(
        current_block_header.id(),
        target.clone(),
        false,
        net2.time_service(),
        storage.clone(),
        sender,
        peers.clone(),
        sender_2,
        fetcher_factory_1,
    )?;
    let join_handle = node2.process_block_connect_event(receiver).await;
    let branch = sync_task.await?;
    let node2 = join_handle.await;

    assert_eq!(branch.current_header().id(), target.block_id);
    let current_block_header = node2.chain().current_header();
    // node2's main chain not change.
    assert_ne!(target.block_id, current_block_header.id());

    let reports = task_event_counter.get_reports();
    reports
        .iter()
        .for_each(|report| debug!("task_report: {}", report));

    //set target to latest.
    let target = arc_node1.chain().get_block_info(None)?.unwrap();

    let (sender, receiver) = unbounded();
    //continue sync
    //TODO find a way to verify continue sync will reuse previous task local block.
    let (sender_2, _receiver_2) = unbounded();
    let peers: Vec<PeerInfo> = Vec::new();
    let fetcher_factory_1 = Arc::new(SyncNodeMockerFactory::new(
        DummyNetworkService,
        arc_node1.clone(),
    ));
    let (sync_task, _task_handle, task_event_counter) = full_sync_task(
        current_block_header.id(),
        target.clone(),
        false,
        net2.time_service(),
        storage.clone(),
        sender,
        peers.clone(),
        sender_2,
        fetcher_factory_1,
    )?;

    let join_handle = node2.process_block_connect_event(receiver).await;
    let branch = sync_task.await?;
    let node2 = join_handle.await;
    let current_block_header = node2.chain().current_header();
    assert_eq!(branch.current_header().id(), target.block_id);
    assert_eq!(target.block_id, current_block_header.id());
    assert_eq!(
        arc_node1.chain().current_header().id(),
        current_block_header.id()
    );
    let reports = task_event_counter.get_reports();
    reports
        .iter()
        .for_each(|report| debug!("reports: {}", report));

    Ok(())
}

#[stest::test]
pub async fn test_full_sync_cancel() -> Result<()> {
    let net1 = ChainNetwork::new_builtin(BuiltinNetworkID::Test);
    let mut node1 = SyncNodeMocker::new(net1, 1, 50)?;
    node1.produce_block(10)?;

    let arc_node1 = Arc::new(node1);

    let net2 = ChainNetwork::new_builtin(BuiltinNetworkID::Test);

    let node2 = SyncNodeMocker::new(net2.clone(), 10, 50)?;

    let target = arc_node1.chain().get_block_info(None)?.unwrap();

    let current_block_header = node2.chain().current_header();

    let storage = node2.chain().get_storage();
    let (sender, receiver) = unbounded();
    let (sender_2, _receiver_2) = unbounded();
    let peers: Vec<PeerInfo> = Vec::new();
    let fetcher_factory_1 = Arc::new(SyncNodeMockerFactory::new(
        DummyNetworkService,
        arc_node1.clone(),
    ));
    let (sync_task, task_handle, task_event_counter) = full_sync_task(
        current_block_header.id(),
        target.clone(),
        false,
        net2.time_service(),
        storage.clone(),
        sender,
        peers.clone(),
        sender_2,
        fetcher_factory_1,
    )?;
    let join_handle = node2.process_block_connect_event(receiver).await;
    let sync_join_handle = tokio::task::spawn(sync_task);

    Delay::new(Duration::from_millis(100)).await;

    task_handle.cancel();
    let sync_result = sync_join_handle.await?;
    assert!(sync_result.is_err());
    assert!(sync_result.err().unwrap().is_canceled());

    let node2 = join_handle.await;
    let current_block_header = node2.chain().current_header();
    assert_ne!(target.block_id, current_block_header.id());
    let reports = task_event_counter.get_reports();
    reports
        .iter()
        .for_each(|report| debug!("reports: {}", report));

    Ok(())
}

#[ignore]
#[stest::test]
pub async fn test_full_sync_by_total_difficulty() {
    //TODO add a test to verify low block number but high total difficulty.
}
