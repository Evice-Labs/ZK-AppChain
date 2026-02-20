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
    
    // PERUBAHAN 1: Menggunakan SCREAMING_SNAKE_CASE untuk variabel immutable.
    // Ini standar industri agar developer tahu variabel ini tidak bisa diubah setelah constructor.
    address public immutable SEQUENCER; 
    
    bytes32 public currentStateRoot;
    IPlonky2Verifier public verifier;
    uint256 public currentBatchId;

    // --- Events ---
    event StateUpdated(uint256 indexed batchId, bytes32 oldStateRoot, bytes32 newStateRoot);
    event VerifierUpdated(address indexed newVerifier); // Standar: address pada event sebaiknya di-index

    // --- Errors ---
    // Standar industri menggunakan Custom Errors alih-alih require("string") untuk menghemat gas.
    error Unauthorized();
    error InvalidProof();

    // --- Modifiers ---
    
    // PERUBAHAN 2: "Unwrapped Modifier Logic".
    // Kita memanggil fungsi internal di dalam modifier. 
    // Jika sebuah modifier digunakan berkali-kali, cara ini akan sangat menghemat ukuran kontrak (contract size).
    modifier onlySequencer() {
        _checkSequencer();
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
    function updateState(bytes32 _newStateRoot, bytes calldata _proof) external onlySequencer {
        bytes32 oldRoot = currentStateRoot;

        // 1. Verifikasi ZK-Proof secara matematis
        bool isValid = verifier.verifyProof(oldRoot, _newStateRoot, _proof);
        if (!isValid) revert InvalidProof();

        // 2. Jika valid, perbarui state root Ethereum dengan data dari L2
        currentStateRoot = _newStateRoot;
        
        // Optimasi: Increment di Solidity 0.8+ lebih hemat gas menggunakan uncheck jika yakin tidak akan overflow
        unchecked {
            currentBatchId++;
        }

        // 3. Pancarkan event agar bisa dibaca oleh ekosistem
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