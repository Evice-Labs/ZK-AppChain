// l1-contracts/src/EviceRollup.sol
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/**
 * @title IPlonky2Verifier
 * @dev Interface untuk kontrak verifier Plonky2 yang akan di-generate nanti.
 */
interface IPlonky2Verifier {
    function verifyProof(
        bytes32 oldStateRoot,
        bytes32 newStateRoot,
        bytes calldata proof
    ) external view returns (bool);
}

/**
 * @title EviceRollup
 * @dev Kontrak utama penyelesaian (Settlement Layer) untuk Evice Intent Ecosystem.
 */
contract EviceRollup {
    // --- State Variables ---
    address public immutable SEQUENCER; 
    bytes32 public currentStateRoot;
    IPlonky2Verifier public verifier;
    uint256 public currentBatchId;
    enum IntentStatus { NONE, LOCKED, RESOLVED }
    mapping(bytes32 => IntentStatus) public intentRegistry;

    // --- Events ---
    event IntentLocked(bytes32 indexed intentId, address indexed user, uint256 amount);
    event IntentSettled(bytes32 indexed intentId, address indexed solver);
    event StateUpdated(uint256 indexed batchId, bytes32 oldStateRoot, bytes32 newStateRoot);

    // --- Errors ---
    // Standar industri menggunakan Custom Errors alih-alih require("string") untuk menghemat gas.
    error Unauthorized();
    error InvalidProof();
    error IntentNotLocked();

    // --- Modifiers ---
    
    // "Unwrapped Modifier Logic".
    // Kita memanggil fungsi internal di dalam modifier. 
    // Jika sebuah modifier digunakan berkali-kali, cara ini akan sangat menghemat ukuran kontrak (contract size).
    modifier onlySequencer() {
        if (msg.sender != SEQUENCER) revert Unauthorized();
        _;
    }

    /**
     * @param _initialSequencer Alamat wallet dari node Sequencer (Velocity) Anda.
     * @param _initialStateRoot Akar Merkle awal saat sistem pertama kali hidup.
     */
    constructor(address _initialSequencer, bytes32 _initialStateRoot) {
        SEQUENCER = _initialSequencer;
        currentStateRoot = _initialStateRoot;
        currentBatchId = 0;
    }

    /**
     * @dev User mengunci dana di L1 untuk Intent L2.
     * Ini memberikan jaminan kepada Solver bahwa dana tersedia.
     */
    function depositIntent(bytes32 _intentId) external payable {
        require(msg.value > 0, "Amount must > 0");
        intentRegistry[_intentId] = IntentStatus.LOCKED;
        emit IntentLocked(_intentId, msg.sender, msg.value);
    }

    /**
     * @dev Memperbarui kontrak Verifier (jika sirkuit Plonky2 kita di-upgrade nanti).
     */
    function setVerifier(address _verifierAddress) external onlySequencer {
        verifier = IPlonky2Verifier(_verifierAddress);
        emit VerifierUpdated(_verifierAddress);
    }

    /**
     * @dev Fungsi utama untuk L2 submit batch ke L1.
     * @param _newStateRoot Akar Merkle baru setelah transaksi L2 dieksekusi.
     * @param _proof Sertifikat kriptografi (ZK-Proof) dari Plonky2.
     */
    function updateStateWithIntents(
        bytes32 _newStateRoot, 
        bytes calldata _proof,
        bytes32[] calldata _resolvedIntentIds
    ) external onlySequencer {
        bytes32 oldRoot = currentStateRoot;

        // 1. Verifikasi ZK-Proof
        // Sirkuit ZK sekarang harus membuktikan bahwa _resolvedIntentIds benar-benar 
        // diselesaikan dengan output yang diminta user.
        if (!verifier.verifyProof(oldRoot, _newStateRoot, _proof)) revert InvalidProof();

        // 2. Tandai Intent sebagai RESOLVED dan rilis dana (logika sederhana)
        for (uint256 i = 0; i < _resolvedIntentIds.length; i++) {
            bytes32 id = _resolvedIntentIds[i];
            if (intentRegistry[id] == IntentStatus.LOCKED) {
                intentRegistry[id] = IntentStatus.RESOLVED;
                // Di sini Anda bisa menambahkan logika transfer dana ke Solver
                emit IntentSettled(id, SEQUENCER); 
            }
        }

        currentStateRoot = _newStateRoot;
        unchecked { currentBatchId++; }

        emit StateUpdated(currentBatchId, oldRoot, _newStateRoot);
    }

    // --- Internal Functions ---
    
    /**
     * @dev Logika validasi untuk modifier onlySequencer.
     */
    function _checkSequencer() internal view {
        if (msg.sender != SEQUENCER) revert Unauthorized();
    }
}