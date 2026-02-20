// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Test, console} from "forge-std/Test.sol";
import {EviceRollup} from "../src/EviceRollup.sol";

contract EviceRollupTest is Test {
    EviceRollup public rollup;
    
    // Setup variabel dummy untuk testing
    address public sequencer = address(0x123); // Alamat dummy Sequencer
    bytes32 public initialStateRoot = keccak256("genesis_state"); // Hash dummy

    function setUp() public {
        // Deploy kontrak EviceRollup dengan 2 argumen yang diwajibkan
        rollup = new EviceRollup(sequencer, initialStateRoot);
    }

    function test_InitialState() public view {
        // Memastikan state awal setelah deploy sudah benar
        assertEq(rollup.SEQUENCER(), sequencer);
        assertEq(rollup.currentStateRoot(), initialStateRoot);
        assertEq(rollup.currentBatchId(), 0);
    }
}