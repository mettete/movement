module bridge_asset::bridge_asset_tests{
    use std::signer;
    use aptos_framework::primary_fungible_store;
    use aptos_framework::dispatchable_fungible_asset;
    use aptos_framework::fungible_asset::{Self, FungibleStore};
    use bridge_asset::bridge_asset;
    use aptos_framework::object;

    #[test(creator = @bridge_asset, minter = @0xface, admin = @admin, master_minter = @master_minter, denylister = @0xcade)]
    fun test_basic_flow(creator: &signer, minter: &signer, admin: &signer, master_minter: &signer, denylister: &signer) {
        bridge_asset::init_for_test(creator);
        let receiver_address = @0xcafe1;
        let minter_address = signer::address_of(minter);

        bridge_asset::mint(admin, minter_address, 100);
        let asset = bridge_asset::metadata();
        assert!(primary_fungible_store::balance(minter_address, asset) == 100, 0);

        // transfer from minter to receiver, check balance
        let minter_store = primary_fungible_store::ensure_primary_store_exists(minter_address, asset);
        let receiver_store = primary_fungible_store::ensure_primary_store_exists(receiver_address, asset);
        dispatchable_fungible_asset::transfer(minter, minter_store, receiver_store, 10);

        // denylist account, check if account is denylisted
        bridge_asset::denylist(denylister, receiver_address);
        assert!(primary_fungible_store::is_frozen(receiver_address, asset), 0);
        bridge_asset::undenylist(denylister, receiver_address);
        assert!(!primary_fungible_store::is_frozen(receiver_address, asset), 0);

        // burn tokens, check balance
        bridge_asset::burn(admin, minter_address, 90);
        assert!(primary_fungible_store::balance(minter_address, asset) == 0, 0);
    }

    //test the ability of a denylisted account to transfer out newly created store
    #[test(creator = @bridge_asset, denylister = @0xcade, receiver = @0xdead)]
    #[expected_failure(abort_code = 327683, location = aptos_framework::object)]
    fun test_untransferrable_store(creator: &signer, denylister: &signer, receiver: &signer) {
        bridge_asset::init_for_test(creator);
        let receiver_address = signer::address_of(receiver);
        let asset = bridge_asset::metadata();

        bridge_asset::denylist(denylister, receiver_address);
        assert!(primary_fungible_store::is_frozen(receiver_address, asset), 0);

        let constructor_ref = object::create_object(receiver_address);
        fungible_asset::create_store(&constructor_ref, asset);
        let store = object::object_from_constructor_ref<FungibleStore>(&constructor_ref);

        object::transfer(receiver, store, @0xdeadbeef);
    }
}