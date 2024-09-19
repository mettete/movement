

// Specifications for the atomic_bridge_initiator module
spec atomic_bridge::atomic_bridge_initiator {
    
    //  Spec for initiate_bridge_transfer
    //  This is not a complete spec, but it is a good start.
    spec initiate_bridge_transfer {

        //  ensures that abort conditions enforce abort, but 
        //  the does not cpature all the possible abort conditions.
        pragma aborts_if_is_partial;
        // The function should abort if the amount is 0
        aborts_if amount == 0;
        // let originator_addr = signer::address_of(originator);
        // let asset = moveth::metadata();
        // let x =  primary_fungible_store::balance(originator_addr, asset); // , EINSUFFICIENT_BALANCE);
        // aborts_if primary_fungible_store::balance(originator_addr, asset) < amount; // , EINSUFFICIENT_BALANCE);

        //  the store nonce is incremented by 1
        let config_address = borrow_global<BridgeConfig>(@atomic_bridge).bridge_module_deployer;
        ensures borrow_global_mut<BridgeTransferStore>(config_address).nonce == old(borrow_global_mut<BridgeTransferStore>(config_address).nonce) + 1;

    }

    spec complete_bridge_transfer {

        pragma aborts_if_is_partial = true;

        let config_address = borrow_global<BridgeConfig>(@atomic_bridge).bridge_module_deployer;
        let store = borrow_global_mut<BridgeTransferStore>(config_address);
        let bridge_transfer = aptos_std::smart_table::spec_get(store.transfers, bridge_transfer_id);

        //  The function should abort if the bridge transfer is not in the INITIALIZED state
        aborts_if bridge_transfer.state != INITIALIZED with ENOT_INITIALIZED;
        // Should abort fi the pre-image is not the hash of the lock
        aborts_if aptos_std::aptos_hash::spec_keccak256(bcs::to_bytes(pre_image)) != bridge_transfer.hash_lock with EWRONG_PREIMAGE;
        // Should abort if the time lock has expired
        aborts_if timestamp::now_seconds() > bridge_transfer.time_lock with ETIMELOCK_EXPIRED;
        //  Should abort if the bridge transfer ID is not in the store
        aborts_if !aptos_std::smart_table::spec_contains(store.transfers,  bridge_transfer_id) with aptos_std::smart_table::ENOT_FOUND;
        //  Other causes of aborts
        aborts_with EXECUTION_FAILURE, 0x6507;

        //  The state of the bridge transfer is set to COMPLETED in the new store.
        ensures aptos_std::smart_table::spec_get(borrow_global_mut<BridgeTransferStore>(config_address).transfers, bridge_transfer_id).state == COMPLETED;
    }

}

