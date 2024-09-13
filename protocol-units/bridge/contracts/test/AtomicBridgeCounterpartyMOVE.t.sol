// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.22;
pragma abicoder v2;

import {Test, console} from "forge-std/Test.sol";
import {AtomicBridgeCounterpartyMOVE} from "../src/AtomicBridgeCounterpartyMOVE.sol";
import {AtomicBridgeInitiatorMOVE} from "../src/AtomicBridgeInitiatorMOVE.sol";
import {ProxyAdmin} from "@openzeppelin/contracts/proxy/transparent/ProxyAdmin.sol";
import {TransparentUpgradeableProxy} from "@openzeppelin/contracts/proxy/transparent/TransparentUpgradeableProxy.sol";
import {MockMOVEToken} from "../src/MockMOVEToken.sol";

contract AtomicBridgeCounterpartyMOVETest is Test {
    AtomicBridgeCounterpartyMOVE public atomicBridgeCounterpartyMOVEImplementation;
    AtomicBridgeCounterpartyMOVE public atomicBridgeCounterpartyMOVE;
    AtomicBridgeInitiatorMOVE public atomicBridgeInitiatorImplementation;
    AtomicBridgeInitiatorMOVE public atomicBridgeInitiator;
    MockMOVEToken public moveToken;
    ProxyAdmin public proxyAdmin;
    TransparentUpgradeableProxy public proxy;

    address public deployer = address(0x1);
    address public originator = address(1);
    address public recipient = address(0x2);
    address public otherUser = address(0x3);
    bytes32 public hashLock = keccak256(abi.encodePacked("secret"));
    uint256 public amount = 100 * 10 ** 8; // 100 MOVEToken (assuming 8 decimals)
    uint256 public timeLock = 100;
    bytes32 public initiator = keccak256(abi.encodePacked(deployer));
    bytes32 public bridgeTransferId =
        keccak256(
            abi.encodePacked(
                block.timestamp,
                initiator,
                recipient,
                amount,
                hashLock,
                timeLock
            )
        );

    function setUp() public {
        // Deploy the MOVEToken contract and mint some tokens to the deployer
        moveToken = new MockMOVEToken();
        moveToken.initialize(address(this)); // Contract will hold initial MOVE tokens

        originator = vm.addr(uint256(keccak256(abi.encodePacked(block.timestamp, block.prevrandao))));

        // Deploy the AtomicBridgeInitiatorMOVE contract
        atomicBridgeInitiatorImplementation = new AtomicBridgeInitiatorMOVE();
        proxyAdmin = new ProxyAdmin(deployer);
        proxy = new TransparentUpgradeableProxy(
            address(atomicBridgeInitiatorImplementation),
            address(proxyAdmin),
            abi.encodeWithSignature(
                "initialize(address,address)",
                address(moveToken),
                deployer
            )
        );
        atomicBridgeInitiator = AtomicBridgeInitiatorMOVE(address(proxy));

        // Deploy the AtomicBridgeCounterpartyMOVE contract
        atomicBridgeCounterpartyMOVEImplementation = new AtomicBridgeCounterpartyMOVE();
        proxy = new TransparentUpgradeableProxy(
            address(atomicBridgeCounterpartyMOVEImplementation),
            address(proxyAdmin),
            abi.encodeWithSignature(
                "initialize(address,address)",
                address(atomicBridgeInitiator),
                deployer
            )
        );
        atomicBridgeCounterpartyMOVE = AtomicBridgeCounterpartyMOVE(address(proxy));

        // Set the counterparty contract in the AtomicBridgeInitiator contract
        vm.startPrank(deployer);
        atomicBridgeInitiator.setCounterpartyAddress(
            address(atomicBridgeCounterpartyMOVE)
        );
        vm.stopPrank();
    }

    function testLockBridgeTransfer() public {
        uint256 moveAmount = 100 * 10**8;
        moveToken.transfer(originator, moveAmount); 
        vm.startPrank(originator);

        // Approve the AtomicBridgeInitiatorMOVE contract to spend MOVEToken
        moveToken.approve(address(atomicBridgeInitiator), amount);

        // Initiate the bridge transfer
        atomicBridgeInitiator.initiateBridgeTransfer(
            amount,
            initiator,
            hashLock,
            timeLock
        );

        vm.stopPrank();

        vm.startPrank(deployer);  // Only the owner (deployer) can call lockBridgeTransfer
        bool result = atomicBridgeCounterpartyMOVE.lockBridgeTransfer(
            initiator,
            bridgeTransferId,
            hashLock,
            timeLock,
            recipient,
            amount
        );
        vm.stopPrank();

        (
            bytes32 pendingInitiator,
            address pendingRecipient,
            uint256 pendingAmount,
            bytes32 pendingHashLock,
            uint256 pendingTimelock,
            AtomicBridgeCounterpartyMOVE.MessageState pendingState
        ) = atomicBridgeCounterpartyMOVE.bridgeTransfers(bridgeTransferId);

        assert(result);
        assertEq(pendingInitiator, initiator);
        assertEq(pendingRecipient, recipient);
        assertEq(pendingAmount, amount);
        assertEq(pendingHashLock, hashLock);
        assertGt(pendingTimelock, block.timestamp);
        assertEq(
            uint8(pendingState),
            uint8(AtomicBridgeCounterpartyMOVE.MessageState.PENDING)
        );
    }

    function testCompleteBridgeTransfer() public {
        bytes32 preImage = "secret";
        bytes32 testHashLock = keccak256(abi.encodePacked(preImage));

        uint256 moveAmount = 100 * 10**8;
        moveToken.transfer(originator, moveAmount); 
        vm.startPrank(originator);

        // Approve the AtomicBridgeInitiatorMOVE contract to spend MOVEToken
        moveToken.approve(address(atomicBridgeInitiator), amount);

        // Initiate the bridge transfer
        atomicBridgeInitiator.initiateBridgeTransfer(
            amount,
            initiator,
            testHashLock,
            timeLock
        );

        vm.stopPrank();

        vm.startPrank(deployer);  // Only the owner (deployer) can call lockBridgeTransfer
        atomicBridgeCounterpartyMOVE.lockBridgeTransfer(
            initiator,
            bridgeTransferId,
            testHashLock,
            timeLock,
            recipient,
            amount
        );
        vm.stopPrank();

        vm.startPrank(otherUser);

        atomicBridgeCounterpartyMOVE.completeBridgeTransfer(
            bridgeTransferId,
            preImage
        );

        (
            bytes32 completedInitiator,
            address completedRecipient,
            uint256 completedAmount,
            bytes32 completedHashLock,
            uint256 completedTimeLock,
            AtomicBridgeCounterpartyMOVE.MessageState completedState
        ) = atomicBridgeCounterpartyMOVE.bridgeTransfers(bridgeTransferId);

        assertEq(completedInitiator, initiator);
        assertEq(completedRecipient, recipient);
        assertEq(completedAmount, amount);
        assertEq(completedHashLock, testHashLock);
        assertGt(completedTimeLock, block.timestamp);
        assertEq(
            uint8(completedState),
            uint8(AtomicBridgeCounterpartyMOVE.MessageState.COMPLETED)
        );

        vm.stopPrank();
    }

function testAbortBridgeTransfer() public {
    uint256 moveAmount = 100 * 10**8;
    moveToken.transfer(originator, moveAmount); 
    vm.startPrank(originator);

    // Approve the AtomicBridgeInitiatorMOVE contract to spend MOVEToken
    moveToken.approve(address(atomicBridgeInitiator), amount);

    // Initiate the bridge transfer
    atomicBridgeInitiator.initiateBridgeTransfer(
        amount,
        initiator,
        hashLock,
        timeLock
    );

    vm.stopPrank();

    vm.startPrank(deployer);

    atomicBridgeCounterpartyMOVE.lockBridgeTransfer(
        initiator,
        bridgeTransferId,
        hashLock,
        timeLock,
        recipient,
        amount
    );

    vm.stopPrank();

    // Advance the block number to beyond the timelock period
    vm.warp(block.timestamp + timeLock + 1);

    // Try to abort as a malicious user (this should fail)
    //vm.startPrank(otherUser);
    //vm.expectRevert("Ownable: caller is not the owner");
    //atomicBridgeCounterpartyMOVE.abortBridgeTransfer(bridgeTransferId);
    //vm.stopPrank();

    // Abort as the owner (this should pass)
    vm.startPrank(deployer); // The deployer is the owner
    atomicBridgeCounterpartyMOVE.abortBridgeTransfer(bridgeTransferId);

    (
        bytes32 abortedInitiator,
        address abortedRecipient,
        uint256 abortedAmount,
        bytes32 abortedHashLock,
        uint256 abortedTimeLock,
        AtomicBridgeCounterpartyMOVE.MessageState abortedState
    ) = atomicBridgeCounterpartyMOVE.bridgeTransfers(bridgeTransferId);

    assertEq(abortedInitiator, initiator);
    assertEq(abortedRecipient, recipient);
    assertEq(abortedAmount, amount);
    assertEq(abortedHashLock, hashLock);
    assertLe(
        abortedTimeLock,
        block.timestamp,
        "Timelock is not less than or equal to current timestamp"
    );
    assertEq(
        uint8(abortedState),
        uint8(AtomicBridgeCounterpartyMOVE.MessageState.REFUNDED)
    );

    vm.stopPrank();
}


}